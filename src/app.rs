//! Assembly: the parts of L1-L6 wired into a service that actually runs
//! (L6b).
//!
//! This milestone exists because it was missing. Every earlier milestone
//! described a *part* — config, translation, sinks, the hub side, the
//! inbound side, observability — and putting them together was in none of
//! them, so the binary still printed "no features built yet" while all of
//! its components were proven. Found and reported at the L1-L6 gate.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::adapters::{Clock, HttpSink, KyuHub, Sink};
use crate::config::{Config, Source};
use crate::obs::{self, Registry};
use crate::pump::{pump_once, Health, Hub, PumpState, Step};
use crate::translate::{Delivery, Target};

/// How long to wait after an idle poll before asking again. The hub's own
/// long poll does the waiting, so this is only about not hammering it when
/// it answers instantly (an unknown topic, or a hub that is away).
const IDLE_PAUSE_MS: u64 = 1_000;
const HUB_DOWN_PAUSE_MS: u64 = 5_000;

/// The whole service, with its own pieces injectable so the assembly can
/// be tested with fakes as well as against a real hub.
pub struct App {
    pub config: Config,
    pub registry: Arc<Registry>,
    pub sink: Arc<dyn Sink>,
    pub clock: Arc<dyn Clock>,
    pub hub: Option<Arc<dyn Hub>>,
}

impl App {
    /// Build the real thing from a validated config.
    pub fn from_config(config: Config) -> Self {
        let registry = Arc::new(Registry::new());
        for profile in &config.profiles {
            registry.register(&profile.name);
        }

        let (base_url, token) = match &config.kyu {
            Some(k) => (Some(k.base_url.clone()), k.token.clone()),
            None => (None, None),
        };
        let sink: Arc<dyn Sink> = Arc::new(HttpSink::new(
            base_url.clone(),
            token.clone(),
            // The sink's own ceiling; each profile's timeout is applied
            // per attempt inside the retry loop.
            30_000,
        ));
        let hub: Option<Arc<dyn Hub>> =
            base_url.map(|url| Arc::new(KyuHub::new(url, token, 25)) as Arc<dyn Hub>);

        Self {
            config,
            registry,
            sink,
            clock: Arc::new(crate::adapters::TokioClock),
            hub,
        }
    }

    /// Run until `shutdown` resolves: one pump task per kyu profile, one
    /// HTTP listener for the inbound profiles and the two endpoints.
    pub async fn run(
        self,
        listen: SocketAddr,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), String> {
        let router = crate::inbound::router(
            &self.config,
            Arc::clone(&self.sink),
            Arc::clone(&self.clock),
            Arc::clone(&self.registry),
        );

        let listener = tokio::net::TcpListener::bind(listen).await.map_err(|e| {
            format!(
                "cannot listen on {listen}: {e}. What now: check that nothing else holds that \
                 address, and that the container publishes it."
            )
        })?;

        let (stop_tx, _) = tokio::sync::broadcast::channel::<()>(1);
        for profile in &self.config.profiles {
            let Source::Kyu { .. } = &profile.source else {
                continue;
            };
            let Some(hub) = self.hub.clone() else {
                continue;
            };
            let profile = profile.clone();
            let sink = Arc::clone(&self.sink);
            let clock = Arc::clone(&self.clock);
            let registry = Arc::clone(&self.registry);
            let reporting = self.config.reporting.as_ref().map(|r| r.topic.clone());
            let mut stop = stop_tx.subscribe();
            tokio::spawn(async move {
                let mut state = PumpState::default();
                loop {
                    if stop.try_recv().is_ok() {
                        return;
                    }
                    let before = state.health;
                    let started = std::time::Instant::now();
                    let step = pump_once(
                        &profile,
                        hub.as_ref(),
                        sink.as_ref(),
                        clock.as_ref(),
                        &mut state,
                    )
                    .await;
                    let duration_ms = started.elapsed().as_millis() as u64;

                    match &step {
                        Step::Delivered {
                            attempts,
                            ack_failed,
                            ..
                        } => {
                            registry.record(&profile.name, true, duration_ms);
                            obs::log_message(
                                &profile.name,
                                "kyu",
                                "delivered",
                                duration_ms,
                                *attempts,
                            );
                            if *ack_failed {
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
                        Step::HandedBack {
                            attempts, reason, ..
                        } => {
                            registry.record(&profile.name, false, duration_ms);
                            obs::log_message(
                                &profile.name,
                                "kyu",
                                "handed-back",
                                duration_ms,
                                *attempts,
                            );
                            obs::log_warn(&profile.name, "delivery_failed", reason);
                        }
                        Step::DeadLettered { reason, .. } => {
                            registry.record(&profile.name, false, duration_ms);
                            obs::log_message(&profile.name, "kyu", "dead-lettered", duration_ms, 1);
                            obs::log_transition(&profile.name, before, state.health, reason);
                        }
                        Step::Idle | Step::TopicMissing => {}
                        // The remedy in these carries the specifics — which
                        // token to mint, or that the hub itself is away.
                        Step::Denied { detail } | Step::HubDown { detail } => {
                            if before != state.health {
                                obs::log_warn(&profile.name, "hub_problem", detail);
                            }
                        }
                    }

                    // AR11: state changes are logged once, not per attempt —
                    // an hour of hub downtime should produce two lines, not
                    // thousands.
                    if before != state.health {
                        registry.set_health(&profile.name, state.health);
                        let detail = match state.health {
                            Health::Denied => "the hub refused our credentials",
                            Health::HubDown => "the hub could not be reached",
                            Health::Failing => "deliveries are failing; messages wait on the hub",
                            Health::Working => "back to normal",
                            Health::Starting => "starting",
                        };
                        obs::log_transition(&profile.name, before, state.health, detail);

                        // W11 / AR12: one event when a profile falls over,
                        // one when it recovers — never one per message. HA
                        // down for twenty minutes with a backlog would
                        // otherwise produce a burst of warnings in a house
                        // whose dispatcher exists to prevent exactly that.
                        if let Some(topic) = &reporting {
                            if let Some(event) =
                                transition_event(&profile.name, before, state.health, detail)
                            {
                                report(sink.as_ref(), topic, event, &profile.name).await;
                            }
                        }
                    }

                    match step {
                        Step::Idle | Step::TopicMissing => {
                            clock.sleep(IDLE_PAUSE_MS).await;
                        }
                        Step::Denied { .. } | Step::HubDown { .. } => {
                            clock.sleep(HUB_DOWN_PAUSE_MS).await;
                        }
                        _ => {}
                    }
                }
            });
        }

        let result = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| format!("the listener stopped: {e}"));

        // Stopping accepting is the shutdown; the pumps hold nothing, so
        // being cut off mid-poll costs at worst a duplicate (S3).
        let _ = stop_tx.send(());
        result
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
    let state = match to {
        Health::Starting => "starting",
        Health::Working => "working",
        Health::Failing => "failing",
        Health::Denied => "denied",
        Health::HubDown => "hub-down",
    };
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
