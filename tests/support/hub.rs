//! A scripted kyu hub.

use std::sync::{Arc, Mutex};

use http_switchboard::adapters::BoxFuture;
use http_switchboard::pump::{Hub, HubError, Poll};

/// A scripted hub. Answers each poll from `polls` in order (repeating the
/// last), and records every settle call so the ORDER of deliver-then-ack
/// can be asserted — which is the thing that actually loses messages.
pub struct FakeHub {
    polls: Mutex<std::collections::VecDeque<Result<Poll, HubErrorKind>>>,
    pub calls: Arc<Mutex<Vec<String>>>,
    pub last_from_beginning: Arc<Mutex<Option<bool>>>,
}

/// `HubError` is not `Clone`, so the script names the kind and the fake
/// builds the error.
#[derive(Debug, Clone, Copy)]
pub enum HubErrorKind {
    Denied,
    Unreachable,
}

impl FakeHub {
    pub fn new(polls: Vec<Result<Poll, HubErrorKind>>) -> Self {
        Self::with_calls(polls, Arc::new(Mutex::new(Vec::new())))
    }

    /// Share the call log with a sink, so the ORDER of deliver and ack can
    /// be asserted across both — which is the whole point of L4.
    pub fn with_calls(
        polls: Vec<Result<Poll, HubErrorKind>>,
        calls: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            polls: Mutex::new(polls.into()),
            calls,
            last_from_beginning: Arc::new(Mutex::new(None)),
        }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Hub for FakeHub {
    fn next<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        from_beginning: bool,
    ) -> BoxFuture<'a, Result<Poll, HubError>> {
        *self.last_from_beginning.lock().unwrap() = Some(from_beginning);
        self.calls
            .lock()
            .unwrap()
            .push(format!("next(from_beginning={from_beginning})"));
        let next = self
            .polls
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(Poll::Empty));
        Box::pin(async move {
            match next {
                Ok(p) => Ok(p),
                Err(HubErrorKind::Denied) => Err(HubError::Denied),
                Err(HubErrorKind::Unreachable) => Err(HubError::Unreachable {
                    detail: "connection refused".into(),
                }),
            }
        })
    }

    fn ack<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), HubError>> {
        self.calls.lock().unwrap().push(format!("ack({id})"));
        Box::pin(async { Ok(()) })
    }

    fn nack<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        id: &'a str,
        dead: bool,
    ) -> BoxFuture<'a, Result<(), HubError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("nack({id}, dead={dead})"));
        Box::pin(async { Ok(()) })
    }

    fn set_policy<'a>(
        &'a self,
        _topic: &'a str,
        _subscription: &'a str,
        lease_ms: u64,
        max_attempts: u32,
    ) -> BoxFuture<'a, Result<(), HubError>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("policy(lease={lease_ms}, attempts={max_attempts})"));
        Box::pin(async { Ok(()) })
    }
}
