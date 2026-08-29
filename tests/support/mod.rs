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
