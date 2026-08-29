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

    #[error(
        "the destination refused the message with status {status}.{answer} What now: {advice}"
    )]
    Status {
        status: u16,
        advice: String,
        /// The receiver's own words, when the profile asked for them
        /// (W12). Empty otherwise — a receiver's error page is not ours
        /// to hand on by default.
        answer: String,
    },

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
/// How much of a receiver's error text is ever passed on. Bounded because
/// it is text we did not write and cannot vouch for.
const MAX_FORWARDED_ANSWER: usize = 512;

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

/// Trim a receiver's answer to something safe to repeat: bounded, one
/// line, no control characters.
fn readable_answer(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out: String = trimmed.chars().take(MAX_FORWARDED_ANSWER).collect();
    if trimmed.chars().count() > MAX_FORWARDED_ANSWER {
        out.push('…');
    }
    format!(" It said: {out}.")
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
                        // Only read the receiver's words when the profile
                        // asked for them; otherwise they are not ours to
                        // carry around (W12).
                        let answer = if delivery.forward_error_body {
                            readable_answer(&response.text().await.unwrap_or_default())
                        } else {
                            String::new()
                        };
                        Err(DeliverError::Status {
                            status,
                            advice: advice_for(status),
                            answer,
                        })
                    }
                }
                Err(e) if e.is_timeout() => Err(DeliverError::Timeout {
                    timeout_ms: self.timeout_ms,
                }),
                // The URL is stripped before the error escapes. reqwest's
                // Display appends "for url (…)", and this text travels back
                // to whoever sent the message — who is not supposed to learn
                // the destination at all, let alone a Home Assistant webhook
                // id, which IS the credential. Found twice in the Phase 7
                // pass: by the test-gap audit and by the security review,
                // independently.
                Err(e) => Err(DeliverError::Transport {
                    detail: format!("{}", e.without_url()),
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

// ── the hub client ─────────────────────────────────────────────────────

/// The three verbs plus the policy write, over plain HTTP. Contract
/// measured against a running kyu 2.0.0 rather than read off a page:
/// `204` means nothing was waiting, `404` means the topic does not exist
/// yet, and `envelope=json` answers with `{id, payload, attempt, …}`.
pub struct KyuHub {
    client: reqwest::Client,
    base_url: String,
    token: Option<Secret>,
    wait_s: u64,
}

impl KyuHub {
    pub fn new(base_url: String, token: Option<Secret>, wait_s: u64) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            wait_s,
        }
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => req.header("authorization", format!("Bearer {}", t.expose())),
            None => req,
        }
    }

    async fn settle(&self, url: String) -> Result<(), crate::pump::HubError> {
        let response = self
            .authed(self.client.post(&url))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| crate::pump::HubError::Unreachable {
                detail: e.to_string(),
            })?;
        match response.status().as_u16() {
            200..=299 => Ok(()),
            401 | 403 => Err(crate::pump::HubError::Denied),
            status => Err(crate::pump::HubError::Status { status }),
        }
    }
}

impl crate::pump::Hub for KyuHub {
    fn next<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        from_beginning: bool,
    ) -> BoxFuture<'a, Result<crate::pump::Poll, crate::pump::HubError>> {
        use crate::pump::{HubError, HubMessage, Poll};
        Box::pin(async move {
            let mut url = format!(
                "{}/t/{topic}/next?as={subscription}&envelope=json&wait={}",
                self.base_url, self.wait_s
            );
            if from_beginning {
                url.push_str("&from=beginning");
            }

            let response = self
                .authed(self.client.get(&url))
                // The long poll may legitimately take `wait_s`; the client
                // timeout has to outlive it or every poll is a "failure".
                .timeout(Duration::from_secs(self.wait_s + 10))
                .send()
                .await
                .map_err(|e| HubError::Unreachable {
                    detail: e.to_string(),
                })?;

            match response.status().as_u16() {
                204 => Ok(Poll::Empty),
                404 => Ok(Poll::UnknownTopic),
                401 | 403 => Err(HubError::Denied),
                200 => {
                    // Parsed with serde_json rather than reqwest's json
                    // feature: one fewer feature for one line of code.
                    let raw = response.bytes().await.map_err(|e| HubError::Unreachable {
                        detail: format!("the hub's answer could not be read: {e}"),
                    })?;
                    let envelope: serde_json::Value =
                        serde_json::from_slice(&raw).map_err(|e| HubError::Unreachable {
                            detail: format!("the hub's answer was not JSON: {e}"),
                        })?;
                    let id = envelope
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    match envelope.get("payload") {
                        Some(payload) => Ok(Poll::Message(Box::new(HubMessage {
                            id,
                            payload: payload.clone(),
                            attempt: envelope
                                .get("attempt")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(1) as u32,
                        }))),
                        // kyu hands a non-JSON body over under another key
                        // (measured: payload_text for text bodies); either
                        // way there is no `payload` for a template to read.
                        None => Ok(Poll::NotJson { id }),
                    }
                }
                status => Err(HubError::Status { status }),
            }
        })
    }

    fn ack<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), crate::pump::HubError>> {
        Box::pin(self.settle(format!(
            "{}/t/{topic}/ack/{id}?as={subscription}",
            self.base_url
        )))
    }

    fn nack<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        id: &'a str,
        dead: bool,
    ) -> BoxFuture<'a, Result<(), crate::pump::HubError>> {
        let dead = if dead { "&dead=true" } else { "" };
        Box::pin(self.settle(format!(
            "{}/t/{topic}/nack/{id}?as={subscription}{dead}",
            self.base_url
        )))
    }

    fn set_policy<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        lease_ms: u64,
        max_attempts: u32,
    ) -> BoxFuture<'a, Result<(), crate::pump::HubError>> {
        Box::pin(async move {
            let url = format!("{}/api/t/{topic}/subs/{subscription}/policy", self.base_url);
            let response = self
                .authed(self.client.put(&url))
                .timeout(Duration::from_secs(10))
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{"lease_ms":{lease_ms},"max_attempts":{max_attempts}}}"#
                ))
                .send()
                .await
                .map_err(|e| crate::pump::HubError::Unreachable {
                    detail: e.to_string(),
                })?;
            match response.status().as_u16() {
                200..=299 => Ok(()),
                401 | 403 => Err(crate::pump::HubError::Denied),
                status => Err(crate::pump::HubError::Status { status }),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn k11_every_status_remedy_is_present_and_says_something_different() {
        // Phase 7, G7: five branches existed, tests rendered two — and not
        // the three an operator actually meets.
        let cases = [401, 403, 404, 413, 400, 422, 500, 503];
        let mut seen: Vec<String> = Vec::new();
        for status in cases {
            let e = DeliverError::Status {
                status,
                advice: advice_for(status),
                answer: String::new(),
            };
            let message = e.to_string();
            assert!(
                message.contains(&status.to_string()),
                "the status must be in the message: {message}"
            );
            assert!(
                message.len() > 60,
                "a remedy must actually say something: {message}"
            );
            seen.push(advice_for(status));
        }
        // The credential, the unknown address and the too-large body each
        // get their own advice rather than the generic one.
        assert_ne!(seen[0], seen[3], "401 and 413 must not share advice");
        assert_ne!(
            seen[2], seen[4],
            "404 and a generic 4xx must not share advice"
        );
        assert_ne!(seen[4], seen[6], "a 4xx and a 5xx must not share advice");
        assert_eq!(seen[0], seen[1], "401 and 403 are the same problem");
    }

    #[test]
    fn w3_only_the_errors_worth_retrying_are_retried() {
        assert!(DeliverError::Timeout { timeout_ms: 1 }.is_retryable());
        assert!(DeliverError::Transport { detail: "x".into() }.is_retryable());
        for status in [500, 502, 503, 429] {
            assert!(
                DeliverError::Status {
                    status,
                    advice: String::new(),
                    answer: String::new()
                }
                .is_retryable(),
                "{status} should be retried"
            );
        }
        for status in [400, 401, 403, 404, 413, 422] {
            assert!(
                !DeliverError::Status {
                    status,
                    advice: String::new(),
                    answer: String::new()
                }
                .is_retryable(),
                "{status} will not get better by asking again"
            );
        }
    }
}
