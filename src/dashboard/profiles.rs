//! The live set of profiles the dashboard draws: configuration order,
//! the store that holds each profile now, and the counters registry.

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use super::handlers::{recheck, recheck_one, snapshot};
use super::view::{view_of, ProfileView};
use super::{RECHECK_ONE_ROUTE, RECHECK_ROUTE, SNAPSHOT_ROUTE};
use crate::config::Config;
use crate::obs::Registry;
use crate::reload::ProfileStore;

/// The profiles the dashboard shows, in configuration order.
pub struct Profiles {
    pub(super) registry: Arc<Registry>,
    store: Arc<ProfileStore>,
    /// Profile names in config order. The set never changes while the
    /// service runs; a destination can (feat-reload-1), so each snapshot
    /// reads the profile from the store.
    pub(super) order: Vec<String>,
}

impl Profiles {
    pub fn new(config: &Config, store: Arc<ProfileStore>, registry: Arc<Registry>) -> Self {
        let order = config.profiles.iter().map(|p| p.name.clone()).collect();
        Self {
            registry,
            store,
            order,
        }
    }

    /// The profiles as they stand now (the translate page renders from them).
    pub(crate) fn store(&self) -> &Arc<ProfileStore> {
        &self.store
    }

    /// Every profile in config order, as the browser sees it.
    pub fn snapshot(&self) -> Vec<ProfileView> {
        let stats = self.registry.snapshot();
        self.order
            .iter()
            .filter_map(|name| self.store.get(name))
            .map(|p| view_of(&p, stats.get(&p.name)))
            .collect()
    }

    /// The JSON that is both the first embedded snapshot and the poll's
    /// answer.
    pub(super) fn snapshot_json(&self) -> String {
        serde_json::json!({ "profiles": self.snapshot() }).to_string()
    }

    /// The routes behind the admin login.
    pub(super) fn routes(self: &Arc<Self>) -> Router {
        Router::new()
            .route(SNAPSHOT_ROUTE, get(snapshot))
            .route(RECHECK_ROUTE, post(recheck))
            .route(RECHECK_ONE_ROUTE, post(recheck_one))
            .with_state(Arc::clone(self))
    }
}
