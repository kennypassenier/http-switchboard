//! L4 — the hub side (K2; AR8). The heart of the project.
//!
//! Two layers, on purpose. The scripted-hub tests pin the ORDER — poll,
//! deliver, only then acknowledge — because that ordering is what loses
//! messages and it must be provable without a container. The E2E tests
//! then run the same code against a real kyu, because a fake hub has no
//! leases, no redelivery and no dead letters, which are the three things
//! this milestone actually rests on.

#[path = "../support/mod.rs"]
mod support;

use http_switchboard::adapters::HttpSink;
use http_switchboard::config::{self, Profile};
use http_switchboard::pump::{pump_once, Health, HubMessage, Poll, PumpState, Step};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use support::{FakeClock, FakeHub, FakeSink, HubErrorKind, KyuHarness, TestServer};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn kyu_profile(base_url: &str, topic: &str, to: &str, body: &str) -> Profile {
    let text = format!(
        r#"
[kyu]
base_url = "{base_url}"

[[profiles]]
name = "p"
subscription = "switchboard"
from = {{ kyu_topic = "{topic}" }}
to = {to}
content_type = "application/json"
retries = 0
timeout_ms = 2000
body = '''{body}'''
"#
    );
    config::load("t.toml", &text, &env(&[]))
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn message(id: &str, payload: serde_json::Value) -> Poll {
    Poll::Message(Box::new(HubMessage {
        id: id.to_string(),
        payload,
        attempt: 1,
    }))
}

// ── the same code against a real kyu ───────────────────────────────────
macro_rules! require_hub {
    ($harness:ident) => {
        let Some($harness) = KyuHarness::start().await else {
            unreachable!("KyuHarness::start always starts a kyu");
        };
    };
}

mod e2e_delivery;
mod e2e_policy;
mod scripted_giveup;
mod scripted_order;
mod scripted_states;
mod scripted_streak;
