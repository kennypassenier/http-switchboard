//! What the endpoints say: `/healthz` as JSON and the `switchboard_*`
//! Prometheus text. Both are read-only views of the [`Registry`].

use super::registry::{now_unix, Registry};
use crate::pump::Health;

/// The outcomes of a config reload, as the metric labels them.
const RELOAD_OUTCOMES: [&str; 3] = ["applied", "needs-restart", "invalid"];

/// The word `/healthz` uses for a profile's remembered health.
fn state_word(health: Option<Health>) -> &'static str {
    match health {
        None | Some(Health::Starting) => "starting",
        Some(Health::Working) => "working",
        Some(Health::Failing) => "failing",
        Some(Health::Denied) => "denied",
        Some(Health::HubDown) => "hub-down",
    }
}

/// Whether a remembered health counts against `/healthz?strict=1`.
fn is_unhealthy(health: Option<Health>) -> bool {
    matches!(
        health,
        Some(Health::Failing) | Some(Health::Denied) | Some(Health::HubDown)
    )
}

fn json_string(value: &str) -> String {
    serde_json::Value::String(value.to_string()).to_string()
}

/// One counter family in the Prometheus text format: HELP, TYPE, then a
/// line per `(labels, value)` series.
fn push_counter(out: &mut String, name: &str, help: &str, series: &[(String, u64)]) {
    out.push_str(&format!("# HELP {name} {help}\n"));
    out.push_str(&format!("# TYPE {name} counter\n"));
    for (labels, value) in series {
        out.push_str(&format!("{name}{{{labels}}} {value}\n"));
    }
}

impl Registry {
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
            if is_unhealthy(stats.health) {
                healthy = false;
            }
            let age = stats
                .last_success_unix
                .map(|t| now.saturating_sub(t).to_string())
                .unwrap_or_else(|| "null".to_string());
            entries.push(format!(
                r#"{{"name":{},"state":"{}","last_success_age_s":{age}}}"#,
                json_string(name),
                state_word(stats.health)
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
        // One series per profile for a given counter of its stats.
        let per_profile = |value: fn(&super::ProfileStats) -> u64| -> Vec<(String, u64)> {
            snapshot
                .iter()
                .map(|(name, s)| (format!("profile={}", json_string(name)), value(s)))
                .collect()
        };
        let mut out = String::new();
        push_counter(
            &mut out,
            "switchboard_messages_received_total",
            "Messages taken in per profile.",
            &per_profile(|s| s.received),
        );
        push_counter(
            &mut out,
            "switchboard_messages_delivered_total",
            "Messages the destination accepted.",
            &per_profile(|s| s.delivered),
        );
        push_counter(
            &mut out,
            "switchboard_messages_failed_total",
            "Messages the destination did not accept.",
            &per_profile(|s| s.failed),
        );
        push_counter(
            &mut out,
            "switchboard_delivery_duration_ms_total",
            "Time spent delivering, per profile.",
            &per_profile(|s| s.duration_ms_total),
        );
        push_counter(
            &mut out,
            "switchboard_config_reloads_total",
            "Config reloads per outcome (applied, needs-restart, invalid).",
            &self.reload_series(),
        );
        out
    }

    /// The reload counter per outcome; every outcome is listed, zero or not.
    fn reload_series(&self) -> Vec<(String, u64)> {
        let reloads = self.reloads.lock().unwrap();
        RELOAD_OUTCOMES
            .iter()
            .map(|outcome| {
                (
                    format!("outcome=\"{outcome}\""),
                    reloads.get(outcome).copied().unwrap_or(0),
                )
            })
            .collect()
    }
}
