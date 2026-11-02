//! Health transitions: record one, log it once, and tell the house about
//! the ones that matter by publishing a self-report event.

use crate::adapters::Sink;
use crate::obs::{self, Registry};
use crate::pump::Health;
use crate::translate::{Delivery, Target};

/// Note one transition for a profile: update the registry, log it once,
/// and — when the transition is worth telling the house about — publish
/// one event. Shared by the pump loop and the inbound handler, because
/// AR12 makes no exception for the source kind and the first version
/// covered only the kyu side (Phase 7 audit, G3).
pub async fn note_transition(
    registry: &Registry,
    sink: &dyn Sink,
    reporting: Option<&str>,
    profile: &str,
    before: Health,
    now: Health,
    detail: &str,
) {
    if before == now {
        return;
    }
    registry.set_health(profile, now);
    announce_transition(sink, reporting, profile, before, now, detail).await;
}

/// The reporting half of a transition: log it once and publish the event
/// when it is worth one. For callers that already recorded the new health
/// atomically (`Registry::swap_health`) and must not write it a second time.
pub async fn announce_transition(
    sink: &dyn Sink,
    reporting: Option<&str>,
    profile: &str,
    before: Health,
    now: Health,
    detail: &str,
) {
    if before == now {
        return;
    }
    obs::log_transition(profile, before, now, detail);
    if let (Some(topic), Some(event)) = (reporting, transition_event(profile, before, now, detail))
    {
        report(sink, topic, event, profile).await;
    }
}

/// Which transitions are worth telling the house about. Starting up is
/// not news; falling over and coming back are.
fn transition_event(profile: &str, from: Health, to: Health, detail: &str) -> Option<String> {
    let broken = |h: Health| matches!(h, Health::Failing | Health::Denied | Health::HubDown);
    let event = match (broken(from), broken(to)) {
        (false, true) => "profile.failing",
        (true, false) if to == Health::Working => "profile.recovered",
        _ => return None,
    };
    let state = to.word();
    Some(format!(
        r#"{{"event":"{event}","profile":{},"state":"{state}","detail":{}}}"#,
        serde_json::Value::String(profile.to_string()),
        serde_json::Value::String(detail.to_string())
    ))
}

/// Publish one self-report. A failure to publish a failure is logged and
/// counted — never published, or the report about the broken channel goes
/// down the broken channel (AR12).
async fn report(sink: &dyn Sink, topic: &str, body: String, profile: &str) {
    let delivery = Delivery {
        target: Target::KyuTopic {
            topic: topic.to_string(),
        },
        content_type: "application/json".to_string(),
        headers: Default::default(),
        body,
        forward_error_body: false,
    };
    if let Err(e) = sink.deliver(&delivery).await {
        // Logged and counted, never published: the report about a broken
        // channel must not go down the broken channel (AR12). And it is a
        // warning, not a transition — the profile's state did not change
        // because we failed to talk about it.
        obs::log_warn(
            profile,
            "self_report_failed",
            &format!("could not publish the self-report event: {e}"),
        );
    }
}
