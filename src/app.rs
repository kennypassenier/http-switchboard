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
                        Step::Delivered { attempts, .. } => {
                            registry.record(&profile.name, true, duration_ms);
                            obs::log_message(
                                &profile.name,
                                "kyu",
                                "delivered",
                                duration_ms,
                                *attempts,
                            );
                        }
                        Step::HandedBack { .. } => {
                            registry.record(&profile.name, false, duration_ms);
                            obs::log_message(&profile.name, "kyu", "handed-back", duration_ms, 1);
                        }
                        Step::DeadLettered { reason, .. } => {
                            registry.record(&profile.name, false, duration_ms);
                            obs::log_message(&profile.name, "kyu", "dead-lettered", duration_ms, 1);
                            obs::log_transition(&profile.name, before, state.health, reason);
                        }
                        Step::Idle | Step::TopicMissing => {}
                        Step::Denied | Step::HubDown => {}
                    }

                    // AR11: state changes are logged once, not per attempt —
                    // an hour of hub downtime should produce two lines, not
                    // thousands.
                    if before != state.health {
                        registry.set_health(&profile.name, state.health);
                        obs::log_transition(
                            &profile.name,
                            before,
                            state.health,
                            match state.health {
                                Health::Denied => "the hub refused our credentials",
                                Health::HubDown => "the hub could not be reached",
                                Health::Failing => {
                                    "deliveries are failing; messages wait on the hub"
                                }
                                Health::Working => "back to normal",
                                Health::Starting => "starting",
                            },
                        );
                    }

                    match step {
                        Step::Idle | Step::TopicMissing => {
                            clock.sleep(IDLE_PAUSE_MS).await;
                        }
                        Step::Denied | Step::HubDown => {
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
