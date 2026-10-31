use super::*;

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
