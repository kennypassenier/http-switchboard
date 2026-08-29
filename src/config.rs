//! Profile configuration: the whole model of the service (L1 — K8, K9,
//! K10; AR2, AR5, AR6).
//!
//! Loading is pure: the caller supplies the file's text and a lookup for
//! environment variables, so every validation rule is a unit test with no
//! ambient I/O (AR1). Nothing here starts half-working — a config that
//! does not hold up stops the process, and every error says what to do
//! about it (K10, standing rule 11).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;

use crate::secret::Secret;

/// kyu reserves this prefix for its own events and answers 403 to a
/// publish (verified in kyu's debugging guide).
const RESERVED_TOPIC_PREFIX: &str = "kyu.";

/// Paths the service answers itself (AR11); a profile may not claim them.
const RESERVED_PATHS: [&str; 2] = ["/healthz", "/metrics"];

/// Safety margin between the retry budget and the kyu lease (AR8): the
/// delivery must finish, and the ack must land, before the lease expires,
/// or kyu has already handed the message to someone else.
const LEASE_MARGIN_MS: u64 = 5_000;

const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_RETRIES: u32 = 2;
/// kyu's own default lease is 30 s, which the shipped defaults do NOT fit
/// into: three attempts of ten seconds plus their pauses plus the margin
/// is 38 s. Found by the L1 test that checks the budget against the
/// defaults — the first version of this file shipped a default config it
/// would itself have refused. The lease is a per-subscription policy the
/// service pushes to the hub (AR8), so the honest fix is to ask for a
/// lease that fits rather than to quietly retry less.
const DEFAULT_LEASE_MS: u64 = 60_000;
const DEFAULT_MAX_ATTEMPTS: u32 = 5;
/// First pause between attempts; each following pause doubles.
const BACKOFF_BASE_MS: u64 = 1_000;

// ── the validated model ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Config {
    pub kyu: Option<Kyu>,
    pub reporting: Option<Reporting>,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone)]
pub struct Kyu {
    pub base_url: String,
    pub token: Option<Secret>,
}

#[derive(Debug, Clone)]
pub struct Reporting {
    pub topic: String,
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    /// The kyu subscription this profile consumes under. Its own key,
    /// never an accident of the profile name: renaming a profile would
    /// otherwise create a fresh subscription and abandon everything still
    /// unacked (AR2).
    pub subscription: String,
    pub source: Source,
    pub sink: Sink,
    pub content_type: String,
    pub body: String,
    pub headers: BTreeMap<String, Secret>,
    pub timeout_ms: u64,
    pub retries: u32,
    pub lease_ms: u64,
    pub max_attempts: u32,
    pub inbound_token: Option<Secret>,
}

impl Profile {
    /// Wall-clock a delivery may consume before the ack must be in, worst
    /// case: every attempt burning its full timeout, plus the pauses
    /// between them. The pauses are part of the budget — leaving them out
    /// is how a design that looks like it fits the lease does not.
    pub fn retry_budget_ms(&self) -> u64 {
        let attempts = self.timeout_ms * u64::from(self.retries + 1);
        let pauses: u64 = (0..self.retries)
            .map(|i| BACKOFF_BASE_MS * 2u64.pow(i))
            .sum();
        attempts + pauses
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Http { path: String },
    Kyu { topic: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sink {
    Url { url: String, method: String },
    Kyu { topic: String },
}

// ── errors ─────────────────────────────────────────────────────────────

/// Every variant names the file, the profile where one applies, what is
/// wrong and what to do — K10's bar, which a fixed string could not meet.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{file}: the file is not valid TOML — {source}. What now: fix the syntax the parser points at; an unknown key means a typo, compare it with the example config in the README.")]
    Parse {
        file: String,
        #[source]
        source: toml::de::Error,
    },

    #[error("{file}: there are no profiles. What now: add at least one [[profiles]] block, or stop the service — a switchboard with no profiles would accept nothing and deliver nothing.")]
    NoProfiles { file: String },

    #[error("{file}: profile '{name}' appears more than once. What now: give each profile a unique name; the name is what its log lines, metrics and kyu subscription are labelled with.")]
    DuplicateProfile { file: String, name: String },

    #[error("{file}, profile '{profile}': {field} '{value}' is not a usable name. What now: use lower-case letters, digits, dot, dash or underscore, at most 64 characters — kyu refuses anything else, and a name it refuses would fail on the first poll while the service reported itself healthy.")]
    BadName {
        file: String,
        profile: String,
        field: String,
        value: String,
    },

    #[error("{file}, profile '{profile}': topic '{topic}' starts with '{prefix}'. What now: pick another name — kyu reserves that prefix for its own events and answers 403 to a publish, which would only surface on the first real message.")]
    ReservedTopic {
        file: String,
        profile: String,
        topic: String,
        prefix: String,
    },

    #[error("{file}, profile '{profile}': path '{path}' must start with a slash. What now: write it as an absolute path, for example \"/alertmanager\".")]
    BadPath {
        file: String,
        profile: String,
        path: String,
    },

    #[error("{file}, profile '{profile}': path '{path}' is reserved by the service itself. What now: pick another path — {path} is answered by the health or metrics endpoint, and a profile claiming it would shadow monitoring.")]
    ReservedPath {
        file: String,
        profile: String,
        path: String,
    },

    #[error("{file}, profile '{profile}': inbound_token is set, but this profile's source is a kyu topic. What now: remove inbound_token, or change the source to an http_path — a token on a kyu source guards nothing, and leaving it there suggests a door that was never built.")]
    InboundTokenOnKyuSource { file: String, profile: String },

    #[error("{file}, profile '{profile}': method is set, but this profile delivers to a kyu topic. What now: remove method — publishing to the hub is always a POST to the topic.")]
    MethodOnKyuSink { file: String, profile: String },

    #[error("{file}, profile '{profile}': content_type is missing. What now: state it explicitly, e.g. content_type = \"application/json\" — a missing content type is forwarded as-is and arrives at Home Assistant as an empty payload, while every layer still reports success.")]
    MissingContentType { file: String, profile: String },

    #[error("{file}, profile '{profile}': uses a kyu topic, but there is no [kyu] section. What now: add [kyu] with base_url (and token if the hub requires one), or change this profile to use http.")]
    MissingKyuSection { file: String, profile: String },

    #[error("{file}, profile '{profile}': the retry budget is {needed_ms} ms but the kyu lease is {lease_ms} ms. What now: lower timeout_ms or retries, or raise lease_ms — a delivery finishing after the lease expires is acked into a 409 while the hub has already handed the message to someone else, which is a duplicate by construction.")]
    LeaseBudget {
        file: String,
        profile: String,
        needed_ms: u64,
        lease_ms: u64,
    },

    #[error("{file}, profile '{profile}': its source is the self-report topic '{topic}'. What now: use another source — a profile that consumes its own failure events feeds itself forever.")]
    SelfReportLoop {
        file: String,
        profile: String,
        topic: String,
    },

    #[error("{file}{location}: environment variable '{var}' is not set. What now: provide it in the environment the service starts in (the homelab vault composes it from latch); the config only ever holds the reference, never the value.")]
    MissingEnv {
        file: String,
        location: String,
        var: String,
    },

    #[error("{file}, profile '{profile}': {problem}. What now: name exactly one endpoint per side — a source is either http_path or kyu_topic, a destination is either url or kyu_topic; anything else leaves it ambiguous where a message comes from or goes to.")]
    Shape {
        file: String,
        profile: String,
        problem: String,
    },

    #[error("{file}: [kyu] base_url '{value}' is not an http(s) address. What now: write the hub's address including the scheme, e.g. \"http://10.10.10.9:8080\".")]
    BadKyuUrl { file: String, value: String },

    #[error("{file}, profile '{profile}': destination '{url}' is not an http(s) address. What now: write the full address including the scheme, e.g. \"http://10.10.10.2:8123/api/webhook/abc\".")]
    BadSinkUrl {
        file: String,
        profile: String,
        url: String,
    },
}

// ── the raw file, exactly as written ───────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    kyu: Option<RawKyu>,
    reporting: Option<RawReporting>,
    #[serde(default)]
    profiles: Vec<RawProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKyu {
    base_url: String,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReporting {
    topic: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    name: String,
    subscription: Option<String>,
    from: RawEndpoint,
    to: RawEndpoint,
    content_type: Option<String>,
    body: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    method: Option<String>,
    timeout_ms: Option<u64>,
    retries: Option<u32>,
    lease_ms: Option<u64>,
    max_attempts: Option<u32>,
    inbound_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEndpoint {
    http_path: Option<String>,
    kyu_topic: Option<String>,
    url: Option<String>,
}

// ── loading ────────────────────────────────────────────────────────────

/// Where the values behind `${VAR}` come from. A trait rather than a
/// direct `std::env` call so every rule below is testable without
/// touching the process environment (AR1).
pub trait EnvLookup {
    fn get(&self, key: &str) -> Option<String>;
}

impl<F> EnvLookup for F
where
    F: Fn(&str) -> Option<String>,
{
    fn get(&self, key: &str) -> Option<String> {
        self(key)
    }
}

/// The process environment. The only place in the crate that reads it.
pub struct ProcessEnv;

impl EnvLookup for ProcessEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

pub fn load(file: &str, text: &str, env: &dyn EnvLookup) -> Result<Config, ConfigError> {
    let raw: RawConfig = toml::from_str(text).map_err(|source| ConfigError::Parse {
        file: file.to_string(),
        source,
    })?;

    if raw.profiles.is_empty() {
        return Err(ConfigError::NoProfiles {
            file: file.to_string(),
        });
    }

    let kyu = match raw.kyu {
        Some(k) => {
            if !is_http_url(&k.base_url) {
                return Err(ConfigError::BadKyuUrl {
                    file: file.to_string(),
                    value: k.base_url,
                });
            }
            let token = match k.token {
                Some(t) => Some(resolve(file, "", &t, env)?),
                None => None,
            };
            Some(Kyu {
                base_url: k.base_url.trim_end_matches('/').to_string(),
                token,
            })
        }
        None => None,
    };

    let reporting = raw.reporting.map(|r| Reporting { topic: r.topic });

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut profiles = Vec::with_capacity(raw.profiles.len());
    for rp in raw.profiles {
        let profile = validate_profile(file, rp, kyu.as_ref(), reporting.as_ref(), env)?;
        if !seen.insert(profile.name.clone()) {
            return Err(ConfigError::DuplicateProfile {
                file: file.to_string(),
                name: profile.name,
            });
        }
        profiles.push(profile);
    }

    Ok(Config {
        kyu,
        reporting,
        profiles,
    })
}

fn validate_profile(
    file: &str,
    rp: RawProfile,
    kyu: Option<&Kyu>,
    reporting: Option<&Reporting>,
    env: &dyn EnvLookup,
) -> Result<Profile, ConfigError> {
    let name = rp.name;
    check_name(file, &name, "name", &name)?;

    let subscription = rp.subscription.unwrap_or_else(|| name.clone());
    check_name(file, &name, "subscription", &subscription)?;

    let source = match (&rp.from.http_path, &rp.from.kyu_topic, &rp.from.url) {
        (Some(path), None, None) => {
            check_path(file, &name, path)?;
            Source::Http { path: path.clone() }
        }
        (None, Some(topic), None) => {
            check_topic(file, &name, topic)?;
            Source::Kyu {
                topic: topic.clone(),
            }
        }
        _ => {
            return Err(ConfigError::Shape {
                file: file.to_string(),
                profile: name,
                problem: "'from' must name exactly one of http_path or kyu_topic".to_string(),
            })
        }
    };

    let sink = match (&rp.to.url, &rp.to.kyu_topic, &rp.to.http_path) {
        (Some(url), None, None) => {
            if !is_http_url(url) {
                return Err(ConfigError::BadSinkUrl {
                    file: file.to_string(),
                    profile: name.clone(),
                    url: url.clone(),
                });
            }
            Sink::Url {
                url: url.clone(),
                method: rp.method.clone().unwrap_or_else(|| "POST".to_string()),
            }
        }
        (None, Some(topic), None) => {
            check_topic(file, &name, topic)?;
            if rp.method.is_some() {
                return Err(ConfigError::MethodOnKyuSink {
                    file: file.to_string(),
                    profile: name.clone(),
                });
            }
            Sink::Kyu {
                topic: topic.clone(),
            }
        }
        _ => {
            return Err(ConfigError::Shape {
                file: file.to_string(),
                profile: name,
                problem: "'to' must name exactly one of url or kyu_topic".to_string(),
            })
        }
    };

    // A kyu endpoint on either side needs somewhere to talk to.
    let touches_kyu = matches!(source, Source::Kyu { .. }) || matches!(sink, Sink::Kyu { .. });
    if touches_kyu && kyu.is_none() {
        return Err(ConfigError::MissingKyuSection {
            file: file.to_string(),
            profile: name.clone(),
        });
    }

    // AR12: a profile that consumes its own failure events feeds itself.
    if let (Source::Kyu { topic }, Some(r)) = (&source, reporting) {
        if topic == &r.topic {
            return Err(ConfigError::SelfReportLoop {
                file: file.to_string(),
                profile: name.clone(),
                topic: topic.clone(),
            });
        }
    }

    let content_type = rp
        .content_type
        .ok_or_else(|| ConfigError::MissingContentType {
            file: file.to_string(),
            profile: name.clone(),
        })?;

    let inbound_token = match (rp.inbound_token, &source) {
        (Some(_), Source::Kyu { .. }) => {
            return Err(ConfigError::InboundTokenOnKyuSource {
                file: file.to_string(),
                profile: name.clone(),
            })
        }
        (Some(t), _) => Some(resolve(file, &format!(", profile '{name}'"), &t, env)?),
        (None, _) => None,
    };

    let mut headers = BTreeMap::new();
    for (k, v) in rp.headers {
        headers.insert(k, resolve(file, &format!(", profile '{name}'"), &v, env)?);
    }

    let timeout_ms = rp.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let retries = rp.retries.unwrap_or(DEFAULT_RETRIES);
    let lease_ms = rp.lease_ms.unwrap_or(DEFAULT_LEASE_MS);
    let max_attempts = rp.max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS);

    let profile = Profile {
        name: name.clone(),
        subscription,
        source,
        sink,
        content_type,
        body: rp.body,
        headers,
        timeout_ms,
        retries,
        lease_ms,
        max_attempts,
        inbound_token,
    };

    // AR8: the retry budget must fit inside the lease, with room for the
    // ack itself. Only a kyu source holds a lease.
    if matches!(profile.source, Source::Kyu { .. }) {
        let needed = profile.retry_budget_ms() + LEASE_MARGIN_MS;
        if needed > profile.lease_ms {
            return Err(ConfigError::LeaseBudget {
                file: file.to_string(),
                profile: name,
                needed_ms: needed,
                lease_ms: profile.lease_ms,
            });
        }
    }

    Ok(profile)
}

fn resolve(
    file: &str,
    location: &str,
    value: &str,
    env: &dyn EnvLookup,
) -> Result<Secret, ConfigError> {
    let Some(var) = value
        .strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
    else {
        return Ok(Secret::new(value));
    };
    match env.get(var) {
        Some(v) => Ok(Secret::new(v)),
        None => Err(ConfigError::MissingEnv {
            file: file.to_string(),
            location: location.to_string(),
            var: var.to_string(),
        }),
    }
}

fn check_name(file: &str, profile: &str, field: &str, value: &str) -> Result<(), ConfigError> {
    let ok = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(ConfigError::BadName {
            file: file.to_string(),
            profile: profile.to_string(),
            field: field.to_string(),
            value: value.to_string(),
        })
    }
}

fn check_topic(file: &str, profile: &str, topic: &str) -> Result<(), ConfigError> {
    check_name(file, profile, "topic", topic)?;
    if topic.starts_with(RESERVED_TOPIC_PREFIX) {
        return Err(ConfigError::ReservedTopic {
            file: file.to_string(),
            profile: profile.to_string(),
            topic: topic.to_string(),
            prefix: RESERVED_TOPIC_PREFIX.to_string(),
        });
    }
    Ok(())
}

fn check_path(file: &str, profile: &str, path: &str) -> Result<(), ConfigError> {
    if !path.starts_with('/') {
        return Err(ConfigError::BadPath {
            file: file.to_string(),
            profile: profile.to_string(),
            path: path.to_string(),
        });
    }
    if RESERVED_PATHS.contains(&path) {
        return Err(ConfigError::ReservedPath {
            file: file.to_string(),
            profile: profile.to_string(),
            path: path.to_string(),
        });
    }
    Ok(())
}

fn is_http_url(value: &str) -> bool {
    (value.starts_with("http://") || value.starts_with("https://")) && value.len() > 8
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} profile(s)", self.profiles.len())
    }
}
