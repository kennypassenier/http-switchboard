//! Everything that touches the outside world (L3-L5).
//!
//! Sinks, sources and the clock sit behind traits (AR1). The reason is
//! not tidiness: messages are lost in the ordering of deliver, retry and
//! acknowledge, and that ordering has to be testable with fakes. A pure
//! template core alone would have proven the wrong half.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::config::Profile;
use crate::secret::Secret;
use crate::translate::{Delivery, Target};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Waiting, made injectable so W3's bar — "pauses measured with a mocked
/// clock, not by waiting" — is reachable.
pub trait Clock: Send + Sync {
    fn sleep(&self, ms: u64) -> BoxFuture<'static, ()>;
}

/// The real one.
pub struct TokioClock;

impl Clock for TokioClock {
    fn sleep(&self, ms: u64) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(Duration::from_millis(ms)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeliverError {
    #[error("the destination did not answer within {timeout_ms} ms. What now: check whether the receiver is up and reachable from this container; if it is simply slow, raise timeout_ms for this profile — but keep the retry budget inside the kyu lease.")]
    Timeout { timeout_ms: u64 },

    #[error("the destination refused the message with status {status}. What now: {advice}")]
    Status { status: u16, advice: String },

    #[error("the destination could not be reached: {detail}. What now: check the address in the profile and whether this container can reach it — a name that does not resolve and a port nothing listens on look the same from here.")]
    Transport { detail: String },
}

impl DeliverError {
    /// Whether trying again could plausibly help. A 400 will be a 400
    /// again; a 503 or a timeout may not be.
    pub fn is_retryable(&self) -> bool {
        match self {
            DeliverError::Timeout { .. } | DeliverError::Transport { .. } => true,
            DeliverError::Status { status, .. } => *status >= 500 || *status == 429,
        }
    }
}

fn advice_for(status: u16) -> String {
    match status {
        401 | 403 => "the receiver rejected our credentials — check the header this profile sends, and whether the token behind it was rotated without restarting this service.".to_string(),
        404 => "the receiver does not know this address — check the URL, or, for a Home Assistant webhook, that the automation with that webhook id still exists.".to_string(),
        413 => "the receiver considers the message too large — shorten the template, or raise the receiver's limit.".to_string(),
        400..=499 => "the receiver considers the message wrong, and it will keep doing so — look at the rendered body with `http-switchboard test` before retrying.".to_string(),
        _ => "the receiver is having trouble; this is retried automatically, and if it persists the message stays on the hub rather than disappearing.".to_string(),
    }
}

/// Somewhere a translated message can be delivered.
pub trait Sink: Send + Sync {
    fn deliver<'a>(&'a self, delivery: &'a Delivery) -> BoxFuture<'a, Result<(), DeliverError>>;
}

/// Delivery over HTTP: both to a plain URL and to a kyu topic, because
/// publishing to the hub is an ordinary POST — one client, one timeout,
/// one set of failure modes.
pub struct HttpSink {
    client: reqwest::Client,
    kyu_base_url: Option<String>,
    kyu_token: Option<Secret>,
    timeout_ms: u64,
}

impl HttpSink {
    pub fn new(kyu_base_url: Option<String>, kyu_token: Option<Secret>, timeout_ms: u64) -> Self {
        Self {
            client: reqwest::Client::new(),
            kyu_base_url,
            kyu_token,
            timeout_ms,
        }
    }

    fn url_and_auth(&self, delivery: &Delivery) -> (String, String, Option<Secret>) {
        match &delivery.target {
            Target::Url { url, method } => (url.clone(), method.clone(), None),
            Target::KyuTopic { topic } => (
                format!(
                    "{}/t/{topic}",
                    self.kyu_base_url.as_deref().unwrap_or_default()
                ),
                "POST".to_string(),
                self.kyu_token.clone(),
            ),
        }
    }
}

impl Sink for HttpSink {
    fn deliver<'a>(&'a self, delivery: &'a Delivery) -> BoxFuture<'a, Result<(), DeliverError>> {
        Box::pin(async move {
            let (url, method, auth) = self.url_and_auth(delivery);
            let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| {
                DeliverError::Transport {
                    detail: format!("'{method}' is not an HTTP method ({e})"),
                }
            })?;

            let mut req = self
                .client
                .request(method, &url)
                .timeout(Duration::from_millis(self.timeout_ms))
                .header("content-type", &delivery.content_type)
                .body(delivery.body.clone().into_bytes());

            for (name, value) in &delivery.headers {
                req = req.header(name, value.expose());
            }
            if let Some(token) = auth {
                req = req.header("authorization", format!("Bearer {}", token.expose()));
            }

            match req.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if (200..300).contains(&status) {
                        Ok(())
                    } else {
                        Err(DeliverError::Status {
                            status,
                            advice: advice_for(status),
                        })
                    }
                }
                Err(e) if e.is_timeout() => Err(DeliverError::Timeout {
                    timeout_ms: self.timeout_ms,
                }),
                // The message never carries the URL, so an error may name
                // it; it never carries a header value, which may be secret.
                Err(e) => Err(DeliverError::Transport {
                    detail: format!("{e}"),
                }),
            }
        })
    }
}

/// What happened to one message, for the caller that has to decide
/// whether to acknowledge it (L4) or what to answer the sender (L5).
#[derive(Debug)]
pub struct Outcome {
    pub attempts: u32,
    pub result: Result<(), DeliverError>,
}

/// Deliver, retrying inside the same claim (AR8). Retrying here rather
/// than handing the message back is what stops kyu's five attempts from
/// being burned by a two-minute restart of the receiver.
pub async fn deliver_with_retry(
    profile: &Profile,
    delivery: &Delivery,
    sink: &dyn Sink,
    clock: &dyn Clock,
) -> Outcome {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match sink.deliver(delivery).await {
            Ok(()) => {
                return Outcome {
                    attempts,
                    result: Ok(()),
                }
            }
            Err(e) => {
                let exhausted = attempts > profile.retries;
                if exhausted || !e.is_retryable() {
                    return Outcome {
                        attempts,
                        result: Err(e),
                    };
                }
                // 1 s, 2 s, 4 s … the same series the config's budget
                // check accounts for, so the two cannot drift apart.
                clock.sleep(1_000 * 2u64.pow(attempts - 1)).await;
            }
        }
    }
}
