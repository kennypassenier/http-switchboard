//! A clock that never sleeps: it records the pauses it was asked for.

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
