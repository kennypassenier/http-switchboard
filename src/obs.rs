//! Health, metrics and structured logging (L6 — W5, W6, W7; AR11).
//!
//! One fact, modelled once. A profile's state is the same thing
//! `/healthz` reports, the log line records and — later — the self-report
//! event announces; three separate notions of "is it working" is how they
//! come to disagree.
//!
//! `/healthz` deliberately reports more than "the process is alive". A
//! liveness-only endpoint answers 200 while every profile has been denied
//! for six hours, which is exactly the silent death W5 was raised to
//! Essential against.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pump::Health;

#[derive(Debug, Default, Clone)]
pub struct ProfileStats {
    pub received: u64,
    pub delivered: u64,
    pub failed: u64,
    pub duration_ms_total: u64,
    pub last_success_unix: Option<u64>,
    pub health: Option<Health>,
}

/// Everything the endpoints answer from. Counters only — no payloads, no
/// header values, nothing that could carry a secret into a scrape.
#[derive(Debug, Default)]
pub struct Registry {
    profiles: Mutex<BTreeMap<String, ProfileStats>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make a profile visible before it has done anything, so a service
    /// that is failing from the first second still lists it.
    pub fn register(&self, profile: &str) {
        self.profiles
            .lock()
            .unwrap()
            .entry(profile.to_string())
            .or_default();
    }

    pub fn record(&self, profile: &str, delivered: bool, duration_ms: u64) {
        let mut guard = self.profiles.lock().unwrap();
        let stats = guard.entry(profile.to_string()).or_default();
        stats.received += 1;
        stats.duration_ms_total += duration_ms;
        if delivered {
            stats.delivered += 1;
            stats.last_success_unix = Some(now_unix());
        } else {
            stats.failed += 1;
        }
    }

    pub fn set_health(&self, profile: &str, health: Health) {
        self.profiles
            .lock()
            .unwrap()
            .entry(profile.to_string())
            .or_default()
            .health = Some(health);
    }

    /// The state a profile is in right now, so a caller can tell a
    /// transition from a repeat.
    pub fn health_of(&self, profile: &str) -> Option<Health> {
        self.profiles
            .lock()
            .unwrap()
            .get(profile)
            .and_then(|s| s.health)
    }

    pub fn snapshot(&self) -> BTreeMap<String, ProfileStats> {
        self.profiles.lock().unwrap().clone()
    }

    /// `(healthy, body)`. `healthy` means every profile is doing its job.
    ///
    /// The caller decides what to do with that, and the two callers want
    /// different things (Phase 7 audit, G2): the container's own
    /// healthcheck asks "is this process alive", and must not restart the
    /// service because Home Assistant is down — each restart would reset
    /// the pump state and turn AR12's "exactly one failure event" into one
    /// per restart. Uptime Kuma asks "is it doing its job", and wants a
    /// non-2xx it can alarm on. Hence `/healthz` (liveness, always 200
    /// while the process answers) and `/healthz?strict=1` (503 when any
    /// profile is failing, denied or cut off).
    pub fn healthz(&self) -> (bool, String) {
        let snapshot = self.snapshot();
        let now = now_unix();
        let mut healthy = true;
        let mut entries = Vec::new();

        for (name, stats) in &snapshot {
            let state = match stats.health {
                None | Some(Health::Starting) => "starting",
                Some(Health::Working) => "working",
                Some(Health::Failing) => "failing",
                Some(Health::Denied) => "denied",
                Some(Health::HubDown) => "hub-down",
            };
            if matches!(
                stats.health,
                Some(Health::Failing) | Some(Health::Denied) | Some(Health::HubDown)
            ) {
                healthy = false;
            }
            let age = stats
                .last_success_unix
                .map(|t| now.saturating_sub(t).to_string())
                .unwrap_or_else(|| "null".to_string());
            entries.push(format!(
                r#"{{"name":{},"state":"{state}","last_success_age_s":{age}}}"#,
                json_string(name)
            ));
        }

        let status = if healthy { "ok" } else { "degraded" };
        (
            healthy,
            format!(
                r#"{{"status":"{status}","profiles":[{}]}}"#,
                entries.join(",")
            ),
        )
    }

    /// Prometheus text format. Counters per profile plus the delivery
    /// duration W6 asks for — without it "deliveries got slow" is an
    /// archaeology exercise in the logs instead of a graph.
    pub fn metrics(&self) -> String {
        let snapshot = self.snapshot();
        let mut out = String::new();
        out.push_str("# HELP switchboard_messages_received_total Messages taken in per profile.\n");
        out.push_str("# TYPE switchboard_messages_received_total counter\n");
        for (name, s) in &snapshot {
            out.push_str(&format!(
                "switchboard_messages_received_total{{profile={}}} {}\n",
                json_string(name),
                s.received
            ));
        }
        out.push_str(
            "# HELP switchboard_messages_delivered_total Messages the destination accepted.\n",
        );
        out.push_str("# TYPE switchboard_messages_delivered_total counter\n");
        for (name, s) in &snapshot {
            out.push_str(&format!(
                "switchboard_messages_delivered_total{{profile={}}} {}\n",
                json_string(name),
                s.delivered
            ));
        }
        out.push_str(
            "# HELP switchboard_messages_failed_total Messages the destination did not accept.\n",
        );
        out.push_str("# TYPE switchboard_messages_failed_total counter\n");
        for (name, s) in &snapshot {
            out.push_str(&format!(
                "switchboard_messages_failed_total{{profile={}}} {}\n",
                json_string(name),
                s.failed
            ));
        }
        out.push_str(
            "# HELP switchboard_delivery_duration_ms_total Time spent delivering, per profile.\n",
        );
        out.push_str("# TYPE switchboard_delivery_duration_ms_total counter\n");
        for (name, s) in &snapshot {
            out.push_str(&format!(
                "switchboard_delivery_duration_ms_total{{profile={}}} {}\n",
                json_string(name),
                s.duration_ms_total
            ));
        }
        out
    }
}

/// One line per message — the K11 contract, and what a Loki query filters
/// on. Per-attempt detail belongs at debug level; this is the summary.
pub fn log_message(profile: &str, source: &str, outcome: &str, duration_ms: u64, attempts: u32) {
    println!(
        "{}",
        message_line(profile, source, outcome, duration_ms, attempts)
    );
}

/// The line itself, so a test can assert it is valid JSON with the fixed
/// fields — W7's bar, which cannot be checked on something that only ever
/// goes to stdout.
pub fn message_line(
    profile: &str,
    source: &str,
    outcome: &str,
    duration_ms: u64,
    attempts: u32,
) -> String {
    format!(
        r#"{{"ts":{},"level":"info","profile":{},"source":"{source}","outcome":"{outcome}","duration_ms":{duration_ms},"attempts":{attempts}}}"#,
        now_unix(),
        json_string(profile)
    )
}

/// A state change, logged once rather than on every attempt: a hub that
/// is away for an hour should produce two lines, not thousands.
pub fn log_transition(profile: &str, from: Health, to: Health, detail: &str) {
    println!("{}", transition_line(profile, from, to, detail));
}

/// Something went wrong that is NOT a state change. Kept separate on
/// purpose: the first version of this reported a failed self-report by
/// logging a Working -> Failing transition that never happened, which is
/// a log line lying about the state it exists to describe (found while
/// smoke-testing the container image).
pub fn log_warn(profile: &str, event: &str, detail: &str) {
    println!("{}", warn_line(profile, event, detail));
}

pub fn warn_line(profile: &str, event: &str, detail: &str) -> String {
    format!(
        r#"{{"ts":{},"level":"warn","profile":{},"event":"{event}","detail":{}}}"#,
        now_unix(),
        json_string(profile),
        json_string(detail)
    )
}

pub fn transition_line(profile: &str, from: Health, to: Health, detail: &str) -> String {
    format!(
        r#"{{"ts":{},"level":"warn","profile":{},"event":"state_change","from":"{from:?}","to":"{to:?}","detail":{}}}"#,
        now_unix(),
        json_string(profile),
        json_string(detail)
    )
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Quote a value the way JSON needs it. Profile names are already
/// restricted to a safe charset by the config, but a log line that can be
/// broken by its own data is a log line you cannot query.
fn json_string(value: &str) -> String {
    serde_json::Value::String(value.to_string()).to_string()
}

// ── chassis glue (2.0.0) ─────────────────────────────────────────────────

/// One `/healthz` subsystem per profile: the kit renders
/// `{"ok","detail"}` and answers 503 while any profile is failing, denied
/// or cut off from the hub — the old `?strict=1` semantics, now the only
/// ones (a plain liveness poll uses `--healthcheck`, which counts 503 as
/// alive).
pub struct ProfileSubsystem {
    name: String,
    registry: Arc<Registry>,
}

impl ProfileSubsystem {
    pub fn new(name: &str, registry: Arc<Registry>) -> Self {
        Self {
            name: name.to_string(),
            registry,
        }
    }
}

impl chassis::Subsystem for ProfileSubsystem {
    fn name(&self) -> &str {
        &self.name
    }
    fn check(&self) -> chassis::SubsystemStatus {
        match self.registry.health_of(&self.name) {
            None | Some(Health::Starting) => chassis::SubsystemStatus::ok("starting"),
            Some(Health::Working) => chassis::SubsystemStatus::ok("working"),
            Some(Health::Failing) => chassis::SubsystemStatus::failing("failing"),
            Some(Health::Denied) => chassis::SubsystemStatus::failing("denied"),
            Some(Health::HubDown) => chassis::SubsystemStatus::failing("hub-down"),
        }
    }
}

/// The `switchboard_*` metrics, appended verbatim to the kit's `/metrics`.
pub struct RegistryMetrics(pub Arc<Registry>);

impl chassis::ScrapeSource for RegistryMetrics {
    fn scrape(&self) -> String {
        self.0.metrics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn w5_a_denied_profile_makes_the_service_report_degraded() {
        let registry = Registry::new();
        registry.register("a");
        registry.set_health("a", Health::Working);
        let (healthy, body) = registry.healthz();
        assert!(healthy, "{body}");
        assert!(body.contains(r#""state":"working""#), "{body}");

        registry.set_health("a", Health::Denied);
        let (healthy, body) = registry.healthz();
        assert!(
            !healthy,
            "a denied profile must not be reported as healthy: {body}"
        );
        assert!(body.contains("degraded"), "{body}");
        assert!(body.contains(r#""state":"denied""#), "{body}");
    }

    #[test]
    fn w6_the_counters_and_the_duration_move_as_expected() {
        let registry = Registry::new();
        registry.record("a", true, 12);
        registry.record("a", false, 30);
        let text = registry.metrics();
        assert!(
            text.contains(r#"switchboard_messages_received_total{profile="a"} 2"#),
            "{text}"
        );
        assert!(
            text.contains(r#"switchboard_messages_delivered_total{profile="a"} 1"#),
            "{text}"
        );
        assert!(
            text.contains(r#"switchboard_messages_failed_total{profile="a"} 1"#),
            "{text}"
        );
        assert!(
            text.contains(r#"switchboard_delivery_duration_ms_total{profile="a"} 42"#),
            "the duration W6 asks for is missing: {text}"
        );
    }

    #[test]
    fn w7_every_log_line_is_valid_json_with_the_fixed_fields() {
        // W7's bar. A line that its own data can break is a line you
        // cannot query, so the awkward cases are in here on purpose.
        for (profile, detail) in [
            ("alertmanager", "plain"),
            (r#"quote"and\backslash"#, "line\nbreak and \"quotes\""),
        ] {
            let line = message_line(profile, "kyu", "delivered", 12, 2);
            let parsed: serde_json::Value =
                serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON: {line} ({e})"));
            assert_eq!(parsed["profile"], profile);
            assert_eq!(parsed["outcome"], "delivered");
            assert_eq!(parsed["duration_ms"], 12);
            assert_eq!(parsed["attempts"], 2);
            assert!(parsed["ts"].is_number());

            let line = transition_line(profile, Health::Working, Health::Denied, detail);
            let parsed: serde_json::Value =
                serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON: {line} ({e})"));
            assert_eq!(parsed["from"], "Working");
            assert_eq!(parsed["to"], "Denied");
            assert_eq!(parsed["detail"], detail);
        }
    }

    #[test]
    fn w7_a_warning_is_not_dressed_up_as_a_state_change() {
        // The bug the container smoke test surfaced: a failed self-report
        // was logged as a Working -> Failing transition that never
        // happened.
        let line = warn_line("p", "self_report_failed", "the hub refused");
        let parsed: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
        assert_eq!(parsed["event"], "self_report_failed");
        assert!(
            parsed.get("from").is_none(),
            "no invented transition: {line}"
        );
        assert!(parsed.get("to").is_none(), "no invented transition: {line}");
    }

    #[test]
    fn w7_a_health_body_carries_names_and_never_content() {
        let registry = Registry::new();
        registry.register("alertmanager");
        registry.record("alertmanager", true, 5);
        let (_, body) = registry.healthz();
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["profiles"][0]["name"], "alertmanager");
        assert!(parsed["profiles"][0]["last_success_age_s"].is_number());
    }
}
