//! Three permanent refusals in a row make the profile failing and pause
//! the pump; anything that is not a destination refusal neither extends
//! nor breaks the run.

use super::*;
use http_switchboard::adapters::{BoxFuture, DeliverError, Sink};
use http_switchboard::obs::Registry;
use http_switchboard::pump::streak::REFUSAL_STREAK_LIMIT;
use http_switchboard::translate::Delivery;

/// A sink that answers with one status per delivery, in order (200 =
/// accepted); the last status repeats.
struct SequenceSink(Mutex<Vec<u16>>);

impl SequenceSink {
    fn new(statuses: &[u16]) -> Self {
        Self(Mutex::new(statuses.iter().rev().copied().collect()))
    }
}

impl Sink for SequenceSink {
    fn deliver<'a>(&'a self, _: &'a Delivery) -> BoxFuture<'a, Result<(), DeliverError>> {
        let mut left = self.0.lock().unwrap();
        let status = if left.len() > 1 {
            left.pop().unwrap()
        } else {
            *left.last().unwrap_or(&200)
        };
        Box::pin(async move {
            if status == 200 {
                return Ok(());
            }
            Err(DeliverError::Status {
                status,
                advice: "test".into(),
                answer: String::new(),
            })
        })
    }
}

fn profile() -> Profile {
    kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/secret-path" }"#,
        r#"{"x": {{ x }}}"#,
    )
}

fn messages(count: usize) -> Vec<Result<Poll, HubErrorKind>> {
    (1..=count)
        .map(|n| Ok(message(&format!("m{n}"), serde_json::json!({"x": n}))))
        .collect()
}

async fn turn(hub: &FakeHub, sink: &SequenceSink, state: &mut PumpState) -> Step {
    pump_once(&profile(), hub, sink, &FakeClock::default(), state).await
}

#[tokio::test]
async fn three_refusals_in_a_row_fail_the_profile_and_pause_the_pump() {
    let hub = FakeHub::new(messages(4));
    let sink = SequenceSink::new(&[422]);
    let mut state = PumpState {
        health: Health::Working,
        ..PumpState::default()
    };
    for n in 1..REFUSAL_STREAK_LIMIT {
        assert!(matches!(
            turn(&hub, &sink, &mut state).await,
            Step::DeadLettered { .. }
        ));
        assert!(!state.paused, "paused after only {n} refusals");
        assert_eq!(state.health, Health::Working);
    }
    // The third is still dead-lettered, and then the pump stops.
    assert!(matches!(
        turn(&hub, &sink, &mut state).await,
        Step::DeadLettered { .. }
    ));
    assert!(state.paused);
    assert_eq!(state.health, Health::Failing);
    let calls = hub.calls();
    assert!(
        calls.contains(&"nack(m3, dead=true)".to_string()),
        "{calls:?}"
    );

    assert_eq!(turn(&hub, &sink, &mut state).await, Step::Paused);
    assert_eq!(hub.calls(), calls, "a paused pump must not touch the hub");
}

#[tokio::test]
async fn a_success_in_between_starts_the_count_again() {
    let hub = FakeHub::new(messages(5));
    let sink = SequenceSink::new(&[422, 422, 200, 422, 422]);
    let mut state = PumpState::default();
    for _ in 0..5 {
        turn(&hub, &sink, &mut state).await;
    }
    assert!(!state.paused);
    assert_eq!(state.health, Health::Working);
}

#[tokio::test]
async fn messages_that_are_the_problem_do_not_count_as_refusals() {
    let mut polls = messages(1);
    polls.push(Ok(Poll::NotJson { id: "bad".into() }));
    polls.extend(messages(3));
    let hub = FakeHub::new(polls);
    let sink = SequenceSink::new(&[422]);
    let mut state = PumpState::default();
    for _ in 0..3 {
        turn(&hub, &sink, &mut state).await; // refusal, not-JSON, refusal
    }
    assert!(!state.paused, "a not-JSON message must not count");
    turn(&hub, &sink, &mut state).await;
    turn(&hub, &sink, &mut state).await;
    assert!(
        state.paused,
        "the third refusal pauses, the not-JSON one between is neutral"
    );
}

#[tokio::test]
async fn a_recheck_resumes_and_the_waiting_messages_flow() {
    let hub = FakeHub::new(messages(4));
    let sink = SequenceSink::new(&[422, 422, 422, 200]);
    let mut state = PumpState::default();
    for _ in 0..3 {
        turn(&hub, &sink, &mut state).await;
    }
    assert_eq!(turn(&hub, &sink, &mut state).await, Step::Paused);

    assert!(state.resume(), "resume reports that it had been paused");
    let step = turn(&hub, &sink, &mut state).await;
    assert!(matches!(step, Step::Delivered { .. }), "{step:?}");
    assert!(hub.calls().contains(&"ack(m4)".to_string()));
    assert_eq!(state.health, Health::Working);
}

#[test]
fn the_registry_publishes_the_pause_and_a_recheck_clears_it() {
    let registry = Registry::new();
    registry.register("p");
    registry.set_paused("p", true);
    assert!(registry.snapshot()["p"].paused);
    assert!(registry.reset_health_of("p"));
    assert!(!registry.snapshot()["p"].paused);
    registry.set_paused("p", true);
    registry.reset_health();
    assert!(!registry.snapshot()["p"].paused);
}
