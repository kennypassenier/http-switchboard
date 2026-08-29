//! Phase 7 — the wrapping, not the app (G9, and part of G7).
//!
//! The deployment artifacts were touched by no test: the container's
//! healthcheck depends on `--healthcheck`, which nothing ran, and a
//! broken one presents as a restart loop inside an image with no shell to
//! debug it. The CLI verbs are the runbook's own procedures, so a
//! regression there lands in the emergency path.

mod support;

use std::io::Write;
use std::process::{Child, Command, Stdio};

use support::TestServer;

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

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn tempdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hsw-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn ar13_the_healthcheck_flag_answers_for_a_live_and_a_dead_service() {
    // The container has no shell and no curl, so this flag IS the
    // healthcheck. It had never been run by a test.
    let receiver = TestServer::start(vec![200]).await;
    let dir = tempdir("healthcheck");
    let config = dir.join("config.toml");
    let mut file = std::fs::File::create(&config).unwrap();
    write!(
        file,
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
body = '''{{"x": {{{{ x }}}}}}'''
"#,
        receiver.base_url
    )
    .unwrap();
    drop(file);

    let port = free_port();
    let url = format!("http://127.0.0.1:{port}/healthz");

    // Dead: nothing is listening yet.
    let out = Command::new(binary())
        .args(["--healthcheck", &url])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a dead service must fail the check");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("unhealthy"),
        "and say so: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Live.
    let _child = Killed(
        Command::new(binary())
            .arg(&config)
            .arg("--listen")
            .arg(format!("127.0.0.1:{port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for _ in 0..80 {
        if reqwest::get(&url)
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    let out = Command::new(binary())
        .args(["--healthcheck", &url])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "a live service must pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn k8_no_secret_reaches_the_log_of_a_running_service() {
    // W7's bar says the secret scan runs over the log form too. It was
    // asserted on config errors and on sink errors, never on what the
    // running binary actually prints (Phase 7, G7).
    let dir = tempdir("logscan");
    let config = dir.join("config.toml");
    let mut file = std::fs::File::create(&config).unwrap();
    write!(
        file,
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "http://127.0.0.1:9/in" }}
content_type = "application/json"
retries = 0
timeout_ms = 500
headers = {{ authorization = "${{TEST_HEADER_SECRET}}" }}
body = '''{{"x": {{{{ x }}}}}}'''
"#
    )
    .unwrap();
    drop(file);

    let port = free_port();
    let child = Command::new(binary())
        .arg(&config)
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .env("TEST_HEADER_SECRET", "Bearer never-print-this-value")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Not wrapped in Killed here: this test needs to take the output, so
    // it owns the child and kills it itself.
    let mut child = child;

    for _ in 0..80 {
        if reqwest::get(format!("http://127.0.0.1:{port}/healthz"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    // A delivery that fails, so the failure paths print too.
    let _ = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/hook"))
        .header("content-type", "application/json")
        .body(r#"{"x": 1}"#)
        .send()
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    child.kill().unwrap();
    let out = child.wait_with_output().unwrap();
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !printed.contains("never-print-this-value"),
        "a secret reached the log of the running service: {printed}"
    );
    assert!(
        printed.contains(r#""outcome":"failed""#),
        "the failure should have been logged at all: {printed}"
    );
}

#[test]
fn k10_the_command_line_fails_closed_and_says_what_to_do() {
    // The runbook's procedures are exactly these verbs, so a regression
    // here lands in the emergency path (Phase 7, G7).
    let dir = tempdir("cli");

    // No arguments: usage, non-zero.
    let out = Command::new(binary()).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("http-switchboard"));

    // A config file that does not exist.
    let out = Command::new(binary())
        .arg(dir.join("nowhere.toml"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");
    assert!(stderr.contains("nowhere.toml"), "{stderr}");

    // A listen address that is not one.
    let config = dir.join("ok.toml");
    std::fs::write(
        &config,
        r#"
[[profiles]]
name = "hook"
from = { http_path = "/hook" }
to = { url = "http://127.0.0.1:9/in" }
content_type = "application/json"
body = '''{"x": 1}'''
"#,
    )
    .unwrap();
    let out = Command::new(binary())
        .arg(&config)
        .args(["--listen", "not-an-address"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");

    // An unknown option on the test verb.
    let out = Command::new(binary())
        .args(["test", "--nonsense", "x"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nonsense"));
}
