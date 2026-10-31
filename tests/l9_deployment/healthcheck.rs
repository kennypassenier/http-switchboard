use super::*;

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
    let said = String::from_utf8_lossy(&out.stderr);
    // 2.0.0: the kit's --healthcheck words it "… failed … What now: …".
    assert!(
        said.contains("unhealthy") || said.contains("failed"),
        "and say so: {said}"
    );

    // Live.
    let _child = Killed(
        Command::new(binary())
            .args(["--config"])
            .arg(&config)
            .args(["--state-dir"])
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
