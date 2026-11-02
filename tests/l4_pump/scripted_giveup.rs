//! Messages that can never work are given up on at once, and say so
//! without touching the profile's health.

use super::*;
use http_switchboard::adapters::{BoxFuture, DeliverError, Sink};
use http_switchboard::translate::Delivery;

/// A sink that refuses every delivery with one status.
struct StatusSink(u16);

impl Sink for StatusSink {
    fn deliver<'a>(&'a self, _: &'a Delivery) -> BoxFuture<'a, Result<(), DeliverError>> {
        let status = self.0;
        Box::pin(async move {
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

async fn refused_with(status: u16) -> (Step, Vec<String>, Health) {
    let hub = FakeHub::new(vec![Ok(message("m1", serde_json::json!({"x": 1})))]);
    let mut state = PumpState {
        health: Health::Working,
        ..PumpState::default()
    };
    let step = pump_once(
        &profile(),
        &hub,
        &StatusSink(status),
        &FakeClock::default(),
        &mut state,
    )
    .await;
    (step, hub.calls(), state.health)
}

#[tokio::test]
async fn permanent_statuses_are_dead_lettered_at_once_and_leave_health_alone() {
    for status in http_switchboard::adapters::PERMANENT_STATUSES {
        let (step, calls, health) = refused_with(status).await;
        match step {
            Step::DeadLettered {
                summary, reason, ..
            } => {
                assert_eq!(
                    summary,
                    format!("the destination refused this message with status {status}, which retrying will not change")
                );
                assert!(reason.contains("What now:"), "{reason}");
            }
            other => panic!("status {status}: expected a dead letter, got {other:?}"),
        }
        assert!(
            calls.contains(&"nack(m1, dead=true)".to_string()),
            "{calls:?}"
        );
        assert_eq!(health, Health::Working, "status {status} moved the health");
    }
}

#[tokio::test]
async fn other_refusals_keep_being_handed_back() {
    for status in [401, 403, 404, 500, 503] {
        let (step, calls, health) = refused_with(status).await;
        assert!(
            matches!(step, Step::HandedBack { .. }),
            "{status}: {step:?}"
        );
        assert!(
            calls.contains(&"nack(m1, dead=false)".to_string()),
            "{calls:?}"
        );
        assert_eq!(health, Health::Failing);
    }
}

#[tokio::test]
async fn an_unrenderable_message_no_longer_fails_the_profile() {
    let hub = FakeHub::new(vec![Ok(message("m1", serde_json::json!({"other": 1})))]);
    let mut state = PumpState {
        health: Health::Working,
        ..PumpState::default()
    };
    let step = pump_once(
        &profile(),
        &hub,
        &StatusSink(200),
        &FakeClock::default(),
        &mut state,
    )
    .await;
    match step {
        Step::DeadLettered { summary, .. } => assert!(summary.contains("template"), "{summary}"),
        other => panic!("expected a dead letter, got {other:?}"),
    }
    assert_eq!(state.health, Health::Working);
}

#[tokio::test]
async fn first_sight_follows_the_hubs_attempt_number() {
    let again = Poll::Message(Box::new(HubMessage {
        id: "m1".into(),
        payload: serde_json::json!({"x": 1}),
        attempt: 2,
    }));
    let hub = FakeHub::new(vec![
        Ok(message("m1", serde_json::json!({"x": 1}))),
        Ok(again),
    ]);
    let mut state = PumpState::default();
    pump_once(
        &profile(),
        &hub,
        &StatusSink(503),
        &FakeClock::default(),
        &mut state,
    )
    .await;
    assert!(state.first_sight);
    pump_once(
        &profile(),
        &hub,
        &StatusSink(503),
        &FakeClock::default(),
        &mut state,
    )
    .await;
    assert!(!state.first_sight, "attempt 2 is the same message again");
}

#[tokio::test]
async fn a_given_up_message_proves_the_hub_answers_and_ends_a_stale_hub_down() {
    for stale in [Health::Starting, Health::HubDown, Health::Denied] {
        let hub = FakeHub::new(vec![Ok(Poll::NotJson { id: "m1".into() })]);
        let mut state = PumpState {
            health: stale,
            ..PumpState::default()
        };
        pump_once(
            &profile(),
            &hub,
            &StatusSink(200),
            &FakeClock::default(),
            &mut state,
        )
        .await;
        assert_eq!(state.health, Health::Working, "from {stale:?}");

        let hub = FakeHub::new(vec![Ok(message("m1", serde_json::json!({"other": 1})))]);
        let mut state = PumpState {
            health: stale,
            ..PumpState::default()
        };
        pump_once(
            &profile(),
            &hub,
            &StatusSink(200),
            &FakeClock::default(),
            &mut state,
        )
        .await;
        assert_eq!(state.health, Health::Working, "from {stale:?}");
    }
}

#[tokio::test]
async fn an_oversize_message_is_dead_lettered_unread_with_a_safe_reason() {
    let hub = FakeHub::new(vec![Ok(Poll::TooLarge { id: "m1".into() })]);
    let mut state = PumpState {
        health: Health::Working,
        ..PumpState::default()
    };
    let step = pump_once(
        &profile(),
        &hub,
        &StatusSink(200),
        &FakeClock::default(),
        &mut state,
    )
    .await;
    match step {
        Step::DeadLettered { summary, .. } => {
            assert_eq!(
                summary,
                "the message on the hub is larger than this service reads"
            )
        }
        other => panic!("expected a dead letter, got {other:?}"),
    }
    assert!(hub.calls().contains(&"nack(m1, dead=true)".to_string()));
    assert_eq!(state.health, Health::Working);
}
