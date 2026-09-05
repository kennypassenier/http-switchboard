//! Phase 7 — the promise that rested on words (G1).
//!
//! S3 says a hard kill at any moment costs at most a duplicate, never a
//! loss. Nothing had ever killed the running service to find out. This
//! does: the binary runs as its own process against a real kyu, gets
//! SIGKILLed while a delivery is in flight, and is started again.

mod support;

use std::io::Write;
use std::process::{Child, Command, Stdio};

use support::{KyuHarness, TestServer};

fn binary() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn spawn(config: &std::path::Path, port: u16) -> Child {
    // 2.0.0: the kit's argv; the config's directory doubles as the state dir.
    Command::new(binary())
        .arg("--config")
        .arg(config)
        .arg("--state-dir")
        .arg(config.parent().expect("the config lives in a directory"))
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the binary must be built")
}

async fn wait_healthy(port: u16) {
    for _ in 0..80 {
        if reqwest::get(format!("http://127.0.0.1:{port}/healthz"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    panic!("the service did not come up");
}

/// A child that is killed when the test ends, however the test ends.
struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn s3_e2e_a_hard_kill_mid_delivery_loses_nothing() {
    let Some(hub) = KyuHarness::start().await else {
        eprintln!("skipped: set KYU_IMAGE to run this against a real kyu");
        return;
    };
    // The first delivery attempt hangs forever; the second is accepted.
    // So the process is killed with the message claimed and undelivered —
    // the exact moment S3 is about.
    let receiver = TestServer::start(vec![0, 200]).await;
    let port = free_port();

    let dir = std::env::temp_dir().join(format!("hsw-kill-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    let mut file = std::fs::File::create(&config).unwrap();
    write!(
        file,
        r#"
[kyu]
base_url = "{}"

[[profiles]]
name = "survivor"
from = {{ kyu_topic = "kill.raw" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
# Short lease so the abandoned claim comes back quickly; the timeout is
# long enough that the hanging attempt is still in flight when the kill
# lands, and short enough that the config's own budget check accepts it
# (it refused the first version of this test, correctly).
lease_ms = 15000
timeout_ms = 9000
body = '''{{"alert": {{{{ name }}}}}}'''
"#,
        hub.base_url, receiver.base_url
    )
    .unwrap();
    drop(file);

    let mut child = Killed(spawn(&config, port));
    wait_healthy(port).await;

    // Give the subscription its first poll, then publish.
    tokio::time::sleep(std::time::Duration::from_millis(1_500)).await;
    hub.publish("kill.raw", r#"{"name": "SurvivesTheKill"}"#)
        .await;

    // Wait until the delivery is genuinely in flight (the receiver has the
    // request and is holding it).
    let mut in_flight = false;
    for _ in 0..60 {
        if !receiver.received().is_empty() {
            in_flight = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(in_flight, "the delivery never reached the receiver");

    // SIGKILL: no shutdown hook runs, nothing is flushed, the claim is
    // simply abandoned.
    child.0.kill().unwrap();
    child.0.wait().unwrap();

    // The hub still holds it. Start again and let the lease expire.
    let port2 = free_port();
    let _restarted = Killed(spawn(&config, port2));
    wait_healthy(port2).await;

    let mut delivered_again = false;
    for _ in 0..90 {
        if receiver.received().len() >= 2 {
            delivered_again = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    assert!(
        delivered_again,
        "after a hard kill the message must come back and be delivered — \
         it was seen {} time(s)",
        receiver.received().len()
    );
    let bodies: Vec<String> = receiver.received().iter().map(|r| r.body.clone()).collect();
    for body in &bodies {
        assert_eq!(
            body, r#"{"alert": "SurvivesTheKill"}"#,
            "and it must be the same message, unchanged"
        );
    }
}
