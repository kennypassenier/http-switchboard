//! L6b — the parts, assembled into something that runs.
//!
//! Exit criterion: the binary loads a real config, a message travels the
//! whole way through the running service, and a broken config stops it
//! with a non-zero exit and a remedy.

mod support;

use std::collections::HashMap;

use http_switchboard::app::App;
use http_switchboard::config;
use support::{KyuHarness, TestServer};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn binary() -> std::path::PathBuf {
    // The test binary lives in target/<profile>/deps; the service binary
    // is two levels up from there.
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

#[test]
fn l6b_a_broken_config_stops_the_binary_with_a_remedy() {
    let dir = std::env::temp_dir().join(format!("hsw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.toml");
    std::fs::write(&path, "[[profiles]]\nname = \"a\"\n").unwrap();

    // 2.0.0: the kit's --check; the state dir is the kit's probe target.
    let out = std::process::Command::new(binary())
        .args(["--check", "--config"])
        .arg(&path)
        .arg("--state-dir")
        .arg(&dir)
        .output()
        .expect("the binary must be built");

    assert!(!out.status.success(), "a broken config must not pass");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");
    assert!(
        stderr.contains("broken.toml"),
        "the file is named: {stderr}"
    );
}

#[test]
fn l6b_the_shipped_example_config_passes_the_check() {
    // The config that ships with the project is checked by the binary
    // itself, not only by a unit test that happens to parse it.
    let state = std::env::temp_dir().join(format!("hsw-check-{}", std::process::id()));
    std::fs::create_dir_all(&state).unwrap();
    let out = std::process::Command::new(binary())
        .args([
            "--check",
            "--config",
            "deploy/config.example.toml",
            "--state-dir",
        ])
        .arg(&state)
        .env("KYU_TOKEN", "vault-value")
        .output()
        .expect("the binary must be built");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("1 profile"), "{stdout}");
}

#[tokio::test]
async fn l6b_a_message_travels_through_the_running_service() {
    // The whole way: a POST arrives at the service, the profile
    // translates it, and the receiver sees the result — with the service
    // started the way the binary starts it.
    let receiver = TestServer::start(vec![200]).await;
    let port = free_port();
    let text = format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
body = '''{{"alert": {{{{ name }}}}}}'''
"#,
        receiver.base_url
    );
    let cfg = config::load("t.toml", &text, &env(&[])).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        App::from_config(cfg)
            .run(format!("127.0.0.1:{port}").parse().unwrap(), async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });
    wait_until_healthy(port).await;

    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/hook"))
        .header("content-type", "application/json")
        .body(r#"{"name": "HostDown"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(receiver.received()[0].body, r#"{"alert": "HostDown"}"#);

    // And it stops when asked.
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn l6b_e2e_the_running_service_pumps_the_hub_by_itself() {
    // No test driving pump_once by hand: the service's own loop picks the
    // message up, translates it and delivers it.
    let Some(hub) = KyuHarness::start().await else {
        eprintln!("skipped: set KYU_IMAGE to run this against a real kyu");
        return;
    };
    let receiver = TestServer::start(vec![200]).await;
    let port = free_port();
    let text = format!(
        r#"
[kyu]
base_url = "{}"

[[profiles]]
name = "pumped"
from = {{ kyu_topic = "assembly.raw" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
body = '''{{"alert": {{{{ name }}}}}}'''
"#,
        hub.base_url, receiver.base_url
    );
    let cfg = config::load("t.toml", &text, &env(&[])).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        App::from_config(cfg)
            .run(format!("127.0.0.1:{port}").parse().unwrap(), async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });
    wait_until_healthy(port).await;

    hub.publish("assembly.raw", r#"{"name": "FilesystemFull"}"#)
        .await;

    let mut seen = false;
    for _ in 0..40 {
        if !receiver.received().is_empty() {
            seen = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(seen, "the service should have pumped the message by itself");
    assert_eq!(
        receiver.received()[0].body,
        r#"{"alert": "FilesystemFull"}"#
    );

    let metrics = reqwest::get(format!("http://127.0.0.1:{port}/metrics"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        metrics.contains(r#"switchboard_messages_delivered_total{profile="pumped"} 1"#),
        "the running service should have counted it: {metrics}"
    );

    let _ = stop_tx.send(());
}

async fn wait_until_healthy(port: u16) {
    for _ in 0..60 {
        if let Ok(r) = reqwest::get(format!("http://127.0.0.1:{port}/healthz")).await {
            if r.status().is_success() {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the service did not become healthy");
}
