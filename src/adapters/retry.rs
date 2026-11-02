//! Delivering with retries inside one claim (AR8).

use std::time::Duration;

use super::error::DeliverError;
use super::{Clock, Sink};
use crate::config::Profile;
use crate::translate::Delivery;

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
        let attempt = one_attempt(profile, delivery, sink).await;
        match attempt {
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
                // 1 s, 2 s, 4 s … from the very function the config's
                // budget check sums (`pause_ms`), so the two cannot drift
                // apart. Computed by index: no list is built per retry.
                clock.sleep(profile.pause_ms(attempts - 1)).await;
            }
        }
    }
}

/// One attempt, bounded by the profile's timeout.
async fn one_attempt(
    profile: &Profile,
    delivery: &Delivery,
    sink: &dyn Sink,
) -> Result<(), DeliverError> {
    // The profile's timeout bounds each attempt (fix-8, 2026-10-04):
    // the sink only carries a 30 s ceiling, and a destination that
    // drops the connection instead of refusing it held one attempt for
    // that long, far past the retry budget the config checked against
    // the lease.
    tokio::time::timeout(
        Duration::from_millis(profile.timeout_ms),
        sink.deliver(delivery),
    )
    .await
    .unwrap_or(Err(DeliverError::Timeout {
        timeout_ms: profile.timeout_ms,
    }))
}
