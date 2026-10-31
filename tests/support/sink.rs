//! A sink that records what it was given and answers from a script.

use std::sync::{Arc, Mutex};

use http_switchboard::adapters::BoxFuture;

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
