//! feat-reload-1 — a new config read while the service keeps running
//! (Kenny, 2026-09-26: Later → Essential, "with tests for a broken and a
//! half-written config").
//!
//! The bar: a valid change reaches the next message; anything short of a
//! complete, valid config of the same shape changes nothing, and the
//! service keeps delivering with what it had.

mod support;

use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use http_switchboard::adapters::{HttpSink, TokioClock};
use http_switchboard::config;
use http_switchboard::inbound;
use http_switchboard::obs::Registry;
use http_switchboard::reload::{self, Outcome, ProfileStore};
use support::TestServer;

fn env() -> impl config::EnvLookup {
    let map: HashMap<String, String> = HashMap::new();
    move |k: &str| map.get(k).cloned()
}

fn profile(receiver: &str, body: &str) -> String {
    format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{receiver}/in" }}
content_type = "application/json"
retries = 0
timeout_ms = 2000
body = '''{body}'''
"#
    )
}

fn load(text: &str) -> Result<config::Config, String> {
    config::load("t.toml", text, &env()).map_err(|e| e.to_string())
}

/// The inbound side over a store, as the binary wires it.
async fn serve(cfg: &config::Config, store: Arc<ProfileStore>) -> String {
    let router = inbound::router(
        cfg,
        store,
        Arc::new(HttpSink::new(None, None, 2_000)),
        Arc::new(TokioClock),
        Arc::new(Registry::new()),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

async fn post(url: &str) -> u16 {
    reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .body(r#"{"x": 7}"#)
        .send()
        .await
        .expect("the server must answer")
        .status()
        .as_u16()
}

const OLD: &str = r#"{"old": {{ x }}}"#;
const NEW: &str = r#"{"new": {{ x }}}"#;

#[tokio::test]
async fn feat_reload_1_a_valid_change_reaches_the_next_message() {
    let receiver = TestServer::start(vec![200]).await;
    let started = load(&profile(&receiver.base_url, OLD)).unwrap();
    let store = Arc::new(ProfileStore::new(&started));
    let base = serve(&started, Arc::clone(&store)).await;

    assert_eq!(post(&format!("{base}/hook")).await, 200);
    let outcome = reload::apply(&started, &store, load(&profile(&receiver.base_url, NEW)));
    assert_eq!(
        outcome,
        Outcome::Applied {
            changed: vec!["hook".to_string()]
        }
    );
    assert_eq!(post(&format!("{base}/hook")).await, 200);

    let bodies: Vec<String> = receiver.received().into_iter().map(|r| r.body).collect();
    assert_eq!(bodies, [r#"{"old": 7}"#, r#"{"new": 7}"#]);
}

#[tokio::test]
async fn feat_reload_1_a_broken_config_changes_nothing() {
    let receiver = TestServer::start(vec![200]).await;
    let started = load(&profile(&receiver.base_url, OLD)).unwrap();
    let store = Arc::new(ProfileStore::new(&started));
    let base = serve(&started, Arc::clone(&store)).await;

    // A typo in a key: valid TOML, not a valid config.
    let broken = profile(&receiver.base_url, NEW).replace("timeout_ms", "timout_ms");
    let outcome = reload::apply(&started, &store, load(&broken));
    let Outcome::Invalid { reason } = &outcome else {
        panic!("expected Invalid, got {outcome:?}");
    };
    assert!(
        reason.contains("timout_ms"),
        "the reason names the key: {reason}"
    );
    assert!(reload::describe(&outcome).contains("unchanged"));

    assert_eq!(post(&format!("{base}/hook")).await, 200);
    assert_eq!(receiver.received()[0].body, r#"{"old": 7}"#);
}

#[tokio::test]
async fn feat_reload_1_a_half_written_file_changes_nothing() {
    let receiver = TestServer::start(vec![200]).await;
    let full = profile(&receiver.base_url, NEW);
    let started = load(&profile(&receiver.base_url, OLD)).unwrap();
    let store = Arc::new(ProfileStore::new(&started));
    let base = serve(&started, Arc::clone(&store)).await;

    // An editor caught mid-save: every cut of the new file must be refused
    // or be the old config, never a mixture.
    for cut in (1..full.len()).step_by(7) {
        let Some(half) = full.get(..cut) else {
            continue;
        };
        let outcome = reload::apply(&started, &store, load(half));
        assert!(
            matches!(
                outcome,
                Outcome::Invalid { .. } | Outcome::NeedsRestart { .. }
            ),
            "a file cut at byte {cut} was applied: {outcome:?}"
        );
    }

    assert_eq!(post(&format!("{base}/hook")).await, 200);
    assert_eq!(receiver.received()[0].body, r#"{"old": 7}"#);
}

#[test]
fn feat_reload_1_a_change_of_shape_asks_for_a_restart() {
    let started = load(&profile("http://127.0.0.1:1", OLD)).unwrap();
    let store = ProfileStore::new(&started);
    let cases = [
        (
            "a second profile",
            format!(
                "{}{}",
                profile("http://127.0.0.1:1", OLD),
                profile("http://127.0.0.1:1", OLD).replace("\"hook\"", "\"other\"")
            ),
        ),
        (
            "a renamed profile",
            profile("http://127.0.0.1:1", OLD).replace("\"hook\"", "\"renamed\""),
        ),
        (
            "a new path",
            profile("http://127.0.0.1:1", OLD).replace("\"/hook\"", "\"/elsewhere\""),
        ),
    ];
    for (what, text) in cases {
        let outcome = reload::apply(&started, &store, load(&text));
        let Outcome::NeedsRestart { reason } = &outcome else {
            panic!("{what}: expected NeedsRestart, got {outcome:?}");
        };
        assert!(
            reload::describe(&outcome).contains("systemctl restart"),
            "{what}: {reason}"
        );
        assert_eq!(
            store.get("hook").unwrap().body,
            OLD,
            "{what} changed the store"
        );
    }
}

#[test]
fn feat_reload_1_an_unchanged_file_reports_nothing_changed() {
    let text = profile("http://127.0.0.1:1", OLD);
    let started = load(&text).unwrap();
    let store = ProfileStore::new(&started);
    let outcome = reload::apply(&started, &store, load(&text));
    assert_eq!(outcome, Outcome::Applied { changed: vec![] });
    assert_eq!(
        reload::describe(&outcome),
        "config reloaded: nothing changed"
    );
}

#[test]
fn feat_reload_1_every_outcome_is_counted_in_metrics() {
    let registry = Registry::new();
    registry.record_reload("applied");
    registry.record_reload("invalid");
    registry.record_reload("invalid");
    let metrics = registry.metrics();
    assert!(metrics.contains(r#"switchboard_config_reloads_total{outcome="applied"} 1"#));
    assert!(metrics.contains(r#"switchboard_config_reloads_total{outcome="needs-restart"} 0"#));
    assert!(metrics.contains(r#"switchboard_config_reloads_total{outcome="invalid"} 2"#));
}

// ── the real binary, the real signal ───────────────────────────────────

const TEST_TOKEN: &str = "test-admin-token-not-a-real-one";
const TEST_SECRET_KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

fn binary() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn hangup(child: &Child) {
    let status = Command::new("kill")
        .args(["-HUP", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
}

async fn deliver(port: u16) -> u16 {
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/hook"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {TEST_TOKEN}"))
        .body(r#"{"x": 7}"#)
        .send()
        .await
        .map(|r| r.status().as_u16())
        .unwrap_or(0)
}

async fn metric(port: u16, outcome: &str) -> Option<u64> {
    let text = reqwest::get(format!("http://127.0.0.1:{port}/metrics"))
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let key = format!(r#"switchboard_config_reloads_total{{outcome="{outcome}"}} "#);
    text.lines()
        .find_map(|l| l.strip_prefix(&key))
        .and_then(|n| n.trim().parse().ok())
}

/// Wait for a reload counter to reach `want`: the signal is handled on
/// the runtime, not synchronously with `kill`.
async fn wait_for(port: u16, outcome: &str, want: u64) {
    for _ in 0..60 {
        if metric(port, outcome).await == Some(want) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("switchboard_config_reloads_total{{outcome=\"{outcome}\"}} never reached {want}");
}

#[tokio::test]
async fn feat_reload_1_sighup_reloads_the_running_binary_and_survives_a_broken_file() {
    let receiver = TestServer::start(vec![200]).await;
    let dir = std::env::temp_dir().join(format!("hsw-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    let write = |text: &str| {
        let mut f = std::fs::File::create(&config).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    };
    write(&profile(&receiver.base_url, OLD));

    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Killed(
        Command::new(binary())
            .arg("--config")
            .arg(&config)
            .arg("--state-dir")
            .arg(&dir)
            .arg("--listen")
            .arg(format!("127.0.0.1:{port}"))
            .env("HTTP_SWITCHBOARD_TOKEN", TEST_TOKEN)
            .env("HTTP_SWITCHBOARD_SECRET_KEY", TEST_SECRET_KEY)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for _ in 0..80 {
        if metric(port, "applied").await.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    assert_eq!(deliver(port).await, 200);

    // 1. A valid change, then SIGHUP: the next message uses it.
    write(&profile(&receiver.base_url, NEW));
    hangup(&child.0);
    wait_for(port, "applied", 1).await;
    assert_eq!(deliver(port).await, 200);

    // 2. A half-written file, then SIGHUP: refused, still delivering the
    //    last good version, and the process is still there — a hangup
    //    ends a process that does not handle it.
    let full = profile(&receiver.base_url, OLD);
    write(&full[..full.len() / 2]);
    hangup(&child.0);
    wait_for(port, "invalid", 1).await;
    assert_eq!(deliver(port).await, 200);

    let bodies: Vec<String> = receiver.received().into_iter().map(|r| r.body).collect();
    assert_eq!(bodies, [r#"{"old": 7}"#, r#"{"new": 7}"#, r#"{"new": 7}"#]);
    let _ = std::fs::remove_dir_all(&dir);
}

// ── the kyu side: a pump picks up the new profile between messages ─────

fn kyu_config(body: &str, lease_ms: u64) -> String {
    format!(
        r#"
[kyu]
base_url = "http://127.0.0.1:1"

[[profiles]]
name = "alerts"
subscription = "switchboard"
from = {{ kyu_topic = "alerts" }}
to = {{ url = "http://127.0.0.1:1/x" }}
content_type = "application/json"
retries = 0
timeout_ms = 2000
lease_ms = {lease_ms}
body = '''{body}'''
"#
    )
}

fn hub_message(id: &str) -> http_switchboard::pump::Poll {
    http_switchboard::pump::Poll::Message(Box::new(http_switchboard::pump::HubMessage {
        id: id.to_string(),
        payload: serde_json::json!({"x": 7}),
        attempt: 1,
    }))
}

#[tokio::test]
async fn feat_reload_1_a_pump_uses_the_new_profile_and_rewrites_a_changed_lease() {
    use http_switchboard::app::App;
    use std::sync::Mutex;
    use support::{FakeHub, FakeSink};

    let started = load(&kyu_config(OLD, 60_000)).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let hub = FakeHub::with_calls(
        vec![
            Ok(hub_message("m1")),
            // The idle poll is the pause in which the reload lands.
            Ok(http_switchboard::pump::Poll::Empty),
            Ok(hub_message("m2")),
        ],
        Arc::clone(&calls),
    );
    let mut app = App::from_config(started.clone());
    app.sink = Arc::new(FakeSink::new(vec![true, true], Arc::clone(&calls)));
    app.hub = Some(Arc::new(hub));
    let stop = app.spawn_pumps();

    let delivered = |n: usize| {
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.starts_with("deliver"))
            .count()
            >= n
    };
    for _ in 0..50 {
        if delivered(1) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        delivered(1),
        "m1 was never delivered: {:?}",
        calls.lock().unwrap()
    );
    let outcome = reload::apply(&started, &app.profiles, load(&kyu_config(NEW, 90_000)));
    assert_eq!(
        outcome,
        Outcome::Applied {
            changed: vec!["alerts".to_string()]
        }
    );
    for _ in 0..150 {
        if delivered(2) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let _ = stop.send(());

    let log = calls.lock().unwrap().clone();
    let deliveries: Vec<&String> = log.iter().filter(|c| c.starts_with("deliver")).collect();
    assert_eq!(
        deliveries,
        [
            r#"deliver(ok=true, body={"old": 7})"#,
            r#"deliver(ok=true, body={"new": 7})"#
        ],
        "full call log: {log:?}"
    );
    assert!(
        log.iter().any(|c| c.starts_with("policy(lease=90000")),
        "the new lease was never written to the hub: {log:?}"
    );
}
