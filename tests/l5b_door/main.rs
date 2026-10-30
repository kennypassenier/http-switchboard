//! L5b — the door, as the kit holds it (H1, W8 amended; 3.0.0).
//!
//! Until 3.0.0 this service checked a per-path `inbound_token` of its own.
//! It now registers the profile paths as the kit's API routes, so the kit
//! checks an `Authorization: Bearer <client token>` before the body is
//! ever read, and a revoked token is out the same second.
//!
//! These tests run the real thing: `chassis::testing::TestApp` starts this
//! service in-process on a free port with a temporary state directory and
//! fresh secrets — the same assembly `main.rs` builds — and drives it over
//! HTTP. That is the kit harness of 1.8.0 (T1); the fakes in
//! `tests/support` stay where the subject is this project's own domain
//! (the pump's ordering, the hub, the retry ladder), which the harness
//! knows nothing about.

#[path = "../support/mod.rs"]
mod support;

use axum::Router;
use chassis::testing::TestApp;
use chassis::AppSpec;
use http_switchboard::app::App as Switchboard;
use http_switchboard::config;
use reqwest::Method;
use std::collections::HashMap;
use std::sync::Arc;
use support::TestServer;

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn one_http_profile(receiver: &str) -> String {
    format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{receiver}/in" }}
content_type = "application/json"
body = '{{"x": {{{{ x }}}}}}'
"#
    )
}

/// Start this service the way `main.rs` does, under the kit's test
/// harness: profile paths as API routes, the Profiles section on the
/// status page, senders as the word for a client.
async fn start(config_text: &str) -> TestApp {
    let cfg = config::load("t.toml", config_text, &env(&[])).expect("config must load");
    let switchboard = Switchboard::from_config(cfg);
    let registry = Arc::clone(&switchboard.registry);
    let profiles = Arc::new(http_switchboard::dashboard::Profiles::new(
        &switchboard.config,
        Arc::clone(&switchboard.profiles),
        Arc::clone(&registry),
    ));
    let subsystems: Vec<http_switchboard::obs::ProfileSubsystem> = switchboard
        .config
        .profiles
        .iter()
        .map(|p| http_switchboard::obs::ProfileSubsystem::new(&p.name, Arc::clone(&registry)))
        .collect();
    let routes = switchboard.profile_router();
    let spec = AppSpec {
        name: "http-switchboard",
        version: env!("CARGO_PKG_VERSION"),
        repository: Some("kennypassenier/http-switchboard"),
        ..Default::default()
    };
    TestApp::start_with(spec, Router::new(), move |app| {
        app.api_routes(routes);
        for s in subsystems {
            app.subsystem(s);
        }
        app.vocabulary("sender", "senders");
        http_switchboard::dashboard::install(app, profiles);
    })
    .await
}

mod door;
mod status_page;
