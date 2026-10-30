use super::*;

#[tokio::test]
async fn k8_no_secret_reaches_the_log_of_a_running_service() {
    // W7's bar says the secret scan runs over the log form too. It was
    // asserted on config errors and on sink errors, never on what the
    // running binary actually prints (Phase 7, G7).
    let dir = tempdir("logscan");
    // A port nothing listens on, so the delivery is refused at once. Port 9
    // was used until 2026-10-04: on WSL it drops the SYN instead of
    // refusing it, and the attempt took 30 s to fail.
    let dead = free_port();
    let config = dir.join("config.toml");
    let mut file = std::fs::File::create(&config).unwrap();
    write!(
        file,
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "http://127.0.0.1:{dead}/in" }}
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
        .args(["--config"])
        .arg(&config)
        .args(["--state-dir"])
        .arg(&dir)
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .env("TEST_HEADER_SECRET", "Bearer never-print-this-value")
        // D-H1: the switchboard's own events and the kit's lines share one
        // JSON shape; this test reads that shape.
        .env("HTTP_SWITCHBOARD_LOG_FORMAT", "json")
        .env("HTTP_SWITCHBOARD_TOKEN", TEST_TOKEN)
        .env("HTTP_SWITCHBOARD_SECRET_KEY", TEST_SECRET_KEY)
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

    // A delivery that fails, so the failure paths print too. Since 3.0.0
    // the profile paths are behind the kit's door (H1), and the admin
    // token is the bearer an operator's own script would use — issuing a
    // sender token needs the dashboard, which is not what this test is
    // about.
    let _ = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/hook"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {TEST_TOKEN}"))
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
    // D-H1: every line the running service printed is one JSON object of
    // the kit's shape, and the switchboard's delivery event is among them
    // with its fields under `fields` — one shape for Loki.
    let objects: Vec<serde_json::Value> = printed
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {e}\n{l}")))
        .collect();
    assert!(
        !objects.is_empty(),
        "json mode must emit JSON lines: {printed}"
    );
    let delivery = objects
        .iter()
        .find(|o| o["fields"]["outcome"] == "failed")
        .unwrap_or_else(|| panic!("no delivery event with fields.outcome=failed: {printed}"));
    assert_eq!(delivery["fields"]["profile"], "hook");
    assert!(delivery["level"].is_string(), "{delivery}");
    assert!(delivery["timestamp"].is_string(), "{delivery}");
    assert!(
        printed.contains(r#""outcome":"failed""#),
        "the failure should have been logged at all: {printed}"
    );
}
