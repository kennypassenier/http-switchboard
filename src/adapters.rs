//! Everything that touches the outside world (L3-L5).
//!
//! Sinks, sources and the clock sit behind traits (AR1). The reason is
//! not tidiness: messages are lost in the ordering of deliver, retry and
//! acknowledge, and that ordering has to be testable with fakes. A pure
//! template core alone would have proven the wrong half.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::translate::Delivery;

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

mod error;
mod http_sink;
mod kyu_hub;
mod retry;
#[cfg(test)]
mod tests;

pub use error::{DeliverError, PERMANENT_STATUSES};
pub use http_sink::HttpSink;
pub use kyu_hub::KyuHub;
pub use retry::{deliver_with_retry, Outcome};

/// Somewhere a translated message can be delivered.
pub trait Sink: Send + Sync {
    fn deliver<'a>(&'a self, delivery: &'a Delivery) -> BoxFuture<'a, Result<(), DeliverError>>;
}
