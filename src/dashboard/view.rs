//! What a profile looks like to the browser: [`ProfileView`], built from a
//! profile's configuration and its live counters. Names, counters and
//! shapes only; nothing in here may carry a secret.

use serde::Serialize;

use crate::config::{Profile, Sink, Source};
use crate::obs::ProfileStats;
use crate::pump::Health;

/// One profile, as the cards, map, trace and settings draw it.
#[derive(Debug, Serialize)]
pub struct ProfileView {
    pub name: String,
    /// `starting`, `working`, `failing`, `denied` or `hub-down`.
    pub state: &'static str,
    pub received: u64,
    pub delivered: u64,
    pub failed: u64,
    /// Mean delivery time; `None` before the first message.
    pub avg_ms: Option<u64>,
    pub last_success_unix: Option<u64>,
    pub source: EndView,
    pub sink: EndView,
    pub content_type: String,
    /// A short label for the template (its content type and field names,
    /// never the body): the hover card on a profile name shows it.
    pub template: String,
    pub timeout_ms: u64,
    pub retries: u32,
    /// The pauses between tries, in order.
    pub pauses_ms: Vec<u64>,
    /// Worst-case time one delivery may take.
    pub budget_ms: u64,
    /// Only a kyu source holds a lease and a hand-out limit.
    pub lease_ms: Option<u64>,
    pub max_attempts: Option<u32>,
}

/// One end of a profile.
#[derive(Debug, Serialize)]
pub struct EndView {
    /// `kyu` or `http` for a source; `kyu` or `url` for a destination.
    pub kind: &'static str,
    /// The words on a card or a node: `kyu topic alerts.raw`,
    /// `POST /uptime-kuma`, `homeassistant.lan:8123`.
    pub label: String,
    /// A second line: `subscription switchboard`, `HTTP, no hub`.
    pub sub: String,
    /// A source's path (HTTP) or topic (kyu); a destination's topic.
    /// Never a destination's path.
    pub path: Option<String>,
}

/// The host and port of a URL, without scheme, user info, path or query.
/// The path of a Home Assistant webhook is the credential (S2).
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .to_string()
}

/// The word the page shows for a profile's remembered health.
fn state_word(health: Option<Health>) -> &'static str {
    match health {
        None | Some(Health::Starting) => "starting",
        Some(Health::Working) => "working",
        Some(Health::Failing) => "failing",
        Some(Health::Denied) => "denied",
        Some(Health::HubDown) => "hub-down",
    }
}

/// The source end: an HTTP path or a kyu topic.
fn source_view(profile: &Profile) -> EndView {
    match &profile.source {
        Source::Http { path } => EndView {
            kind: "http",
            label: format!("POST {path}"),
            sub: "HTTP, no hub".to_string(),
            path: Some(path.clone()),
        },
        Source::Kyu { topic } => EndView {
            kind: "kyu",
            label: format!("kyu topic {topic}"),
            sub: format!("subscription {}", profile.subscription),
            path: Some(topic.clone()),
        },
    }
}

/// The destination end. A url is host and port only: its path may be a
/// webhook credential (S2).
fn sink_view(profile: &Profile) -> EndView {
    match &profile.sink {
        Sink::Url { url, .. } => EndView {
            kind: "url",
            label: host_of(url),
            sub: "path hidden".to_string(),
            path: None,
        },
        Sink::Kyu { topic } => EndView {
            kind: "kyu",
            label: format!("kyu topic {topic}"),
            sub: "published to the hub".to_string(),
            path: Some(topic.clone()),
        },
    }
}

/// A profile's configuration and its counters (if any yet) as one view.
pub(super) fn view_of(profile: &Profile, stats: Option<&ProfileStats>) -> ProfileView {
    let kyu_source = matches!(profile.source, Source::Kyu { .. });
    let received = stats.map_or(0, |s| s.received);
    ProfileView {
        name: profile.name.clone(),
        state: state_word(stats.and_then(|s| s.health)),
        received,
        delivered: stats.map_or(0, |s| s.delivered),
        failed: stats.map_or(0, |s| s.failed),
        avg_ms: stats
            .filter(|_| received > 0)
            .map(|s| s.duration_ms_total / received),
        last_success_unix: stats.and_then(|s| s.last_success_unix),
        source: source_view(profile),
        sink: sink_view(profile),
        content_type: profile.content_type.clone(),
        template: crate::dashboard_profiles::template_label(&profile.body, &profile.content_type),
        timeout_ms: profile.timeout_ms,
        retries: profile.retries,
        pauses_ms: profile.pauses_ms(),
        budget_ms: profile.retry_budget_ms(),
        lease_ms: kyu_source.then_some(profile.lease_ms),
        max_attempts: kyu_source.then_some(profile.max_attempts),
    }
}
