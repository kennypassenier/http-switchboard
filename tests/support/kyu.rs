//! The real-hub harness: a real kyu for the E2E suites.

/// A real kyu for the E2E bar the feature list demands ("against a real
/// kyu, not a mock"): the newest signed kyu release, fetched, verified and
/// cached by the workstation's kyu-latest (Kenny, 2026-10-04: always the
/// latest kyu, never a pinned image), or the binary at KYU_BIN. It starts
/// with an admin token and issues this harness a client token, the only
/// door kyu has had since 3.0.0.
pub struct KyuHarness {
    pub base_url: String,
    /// The client token publishers and pumps send as their bearer.
    pub token: String,
    child: std::process::Child,
    dir: std::path::PathBuf,
}

/// The operator token the hub starts with; `/api/…` management routes need it.
pub const KYU_ADMIN_TOKEN: &str = "switchboard-suite-admin-token-0123456789";
const KYU_SECRET_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn kyu_binary() -> std::path::PathBuf {
    static BIN: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    BIN.get_or_init(|| {
        if let Ok(path) = std::env::var("KYU_BIN") {
            if !path.is_empty() {
                return path.into();
            }
        }
        let helper = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"))
            .join("Projects/workstation/bin/kyu-latest");
        let out = std::process::Command::new(&helper)
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}; set KYU_BIN to a kyu binary", helper.display()));
        assert!(
            out.status.success(),
            "kyu-latest failed: {}; set KYU_BIN to a kyu binary",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().into()
    })
    .clone()
}

impl KyuHarness {
    /// Always `Some` now; the `Option` stays so the call sites read the same.
    pub async fn start() -> Option<Self> {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kyu-e2e-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("kyu state dir");
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free port")
            .port();
        let log = std::fs::File::create(dir.join("kyu.log")).expect("kyu log");
        let child = std::process::Command::new(kyu_binary())
            .env("KYU_LISTEN", format!("127.0.0.1:{port}"))
            .env("KYU_STATE_DIR", dir.join("state"))
            .env("KYU_LOG", "warn")
            .env("KYU_TOKEN", KYU_ADMIN_TOKEN)
            .env("KYU_SECRET_KEY", KYU_SECRET_KEY)
            .stdout(std::process::Stdio::null())
            .stderr(log)
            .spawn()
            .expect("spawn kyu");
        let base_url = format!("http://127.0.0.1:{port}");

        let client = reqwest::Client::new();
        let mut healthy = false;
        for _ in 0..80 {
            if let Ok(r) = client.get(format!("{base_url}/healthz")).send().await {
                if r.status().is_success() {
                    healthy = true;
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let stderr = || std::fs::read_to_string(dir.join("kyu.log")).unwrap_or_default();
        assert!(healthy, "kyu did not become healthy in 20 s: {}", stderr());
        let api = chassis::admin::AdminApi::new(&base_url, KYU_ADMIN_TOKEN)
            .expect("the hub's admin API client");
        let token = api
            .issue_client("http-switchboard-suite", &[])
            .await
            .unwrap_or_else(|e| panic!("kyu refused a client token: {e}; {}", stderr()))
            .token;
        Some(Self {
            base_url,
            token,
            child,
            dir,
        })
    }

    /// The hub as the pump sees it, with this harness's client token.
    pub fn hub(&self, wait_s: u64) -> http_switchboard::adapters::KyuHub {
        http_switchboard::adapters::KyuHub::new(
            self.base_url.clone(),
            Some(http_switchboard::secret::Secret::new(self.token.clone())),
            wait_s,
        )
    }

    pub async fn publish(&self, topic: &str, body: &str) {
        let response = reqwest::Client::new()
            .post(format!("{}/t/{topic}", self.base_url))
            .bearer_auth(&self.token)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("publish must reach the hub");
        assert!(
            response.status().is_success(),
            "publish failed: {}",
            response.status()
        );
    }
}

impl Drop for KyuHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
