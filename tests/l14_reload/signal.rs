use super::*;

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
