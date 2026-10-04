// Each test binary compiles this whole module, so a double only one
// suite needs looks unused to the others.
#![allow(dead_code)]

//! Test doubles shared by the milestone suites.
//!
//! What these fakes cannot express, written down as standing rule 9
//! demands:
//!
//! * `FakeClock` does not pass time — it records what it was asked to
//!   wait. Anything that depends on real elapsed time (a lease actually
//!   expiring on the hub) needs the E2E suite against a real kyu, not
//!   this.
//! * `TestServer` speaks just enough HTTP to answer with a status and
//!   record what arrived. It does not do chunked encoding, keep-alive,
//!   redirects or TLS, so nothing here proves anything about those.

use std::sync::{Arc, Mutex};

use http_switchboard::adapters::{BoxFuture, Clock};

/// A clock that never sleeps and remembers every pause it was asked for.
#[derive(Default, Clone)]
pub struct FakeClock {
    pauses: Arc<Mutex<Vec<u64>>>,
}

impl FakeClock {
    pub fn pauses(&self) -> Vec<u64> {
        self.pauses.lock().unwrap().clone()
    }
}

impl Clock for FakeClock {
    fn sleep(&self, ms: u64) -> BoxFuture<'static, ()> {
        self.pauses.lock().unwrap().push(ms);
        Box::pin(std::future::ready(()))
    }
}

/// One request as the fake receiver saw it.
#[derive(Debug, Clone)]
pub struct Received {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Received {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A minimal HTTP server for tests: answers each request with the next
/// status from `plan` (repeating the last), or hangs forever when the
/// status is 0 — the "never answers" case W2 exists for.
pub struct TestServer {
    pub base_url: String,
    received: Arc<Mutex<Vec<Received>>>,
}

impl TestServer {
    pub async fn start(plan: Vec<u16>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&received);

        tokio::spawn(async move {
            let mut n = 0usize;
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let status = *plan.get(n).unwrap_or(plan.last().unwrap_or(&200));
                n += 1;
                let sink = Arc::clone(&sink);
                tokio::spawn(async move {
                    handle(stream, status, sink).await;
                });
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            received,
        }
    }

    pub fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }
}

async fn handle(mut stream: tokio::net::TcpStream, status: u16, sink: Arc<Mutex<Vec<Received>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    // Read until the headers are complete, then as much body as the
    // content-length announces.
    loop {
        let Ok(n) = stream.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(head_end) = find(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let len = head
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if buf.len() >= head_end + 4 + len {
                let mut lines = head.lines();
                let request_line = lines.next().unwrap_or_default().to_string();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let path = parts.next().unwrap_or_default().to_string();
                let headers = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
                    .collect();
                let body =
                    String::from_utf8_lossy(&buf[head_end + 4..head_end + 4 + len]).to_string();
                sink.lock().unwrap().push(Received {
                    method,
                    path,
                    headers,
                    body,
                });
                break;
            }
        }
    }

    if status == 0 {
        // Never answer. The client's timeout is the thing under test.
        std::future::pending::<()>().await;
    }

    let _ = stream
        .write_all(format!("HTTP/1.1 {status} X\r\ncontent-length: 0\r\n\r\n").as_bytes())
        .await;
    let _ = stream.flush().await;
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ── hub doubles and the real-hub harness ───────────────────────────────

use http_switchboard::pump::{Hub, HubError, Poll};

/// A scripted hub. Answers each poll from `polls` in order (repeating the
/// last), and records every settle call so the ORDER of deliver-then-ack
/// can be asserted — which is the thing that actually loses messages.
pub struct FakeHub {
    polls: Mutex<std::collections::VecDeque<Result<Poll, HubErrorKind>>>,
    pub calls: Arc<Mutex<Vec<String>>>,
    pub last_from_beginning: Arc<Mutex<Option<bool>>>,
}

/// `HubError` is not `Clone`, so the script names the kind and the fake
/// builds the error.
#[derive(Debug, Clone, Copy)]
pub enum HubErrorKind {
    Denied,
    Unreachable,
}

impl FakeHub {
    pub fn new(polls: Vec<Result<Poll, HubErrorKind>>) -> Self {
        Self::with_calls(polls, Arc::new(Mutex::new(Vec::new())))
    }

    /// Share the call log with a sink, so the ORDER of deliver and ack can
    /// be asserted across both — which is the whole point of L4.
    pub fn with_calls(
        polls: Vec<Result<Poll, HubErrorKind>>,
        calls: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            polls: Mutex::new(polls.into()),
            calls,
            last_from_beginning: Arc::new(Mutex::new(None)),
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Hub for FakeHub {
    fn next<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        from_beginning: bool,
    ) -> BoxFuture<'a, Result<Poll, HubError>> {
        *self.last_from_beginning.lock().unwrap() = Some(from_beginning);
        self.calls
            .lock()
            .unwrap()
            .push(format!("next(from_beginning={from_beginning})"));
        let next = self
            .polls
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(Poll::Empty));
        Box::pin(async move {
            match next {
                Ok(p) => Ok(p),
                Err(HubErrorKind::Denied) => Err(HubError::Denied),
                Err(HubErrorKind::Unreachable) => Err(HubError::Unreachable {
                    detail: "connection refused".into(),
                }),
            }
        })
    }

    fn ack<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), HubError>> {
        self.calls.lock().unwrap().push(format!("ack({id})"));
        Box::pin(async { Ok(()) })
    }

    fn nack<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        id: &'a str,
        dead: bool,
    ) -> BoxFuture<'a, Result<(), HubError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("nack({id}, dead={dead})"));
        Box::pin(async { Ok(()) })
    }

    fn set_policy<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        lease_ms: u64,
        max_attempts: u32,
    ) -> BoxFuture<'a, Result<(), HubError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("policy(lease={lease_ms}, attempts={max_attempts})"));
        Box::pin(async { Ok(()) })
    }
}

/// A sink that records what it was given and answers from a script.
pub struct FakeSink {
    plan: Mutex<std::collections::VecDeque<bool>>,
    pub calls: Arc<Mutex<Vec<String>>>,
}

impl FakeSink {
    /// `plan` is one boolean per attempt: true = accepted.
    pub fn new(plan: Vec<bool>, calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            plan: Mutex::new(plan.into()),
            calls,
        }
    }
}

impl http_switchboard::adapters::Sink for FakeSink {
    fn deliver<'a>(
        &'a self,
        delivery: &'a http_switchboard::translate::Delivery,
    ) -> BoxFuture<'a, Result<(), http_switchboard::adapters::DeliverError>> {
        let ok = self.plan.lock().unwrap().pop_front().unwrap_or(true);
        self.calls
            .lock()
            .unwrap()
            .push(format!("deliver(ok={ok}, body={})", delivery.body));
        Box::pin(async move {
            if ok {
                Ok(())
            } else {
                Err(http_switchboard::adapters::DeliverError::Status {
                    status: 503,
                    advice: "test".into(),
                    answer: String::new(),
                })
            }
        })
    }
}

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
