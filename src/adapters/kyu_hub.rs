//! The hub client: the three verbs plus the policy write, over plain HTTP.

use std::time::Duration;

use super::BoxFuture;
use crate::pump::{Hub, HubError, HubMessage, Poll};
use crate::secret::Secret;

/// How long a short hub call (ack, nack, policy write) may take.
const HUB_REQUEST_TIMEOUT_S: u64 = 10;

/// Extra time the client timeout of a long poll gets beyond the poll's own
/// wait, so a poll that legitimately runs its full wait is not a failure.
const LONG_POLL_MARGIN_S: u64 = 10;

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

    /// POST an ack or nack URL and map the status to the hub's errors.
    async fn settle(&self, url: String) -> Result<(), HubError> {
        let response = self
            .authed(self.client.post(&url))
            .timeout(Duration::from_secs(HUB_REQUEST_TIMEOUT_S))
            .send()
            .await
            .map_err(|e| HubError::Unreachable {
                detail: e.to_string(),
            })?;
        match response.status().as_u16() {
            200..=299 => Ok(()),
            401 | 403 => Err(HubError::Denied),
            status => Err(HubError::Status { status }),
        }
    }

    /// One long poll for the next message.
    async fn poll(
        &self,
        topic: &str,
        subscription: &str,
        from_beginning: bool,
    ) -> Result<Poll, HubError> {
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
            .timeout(Duration::from_secs(self.wait_s + LONG_POLL_MARGIN_S))
            .send()
            .await
            .map_err(|e| HubError::Unreachable {
                detail: e.to_string(),
            })?;
        match response.status().as_u16() {
            204 => Ok(Poll::Empty),
            404 => Ok(Poll::UnknownTopic),
            401 | 403 => Err(HubError::Denied),
            200 => parse_envelope(response).await,
            status => Err(HubError::Status { status }),
        }
    }
}

/// Read a 200 answer of the long poll into a message.
async fn parse_envelope(response: reqwest::Response) -> Result<Poll, HubError> {
    // Parsed with serde_json rather than reqwest's json feature: one
    // fewer feature for one line of code.
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
        // kyu hands a non-JSON body over under another key (measured:
        // payload_text for text bodies); either way there is no `payload`
        // for a template to read.
        None => Ok(Poll::NotJson { id }),
    }
}

impl Hub for KyuHub {
    fn next<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        from_beginning: bool,
    ) -> BoxFuture<'a, Result<Poll, HubError>> {
        Box::pin(self.poll(topic, subscription, from_beginning))
    }

    fn ack<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), HubError>> {
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
    ) -> BoxFuture<'a, Result<(), HubError>> {
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
    ) -> BoxFuture<'a, Result<(), HubError>> {
        Box::pin(async move {
            let url = format!("{}/api/t/{topic}/subs/{subscription}/policy", self.base_url);
            let response = self
                .authed(self.client.put(&url))
                .timeout(Duration::from_secs(HUB_REQUEST_TIMEOUT_S))
                .header("content-type", "application/json")
                .body(format!(
                    r#"{{"lease_ms":{lease_ms},"max_attempts":{max_attempts}}}"#
                ))
                .send()
                .await
                .map_err(|e| HubError::Unreachable {
                    detail: e.to_string(),
                })?;
            match response.status().as_u16() {
                200..=299 => Ok(()),
                401 | 403 => Err(HubError::Denied),
                status => Err(HubError::Status { status }),
            }
        })
    }
}
