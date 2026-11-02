//! The long-lived task behind one kyu profile: poll, deliver, record,
//! report, pause, forever, until told to stop.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::broadcast;

use super::pause::{announce_pause, resume, resume_after_recheck, PAUSED_POLL_MS};
use super::transition::note_transition;
use super::{HUB_DOWN_PAUSE_MS, IDLE_PAUSE_MS};
use crate::adapters::{Clock, Sink};
use crate::config::Profile;
use crate::obs::{self, Registry};
use crate::pump::streak::REFUSAL_DETAIL;
use crate::pump::{pump_once, Health, Hub, PumpState, Step};
use crate::reload::ProfileStore;

/// Everything a pump task shares with the rest of the service.
pub(super) struct PumpContext {
    pub(super) hub: Arc<dyn Hub>,
    pub(super) profiles: Arc<ProfileStore>,
    pub(super) sink: Arc<dyn Sink>,
    pub(super) clock: Arc<dyn Clock>,
    pub(super) registry: Arc<Registry>,
    /// The self-report topic, when one is configured.
    pub(super) reporting: Option<String>,
}

/// Run the pump of `profile` until `stop` fires.
pub(super) async fn run(
    mut profile: Arc<Profile>,
    ctx: PumpContext,
    mut stop: broadcast::Receiver<()>,
) {
    let mut state = PumpState::default();
    loop {
        if stop.try_recv().is_ok() {
            return;
        }
        adopt_reload(&ctx.profiles, &ctx.registry, &mut profile, &mut state);
        adopt_registry_health(&ctx.registry, &profile.name, &mut state);
        let before = state.health;
        let was_paused = state.paused;
        let started = Instant::now();
        let step = pump_once(
            &profile,
            ctx.hub.as_ref(),
            ctx.sink.as_ref(),
            ctx.clock.as_ref(),
            &mut state,
        )
        .await;
        let duration_ms = started.elapsed().as_millis() as u64;

        record_step(
            &ctx.registry,
            &profile,
            &step,
            duration_ms,
            before,
            state.health,
            state.first_sight,
        );
        announce_pause(&ctx.registry, &profile.name, &state, was_paused);
        note_transition(
            ctx.registry.as_ref(),
            ctx.sink.as_ref(),
            ctx.reporting.as_deref(),
            &profile.name,
            before,
            state.health,
            transition_detail(&state),
        )
        .await;
        pause_after(ctx.clock.as_ref(), &step).await;
    }
}

/// feat-reload-1: pick up a reloaded profile between messages, never
/// during one. A new lease or attempt cap is a hub-side policy, so it is
/// written again.
fn adopt_reload(
    profiles: &ProfileStore,
    registry: &Registry,
    profile: &mut Arc<Profile>,
    state: &mut PumpState,
) {
    let Some(now) = profiles.get(&profile.name) else {
        return;
    };
    if Arc::ptr_eq(&now, profile) {
        return;
    }
    if now.lease_ms != profile.lease_ms || now.max_attempts != profile.max_attempts {
        // A push that already succeeded means the subscription exists, so
        // the new policy goes in before the next poll, not after it.
        state.policy_before_poll |= state.policy_pushed;
        state.policy_pushed = false;
        state.policy_last_try = None;
    }
    // A reload may have fixed the destination: try claiming again.
    registry.set_paused(&now.name, false);
    resume(&now.name, state, "config reload");
    *profile = now;
}

/// A "Recheck" resets the registry to `Starting` from outside the pump.
/// The pump compares against what the registry says now, so a profile that
/// fails again after a recheck is a transition again (and is noted again)
/// instead of looking unchanged to a pump that still remembers `Failing`.
fn adopt_registry_health(registry: &Registry, name: &str, state: &mut PumpState) {
    resume_after_recheck(registry, name, state);
    if let Some(current) = registry.health_of(name) {
        state.health = current;
    }
}

/// Count and log what one pump step did.
fn record_step(
    registry: &Registry,
    profile: &Profile,
    step: &Step,
    duration_ms: u64,
    before: Health,
    after: Health,
    first_sight: bool,
) {
    match step {
        Step::Delivered {
            attempts,
            ack_failed,
            ..
        } => record_delivered(
            registry,
            profile,
            (duration_ms, first_sight),
            *attempts,
            *ack_failed,
        ),
        Step::HandedBack {
            attempts, reason, ..
        } => record_handed_back(
            registry,
            profile,
            (duration_ms, first_sight),
            *attempts,
            reason,
        ),
        Step::DeadLettered {
            reason, summary, ..
        } => record_given_up(
            registry,
            profile,
            (duration_ms, first_sight),
            reason,
            summary,
        ),
        Step::Idle | Step::TopicMissing | Step::Paused => {}
        // The remedy in these carries the specifics — which
        // token to mint, or that the hub itself is away.
        Step::Denied { detail } | Step::HubDown { detail } => {
            if before != after {
                obs::log_warn(&profile.name, "hub_problem", detail);
            }
        }
    }
}

/// A delivery failed and the message went back to the hub.
fn record_handed_back(
    registry: &Registry,
    profile: &Profile,
    (duration_ms, first_sight): (u64, bool),
    attempts: u32,
    reason: &str,
) {
    registry.record_offer(&profile.name, false, duration_ms, first_sight);
    obs::log_message(&profile.name, "kyu", "handed-back", duration_ms, attempts);
    obs::log_warn(&profile.name, "delivery_failed", reason);
}

/// A message was given up on. Health did not move, so this is not a
/// transition: it is a warning that one message was given up on, and why.
fn record_given_up(
    registry: &Registry,
    profile: &Profile,
    (duration_ms, first_sight): (u64, bool),
    reason: &str,
    summary: &str,
) {
    registry.record_given_up(&profile.name, first_sight, summary);
    obs::log_message(&profile.name, "kyu", "dead-lettered", duration_ms, 1);
    obs::log_warn(&profile.name, "dead_lettered", reason);
}

/// A message was delivered; a failed ack still means one duplicate later.
fn record_delivered(
    registry: &Registry,
    profile: &Profile,
    (duration_ms, first_sight): (u64, bool),
    attempts: u32,
    ack_failed: bool,
) {
    registry.record_offer(&profile.name, true, duration_ms, first_sight);
    obs::log_message(&profile.name, "kyu", "delivered", duration_ms, attempts);
    if ack_failed {
        obs::log_warn(
            &profile.name,
            "ack_failed",
            "the message was delivered but the hub would not take the \
             acknowledgement, so it will be offered again. What now: \
             expect one duplicate; if this repeats, the delivery is \
             probably outlasting the lease — lower timeout_ms or \
             retries, or raise lease_ms.",
        );
    }
}

/// The words a transition is logged and reported with.
///
/// AR11: state changes are logged once, not per attempt, so an hour of
/// hub downtime should produce two lines, not thousands. W11 / AR12: one
/// event when a profile falls over, one when it recovers, never one per
/// message. HA down for twenty minutes with a backlog would otherwise
/// produce a burst of warnings in a house whose dispatcher exists to
/// prevent exactly that.
fn transition_detail(state: &PumpState) -> &'static str {
    if state.paused {
        return REFUSAL_DETAIL;
    }
    match state.health {
        Health::Denied => "the hub refused our credentials",
        Health::HubDown => "the hub could not be reached",
        Health::Failing => "deliveries are failing; messages wait on the hub",
        Health::Working => "back to normal",
        Health::Starting => "starting",
    }
}

/// How long to rest before the next poll, by what the last step found.
async fn pause_after(clock: &dyn Clock, step: &Step) {
    match step {
        Step::Idle | Step::TopicMissing => clock.sleep(IDLE_PAUSE_MS).await,
        Step::Denied { .. } | Step::HubDown { .. } => clock.sleep(HUB_DOWN_PAUSE_MS).await,
        Step::Paused => clock.sleep(PAUSED_POLL_MS).await,
        _ => {}
    }
}
