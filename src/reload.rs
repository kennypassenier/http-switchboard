//! feat-reload-1: a new config read while the service keeps running
//! (Kenny, 2026-09-26, raised Later → Essential).
//!
//! The rating's own warning is why this module is small: reloading adds
//! a second path along which the running service can break. So a reload
//! is all or nothing. The new file goes through exactly the validation a
//! start does; anything short of a complete, valid config — a typo, a
//! half-written file, a profile that no longer fits its lease — leaves
//! the running profiles untouched and says why.
//!
//! What a reload may change is what lives *inside* a profile: its
//! template, destination, headers, timeouts, retries, lease and attempt
//! cap. What it may not change is the shape the kit registered at start —
//! the set of profiles, where each one reads from, its kyu subscription,
//! and the `[kyu]` and `[reporting]` sections. The routes, the health
//! subsystems and the hub connection are built once from those; changing
//! them under a running listener is the second path the rating warned
//! about, and a restart costs nothing (the hub holds the position).

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::config::{Config, Profile};

/// The profiles the running service delivers with, swappable as a whole.
/// Readers take an `Arc` per message, so a reload never changes a profile
/// under a delivery that is already under way.
#[derive(Debug)]
pub struct ProfileStore {
    current: RwLock<Arc<BTreeMap<String, Arc<Profile>>>>,
}

impl ProfileStore {
    pub fn new(config: &Config) -> Self {
        Self {
            current: RwLock::new(Arc::new(index(config))),
        }
    }

    /// The profile as it stands now. `None` only for a name the service
    /// was never started with, which a reload cannot introduce.
    pub fn get(&self, name: &str) -> Option<Arc<Profile>> {
        self.current
            .read()
            .expect("profile store lock")
            .get(name)
            .cloned()
    }

    /// Every profile, in name order.
    pub fn all(&self) -> Vec<Arc<Profile>> {
        self.current
            .read()
            .expect("profile store lock")
            .values()
            .cloned()
            .collect()
    }

    fn replace(&self, config: &Config) {
        *self.current.write().expect("profile store lock") = Arc::new(index(config));
    }
}

fn index(config: &Config) -> BTreeMap<String, Arc<Profile>> {
    config
        .profiles
        .iter()
        .map(|p| (p.name.clone(), Arc::new(p.clone())))
        .collect()
}

/// What one reload did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Applied; the names of the profiles whose contents changed.
    Applied { changed: Vec<String> },
    /// The file is valid but asks for something only a restart can do.
    NeedsRestart { reason: String },
    /// The file could not be read or is not a valid config.
    Invalid { reason: String },
}

impl Outcome {
    /// The metric label for this outcome.
    pub fn label(&self) -> &'static str {
        match self {
            Outcome::Applied { .. } => "applied",
            Outcome::NeedsRestart { .. } => "needs-restart",
            Outcome::Invalid { .. } => "invalid",
        }
    }
}

/// Apply `loaded` — the result of reading and validating the file again —
/// against the shape the service was started with. Nothing changes unless
/// the result is `Applied`.
pub fn apply(started: &Config, store: &ProfileStore, loaded: Result<Config, String>) -> Outcome {
    let new = match loaded {
        Ok(config) => config,
        Err(reason) => return Outcome::Invalid { reason },
    };
    if let Err(reason) = same_shape(started, &new) {
        return Outcome::NeedsRestart { reason };
    }
    let before = store.all();
    let changed = new
        .profiles
        .iter()
        .filter(|p| {
            before
                .iter()
                .find(|b| b.name == p.name)
                .is_none_or(|b| !same_contents(b, p))
        })
        .map(|p| p.name.clone())
        .collect();
    store.replace(&new);
    Outcome::Applied { changed }
}

/// Whether a reload of `new` can be applied to a service started with
/// `started`, and if not, what would need the restart.
pub fn same_shape(started: &Config, new: &Config) -> Result<(), String> {
    let names = |c: &Config| {
        let mut v: Vec<String> = c.profiles.iter().map(|p| p.name.clone()).collect();
        v.sort();
        v
    };
    if names(started) != names(new) {
        return Err(format!(
            "the set of profiles changed ({} → {}); adding, removing or renaming a profile \
             registers routes and health checks, which happens at start",
            names(started).join(", "),
            names(new).join(", ")
        ));
    }
    let kyu = |c: &Config| {
        c.kyu
            .as_ref()
            .map(|k| (k.base_url.clone(), k.token.clone()))
    };
    if kyu(started) != kyu(new) {
        return Err("the [kyu] section changed; the hub connection is opened at start".into());
    }
    let reporting = |c: &Config| c.reporting.as_ref().map(|r| r.topic.clone());
    if reporting(started) != reporting(new) {
        return Err("the [reporting] section changed".into());
    }
    for old in &started.profiles {
        let Some(now) = new.profiles.iter().find(|p| p.name == old.name) else {
            continue;
        };
        if old.source != now.source {
            return Err(format!(
                "profile '{}' reads from somewhere else now; a source is a route or a pump, \
                 created at start",
                old.name
            ));
        }
        if old.subscription != now.subscription {
            return Err(format!(
                "profile '{}' has a new kyu subscription; that is a new position on the hub",
                old.name
            ));
        }
    }
    Ok(())
}

fn same_contents(a: &Profile, b: &Profile) -> bool {
    a.sink == b.sink
        && a.content_type == b.content_type
        && a.body == b.body
        && a.headers == b.headers
        && a.timeout_ms == b.timeout_ms
        && a.retries == b.retries
        && a.lease_ms == b.lease_ms
        && a.max_attempts == b.max_attempts
        && a.forward_error_body == b.forward_error_body
}

/// What the log line and the operator see for one outcome.
pub fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Applied { changed } if changed.is_empty() => {
            "config reloaded: nothing changed".to_string()
        }
        Outcome::Applied { changed } => {
            format!("config reloaded: changed {}", changed.join(", "))
        }
        Outcome::NeedsRestart { reason } => format!(
            "config NOT reloaded: {reason}. The running profiles are unchanged. What now: \
             `systemctl restart http-switchboard` — the hub holds the position, so a restart \
             loses nothing."
        ),
        Outcome::Invalid { reason } => format!(
            "config NOT reloaded: {reason}. The running profiles are unchanged. What now: fix \
             the file and reload again; `http-switchboard --check` shows the same verdict."
        ),
    }
}
