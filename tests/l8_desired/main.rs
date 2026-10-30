//! L8 — the two Desired features (K7, W4).

#[path = "../support/mod.rs"]
mod support;

use http_switchboard::config::{self, Profile};
use http_switchboard::translate::{self, Target};
use std::collections::HashMap;

fn env() -> impl config::EnvLookup {
    let map: HashMap<String, String> = HashMap::new();
    move |k: &str| map.get(k).cloned()
}

fn profile(to: &str) -> Profile {
    let text = format!(
        r#"
[[profiles]]
name = "t"
from = {{ http_path = "/t" }}
to = {to}
content_type = "application/json"
body = '''{{"ok": true}}'''
"#
    );
    config::load("t.toml", &text, &env())
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn profile_with_method(to: &str, method: &str) -> Profile {
    let text = format!(
        r#"
[[profiles]]
name = "t"
from = {{ http_path = "/t" }}
to = {to}
method = "{method}"
content_type = "application/json"
body = '''{{"ok": true}}'''
"#
    );
    config::load("t.toml", &text, &env())
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn binary() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

mod dry_run;
mod path_segments;
