//! Tests of the recorder's hour model.

use super::{History, HOURS, HOUR_SECS, PROFILE_CAP, RECENT_CAP};
use crate::pump::Health;

const H: u64 = HOUR_SECS;

fn names() -> Vec<String> {
    vec!["a".to_string()]
}

#[test]
fn hours_before_the_start_are_none_and_idle_hours_are_up() {
    let h = History::started_at(100 * H + 10);
    let view = h.view_at(103 * H + 5, &names());
    let hours = &view.profiles[0].hours;
    assert_eq!(hours.len(), HOURS);
    assert_eq!(hours[HOURS - 1].state, "up");
    assert_eq!(hours[HOURS - 4].state, "up");
    assert_eq!(hours[HOURS - 5].state, "none");
}

#[test]
fn counts_fall_in_their_hour_and_failures_degrade_it() {
    let h = History::started_at(0);
    h.record_at("a", 50 * H + 1, true, 100);
    h.record_at("a", 50 * H + 2, true, 300);
    h.record_at("a", 50 * H + 3, false, 9);
    h.record_at("a", 51 * H + 3, false, 9);
    let view = h.view_at(51 * H + 10, &names());
    let hours = &view.profiles[0].hours;
    let before = &hours[HOURS - 2];
    assert_eq!(
        (before.received, before.delivered, before.failed),
        (3, 2, 1)
    );
    assert_eq!(before.duration_ms, 400);
    assert_eq!(before.state, "degraded");
    assert_eq!(hours[HOURS - 1].state, "down");
    assert_eq!(hours[HOURS - 1].cause, Some("all-failed"));
    assert_eq!(view.profiles[0].last_failure_unix, Some(51 * H + 3));
}

#[test]
fn a_denied_profile_stays_red_through_quiet_hours_until_it_recovers() {
    let h = History::started_at(0);
    h.record_at("a", 10 * H, true, 5);
    h.note_health_at("a", 10 * H + 60, Health::Denied);
    h.note_health_at("a", 13 * H + 60, Health::Working);
    let view = h.view_at(14 * H, &names());
    let hours = &view.profiles[0].hours;
    let at = |hour: u64| &hours[hours.len() - 1 - (14 - hour) as usize];
    assert_eq!(at(10).state, "down");
    assert_eq!(at(11).state, "down");
    assert_eq!(at(12).state, "down");
    assert_eq!(at(13).state, "down");
    assert_eq!(at(13).cause, Some("denied"));
    assert_eq!(at(14).state, "up");
}

#[test]
fn a_slot_is_reused_after_24_hours_and_memory_stays_fixed() {
    let h = History::started_at(0);
    h.record_at("a", 5 * H, true, 1);
    h.record_at("a", (5 + HOURS as u64) * H, true, 1);
    for i in 0..(RECENT_CAP as u64 * 3) {
        h.record_at("a", 30 * H + i, true, 1);
    }
    let view = h.view_at((5 + HOURS as u64) * H, &names());
    assert_eq!(view.profiles[0].hours[HOURS - 1].received, 1);
    let later = h.view_at(30 * H + 500, &names());
    assert_eq!(later.profiles[0].recent.len(), RECENT_CAP);
    for n in 0..(PROFILE_CAP + 10) {
        h.record_at(&format!("p{n}"), 30 * H, true, 1);
    }
    assert!(h.tracks.lock().unwrap().len() <= PROFILE_CAP);
}
