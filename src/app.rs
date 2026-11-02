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

mod pump_task;
mod transition;

pub use transition::{announce_transition, note_transition};

use pump_task::PumpContext;

use crate::adapters::{Clock, HttpSink, KyuHub, Sink};
use crate::config::{Config, Source};
use crate::obs::Registry;
use crate::pump::Hub;
use crate::reload::ProfileStore;

/// How long to wait after an idle poll before asking again. The hub's own
/// long poll does the waiting, so this is only about not hammering it when
/// it answers instantly (an unknown topic, or a hub that is away).
const IDLE_PAUSE_MS: u64 = 1_000;
/// The same pause when the hub refuses us or cannot be reached.
const HUB_DOWN_PAUSE_MS: u64 = 5_000;
/// The sink's own request ceiling; each profile's timeout is applied per
/// attempt inside the retry loop (`deliver_with_retry`).
const SINK_CEILING_MS: u64 = 30_000;
/// How long one hub long poll waits for a message, in seconds.
const HUB_LONG_POLL_WAIT_S: u64 = 25;

/// The whole service, with its own pieces injectable so the assembly can
/// be tested with fakes as well as against a real hub.
pub struct App {
    /// The config the service was started with: the shape a reload is
    /// held against (feat-reload-1). The profiles actually delivered with
    /// live in `profiles`.
    pub config: Config,
    pub profiles: Arc<ProfileStore>,
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
            SINK_CEILING_MS,
        ));
        let hub: Option<Arc<dyn Hub>> = base_url
            .map(|url| Arc::new(KyuHub::new(url, token, HUB_LONG_POLL_WAIT_S)) as Arc<dyn Hub>);

        Self {
            profiles: Arc::new(ProfileStore::new(&config)),
            config,
            registry,
            sink,
            clock: Arc::new(crate::adapters::TokioClock),
            hub,
        }
    }

    /// The inbound routes only (profile paths), for the kit to merge as
    /// public routes; `/healthz` and `/metrics` are the kit's since 2.0.0.
    pub fn profile_router(&self) -> axum::Router {
        crate::inbound::profile_router(
            &self.config,
            Arc::clone(&self.profiles),
            Arc::clone(&self.sink),
            Arc::clone(&self.clock),
            Arc::clone(&self.registry),
        )
    }

    /// Start one pump task per kyu profile; the returned sender stops them.
    pub fn spawn_pumps(&self) -> tokio::sync::broadcast::Sender<()> {
        let (stop_tx, _) = tokio::sync::broadcast::channel::<()>(1);
        for profile in &self.config.profiles {
            let Source::Kyu { .. } = &profile.source else {
                continue;
            };
            let Some(hub) = self.hub.clone() else {
                continue;
            };
            let ctx = PumpContext {
                hub,
                profiles: Arc::clone(&self.profiles),
                sink: Arc::clone(&self.sink),
                clock: Arc::clone(&self.clock),
                registry: Arc::clone(&self.registry),
                reporting: self.config.reporting.as_ref().map(|r| r.topic.clone()),
            };
            tokio::spawn(pump_task::run(
                Arc::new(profile.clone()),
                ctx,
                stop_tx.subscribe(),
            ));
        }
        stop_tx
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
            Arc::clone(&self.profiles),
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

        let stop_tx = self.spawn_pumps();

        let result = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| format!("the listener stopped: {e}"));

        // Stopping accepting is the shutdown; the pumps hold nothing, so
        // being cut off mid-poll costs at worst a duplicate (S3).
        // `send` only errors when no pump is listening (no kyu profiles),
        // in which case there is nothing to stop.
        let _ = stop_tx.send(());
        result
    }
}
