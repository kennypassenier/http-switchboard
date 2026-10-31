//! L3 — delivering to a URL and to a kyu topic (K3, K4, K6, W2, W3).
//!
//! Against a real socket, not a mock of HTTP: the fake receiver is a
//! genuine TCP listener, so the client's timeout, headers and body are
//! exercised for real (standing rule 9).

#[path = "../support/mod.rs"]
mod support;

use http_switchboard::adapters::{deliver_with_retry, DeliverError, HttpSink, Sink};
use http_switchboard::config::{self, Profile};
use http_switchboard::secret::Secret;
use http_switchboard::translate::{self, Delivery, Target};
use std::collections::HashMap;
use support::{FakeClock, TestServer};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn profile(extra: &str, to: &str) -> Profile {
    let text = format!(
        r#"
[kyu]
base_url = "http://127.0.0.1:1"
token = "${{KYU_TOKEN}}"

[[profiles]]
name = "t"
from = {{ http_path = "/t" }}
to = {to}
content_type = "application/json"
body = '''{{"x": {{{{ x }}}}}}'''
{extra}
"#
    );
    config::load("t.toml", &text, &env(&[("KYU_TOKEN", "s3cr3t-hub-token")]))
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn delivery_to(url: &str, headers: &[(&str, &str)]) -> Delivery {
    Delivery {
        target: Target::Url {
            url: url.to_string(),
            method: "POST".to_string(),
        },
        content_type: "application/json".to_string(),
        headers: headers
            .iter()
            .map(|(k, v)| ((*k).to_string(), Secret::new(*v)))
            .collect(),
        body: r#"{"x": 1}"#.to_string(),
        forward_error_body: false,
    }
}

mod delivery;
mod resilience;
