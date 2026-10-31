//! Delivery over HTTP: both to a plain URL and to a kyu topic, because
//! publishing to the hub is an ordinary POST (one client, one timeout,
//! one set of failure modes).

use std::time::Duration;

use super::error::{advice_for, DeliverError};
use super::{BoxFuture, Sink};
use crate::secret::Secret;
use crate::translate::{Delivery, Target};

/// How much of a receiver's error text is ever passed on. Bounded because
/// it is text we did not write and cannot vouch for.
const MAX_FORWARDED_ANSWER: usize = 512;

/// The HTTP method for a publish to a kyu topic: always a POST.
const KYU_PUBLISH_METHOD: &str = "POST";

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
                KYU_PUBLISH_METHOD.to_string(),
                self.kyu_token.clone(),
            ),
        }
    }

    /// Build the request for `delivery`: method, headers, body, and the
    /// hub token for a kyu topic.
    fn build_request(&self, delivery: &Delivery) -> Result<reqwest::RequestBuilder, DeliverError> {
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
        Ok(req)
    }

    /// Turn a non-2xx response into the error that carries its advice.
    async fn refusal(&self, delivery: &Delivery, response: reqwest::Response) -> DeliverError {
        let status = response.status().as_u16();
        // Only read the receiver's words when the profile asked for them;
        // otherwise they are not ours to carry around (W12).
        let answer = if delivery.forward_error_body {
            match response.text().await {
                Ok(text) => readable_answer(&text),
                // The status still decides the outcome; the sender is told the
                // body could not be read instead of getting a silent blank.
                Err(e) => format!("(the receiver's answer could not be read: {e})"),
            }
        } else {
            String::new()
        };
        DeliverError::Status {
            status,
            advice: advice_for(status),
            answer,
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
            let req = self.build_request(delivery)?;
            match req.send().await {
                Ok(response) if response.status().is_success() => Ok(()),
                Ok(response) => Err(self.refusal(delivery, response).await),
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
