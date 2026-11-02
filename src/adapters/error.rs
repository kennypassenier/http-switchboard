//! Why a delivery failed, in words an operator can act on, and whether
//! trying again could help.

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

    #[error("the request could not be built: {detail}. What now: fix the method or header named in this profile — it was accepted at startup only if the config check missed it; retrying cannot help.")]
    Request { detail: String },
}

impl DeliverError {
    /// Whether trying again could plausibly help. A 400 will be a 400
    /// again; a 503 or a timeout may not be.
    pub fn is_retryable(&self) -> bool {
        match self {
            DeliverError::Timeout { .. } | DeliverError::Transport { .. } => true,
            // Our own request is malformed; the same bytes fail again.
            DeliverError::Request { .. } => false,
            DeliverError::Status { status, .. } => *status >= 500 || *status == 429,
        }
    }
}

/// What to tell the operator for a refusal with `status`. Each class an
/// operator really meets (credentials, unknown address, too large, other
/// 4xx, 5xx) gets its own sentence.
pub(super) fn advice_for(status: u16) -> String {
    match status {
        401 | 403 => "the receiver rejected our credentials — check the header this profile sends, and whether the token behind it was rotated without restarting this service.".to_string(),
        404 => "the receiver does not know this address — check the URL, or, for a Home Assistant webhook, that the automation with that webhook id still exists.".to_string(),
        413 => "the receiver considers the message too large — shorten the template, or raise the receiver's limit.".to_string(),
        400..=499 => "the receiver considers the message wrong, and it will keep doing so — look at the rendered body with `http-switchboard test` before retrying.".to_string(),
        _ => "the receiver is having trouble; this is retried automatically, and if it persists the message stays on the hub rather than disappearing.".to_string(),
    }
}
