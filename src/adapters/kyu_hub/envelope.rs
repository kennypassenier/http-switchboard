//! Reading and parsing the hub's `envelope=json` answer: a bounded read,
//! the message id of an over-size answer, and the envelope itself.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::pump::{HubError, HubMessage, Poll};

/// The most of a hub answer that is read into memory: the same bound the
/// inbound door puts on a request body. A bigger message cannot be
/// delivered anyway (the rendered-body cap is the same size).
const MAX_ENVELOPE_BYTES: usize = crate::inbound::MAX_BODY_BYTES;

/// How much of an over-size answer is looked at to find the message id.
const ID_SCAN_BYTES: usize = 64 * 1024;

/// What reading a bounded answer produced.
enum Body {
    Whole(Vec<u8>),
    /// Over the limit: only the start was kept, the rest was dropped unread.
    TooLarge(Vec<u8>),
}

/// Read the answer chunk by chunk, stopping at the limit.
async fn read_capped(mut response: reqwest::Response) -> Result<Body, HubError> {
    let mut raw: Vec<u8> = Vec::new();
    loop {
        let chunk = response.chunk().await.map_err(|e| HubError::Unreachable {
            detail: format!("the hub's answer could not be read: {e}"),
        })?;
        let Some(chunk) = chunk else {
            return Ok(Body::Whole(raw));
        };
        if raw.len() + chunk.len() > MAX_ENVELOPE_BYTES {
            let keep = chunk.len().min(ID_SCAN_BYTES.saturating_sub(raw.len()));
            raw.extend_from_slice(&chunk[..keep]);
            raw.truncate(ID_SCAN_BYTES);
            return Ok(Body::TooLarge(raw));
        }
        raw.extend_from_slice(&chunk);
    }
}

/// Read a 200 answer of the long poll into a message.
pub(super) async fn parse_envelope(response: reqwest::Response) -> Result<Poll, HubError> {
    match read_capped(response).await? {
        Body::Whole(raw) => parse_envelope_bytes(&raw),
        Body::TooLarge(prefix) => match scan_id(&prefix) {
            Some(id) => Ok(Poll::TooLarge { id }),
            None => {
                crate::obs::log_warn(
                    "hub",
                    "bad_envelope",
                    "an over-size hub answer carries no message id before its payload",
                );
                Err(HubError::Unreachable {
                    detail: "the hub's answer is larger than this service reads and carries no \
                         message id near its start, so the message cannot be given up on"
                        .to_string(),
                })
            }
        },
    }
}

/// Find `"id": "<text>"` near the start of an answer that is not read in
/// full. kyu's envelope (checked in kyu `http/handlers/wire.rs`) is a
/// serde_json map without `preserve_order`, so its keys come out sorted:
/// `id` precedes `payload`, `payload_text` and `payload_base64`. That is a
/// serde_json detail, not a promise, so only an `"id"` found before the
/// first `"payload` key is trusted: an `"id"` inside the payload could
/// otherwise be taken for the message's own. Strict: an escape or an odd
/// shape finds nothing.
pub(in crate::adapters) fn scan_id(prefix: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(prefix);
    let head = &text[..text.find("\"payload").unwrap_or(text.len())];
    let rest = &head[head.find("\"id\"")? + 4..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let id = &rest[..rest.find('"')?];
    (!id.is_empty() && id.len() <= 200 && !id.contains('\\')).then(|| id.to_string())
}

/// Warn once per process that the hub sent no `attempt`.
fn warn_missing_attempt() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        crate::obs::log_warn(
            "hub",
            "no_attempt",
            "the hub's answer carries no attempt number, so every offer is counted as a new \
             message: `received` may count a re-offered message twice (check that kyu and this \
             service speak the same envelope)",
        );
    }
}

/// Turn a whole answer into a message.
pub(in crate::adapters) fn parse_envelope_bytes(raw: &[u8]) -> Result<Poll, HubError> {
    // Parsed with serde_json rather than reqwest's json feature: one
    // fewer feature for one line of code.
    let envelope: serde_json::Value =
        serde_json::from_slice(raw).map_err(|e| HubError::Unreachable {
            detail: format!("the hub's answer was not JSON: {e}"),
        })?;
    // An id is what ack and nack are addressed with; without one every
    // settle would hit a malformed URL and the hub would offer the message
    // again forever. A hub error with a clear line, not a silent "".
    let id = match envelope.get("id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            crate::obs::log_warn(
                "hub",
                "bad_envelope",
                "the hub's answer carries no message id, so it cannot be acknowledged",
            );
            return Err(HubError::Unreachable {
                detail: "the hub's answer carried no message id, so the message cannot be \
                         acknowledged (check that kyu and this service speak the same envelope)"
                    .to_string(),
            });
        }
    };
    let attempt = match envelope.get("attempt").and_then(|v| v.as_u64()) {
        Some(n) => n as u32,
        None => {
            warn_missing_attempt();
            1
        }
    };
    match envelope.get("payload") {
        Some(payload) => Ok(Poll::Message(Box::new(HubMessage {
            id,
            payload: payload.clone(),
            attempt,
        }))),
        // kyu hands a non-JSON body over under another key (measured:
        // payload_text for text bodies); either way there is no `payload`
        // for a template to read.
        None => Ok(Poll::NotJson { id }),
    }
}
