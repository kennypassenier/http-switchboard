//! Validation of one `[[profiles]]` block, in the order the checks have
//! always run: names, endpoints, kyu wiring, content type, template, the
//! retired token, headers, then the lease budget.

use std::collections::BTreeMap;

use super::checks::{check_name, check_path, check_topic, is_http_url};
use super::defaults::{
    DEFAULT_FORWARD_ERROR_BODY, DEFAULT_LEASE_MS, DEFAULT_MAX_ATTEMPTS, DEFAULT_METHOD,
    DEFAULT_RETRIES, DEFAULT_TIMEOUT_MS, LEASE_MARGIN_MS,
};
use super::env::{resolve, EnvLookup};
use super::error::ConfigError;
use super::model::{Kyu, Profile, Reporting, Sink, Source};
use super::raw::{RawEndpoint, RawProfile};
use crate::secret::Secret;

pub(super) fn validate_profile(
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

    let source = source_of(file, &name, &rp.from)?;
    let sink = sink_of(file, &name, &rp.to, rp.method.as_deref())?;
    check_kyu_wiring(file, &name, &source, &sink, kyu, reporting)?;

    let content_type = rp
        .content_type
        .ok_or_else(|| ConfigError::MissingContentType {
            file: file.to_string(),
            profile: name.clone(),
        })?;
    check_template(file, &name, &content_type, &rp.body)?;

    check_inbound_token_retired(file, &name, rp.inbound_token.as_ref())?;
    let headers = resolve_headers(file, &name, rp.headers, env)?;
    let forward_error_body = forward_flag(file, &name, rp.forward_error_body, &source)?;

    let profile = Profile {
        name: name.clone(),
        subscription,
        source,
        sink,
        content_type,
        body: rp.body,
        headers,
        timeout_ms: rp.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
        retries: rp.retries.unwrap_or(DEFAULT_RETRIES),
        lease_ms: rp.lease_ms.unwrap_or(DEFAULT_LEASE_MS),
        max_attempts: rp.max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS),
        forward_error_body,
    };
    check_lease_budget(file, &profile)?;
    Ok(profile)
}

/// 3.0.0: the door moved to the kit's client tokens (H1). The field is
/// still recognised so a 2.x config gets a migration message with the
/// replacement command, rather than serde's bare "unknown field".
fn check_inbound_token_retired(
    file: &str,
    name: &str,
    inbound_token: Option<&String>,
) -> Result<(), ConfigError> {
    if inbound_token.is_some() {
        return Err(ConfigError::InboundTokenRetired {
            file: file.to_string(),
            profile: name.to_string(),
        });
    }
    Ok(())
}

/// Header values may be `${VAR}` references; resolve them all now so a
/// missing variable stops the service at startup.
fn resolve_headers(
    file: &str,
    name: &str,
    raw: BTreeMap<String, String>,
    env: &dyn EnvLookup,
) -> Result<BTreeMap<String, Secret>, ConfigError> {
    let mut headers = BTreeMap::new();
    for (k, v) in raw {
        headers.insert(k, resolve(file, &format!(", profile '{name}'"), &v, env)?);
    }
    Ok(headers)
}

/// `forward_error_body` answers the sender, so a kyu source (nobody is
/// waiting for an answer) refuses it.
fn forward_flag(
    file: &str,
    name: &str,
    requested: Option<bool>,
    source: &Source,
) -> Result<bool, ConfigError> {
    let forward = requested.unwrap_or(DEFAULT_FORWARD_ERROR_BODY);
    if forward && matches!(source, Source::Kyu { .. }) {
        return Err(ConfigError::ForwardOnKyuSource {
            file: file.to_string(),
            profile: name.to_string(),
        });
    }
    Ok(forward)
}

/// The `from` side: exactly one of an http path or a kyu topic.
fn source_of(file: &str, name: &str, from: &RawEndpoint) -> Result<Source, ConfigError> {
    match (&from.http_path, &from.kyu_topic, &from.url) {
        (Some(path), None, None) => {
            check_path(file, name, path)?;
            Ok(Source::Http { path: path.clone() })
        }
        (None, Some(topic), None) => {
            check_topic(file, name, topic)?;
            Ok(Source::Kyu {
                topic: topic.clone(),
            })
        }
        _ => Err(ConfigError::Shape {
            file: file.to_string(),
            profile: name.to_string(),
            problem: "'from' must name exactly one of http_path or kyu_topic".to_string(),
        }),
    }
}

/// The `to` side: exactly one of a url or a kyu topic. `method` only makes
/// sense for a url (publishing to the hub is always a POST).
fn sink_of(
    file: &str,
    name: &str,
    to: &RawEndpoint,
    method: Option<&str>,
) -> Result<Sink, ConfigError> {
    match (&to.url, &to.kyu_topic, &to.http_path) {
        (Some(url), None, None) => {
            if !is_http_url(url) {
                return Err(ConfigError::BadSinkUrl {
                    file: file.to_string(),
                    profile: name.to_string(),
                    url: url.clone(),
                });
            }
            Ok(Sink::Url {
                url: url.clone(),
                method: method.unwrap_or(DEFAULT_METHOD).to_string(),
            })
        }
        (None, Some(topic), None) => {
            check_topic(file, name, topic)?;
            if method.is_some() {
                return Err(ConfigError::MethodOnKyuSink {
                    file: file.to_string(),
                    profile: name.to_string(),
                });
            }
            Ok(Sink::Kyu {
                topic: topic.clone(),
            })
        }
        _ => Err(ConfigError::Shape {
            file: file.to_string(),
            profile: name.to_string(),
            problem: "'to' must name exactly one of url or kyu_topic".to_string(),
        }),
    }
}

/// A kyu endpoint on either side needs a `[kyu]` section; and (AR12) a
/// profile that consumes the self-report topic would feed itself.
fn check_kyu_wiring(
    file: &str,
    name: &str,
    source: &Source,
    sink: &Sink,
    kyu: Option<&Kyu>,
    reporting: Option<&Reporting>,
) -> Result<(), ConfigError> {
    let touches_kyu = matches!(source, Source::Kyu { .. }) || matches!(sink, Sink::Kyu { .. });
    if touches_kyu && kyu.is_none() {
        return Err(ConfigError::MissingKyuSection {
            file: file.to_string(),
            profile: name.to_string(),
        });
    }
    if let (Source::Kyu { topic }, Some(r)) = (source, reporting) {
        if topic == &r.topic {
            return Err(ConfigError::SelfReportLoop {
                file: file.to_string(),
                profile: name.to_string(),
                topic: topic.clone(),
            });
        }
    }
    Ok(())
}

/// Content type and template rules.
///
/// Phase 7, G8: escaping is a mechanism only for JSON. A profile that
/// delivers anything else would interpolate values raw, so a message
/// could append fields of its own, the exact class AR7 exists to close.
/// Refused rather than started; a non-JSON destination needs a safe
/// escaping rule first, and that is a mini-round. AR7: the shape mistake
/// JSON autoescape invites is caught at startup, not on the first real
/// message.
fn check_template(
    file: &str,
    name: &str,
    content_type: &str,
    body: &str,
) -> Result<(), ConfigError> {
    if !crate::translate::is_json_content_type(content_type) {
        return Err(ConfigError::Template {
            file: file.to_string(),
            profile: name.to_string(),
            problem: format!(
                "content_type is '{content_type}', and only JSON is supported today. What now: use application/json — values are escaped by the engine there, which is what stops a message from adding fields of its own. A non-JSON destination needs an escaping rule of its own first."
            ),
        });
    }
    crate::translate::check_template(body, content_type).map_err(|problem| ConfigError::Template {
        file: file.to_string(),
        profile: name.to_string(),
        problem,
    })
}

/// AR8: the retry budget must fit inside the lease, with room for the ack
/// itself. Only a kyu source holds a lease.
fn check_lease_budget(file: &str, profile: &Profile) -> Result<(), ConfigError> {
    if !matches!(profile.source, Source::Kyu { .. }) {
        return Ok(());
    }
    let needed = profile.retry_budget_ms() + LEASE_MARGIN_MS;
    if needed > profile.lease_ms {
        return Err(ConfigError::LeaseBudget {
            file: file.to_string(),
            profile: profile.name.clone(),
            needed_ms: needed,
            lease_ms: profile.lease_ms,
        });
    }
    Ok(())
}
