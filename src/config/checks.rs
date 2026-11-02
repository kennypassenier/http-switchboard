//! The small rules a profile is held to: names, topics, paths, URLs, the
//! template's content type and the retry budget against the lease. Each one
//! refuses with a [`ConfigError`] that says what to do.

use super::defaults::{
    LEASE_MARGIN_MS, MAX_NAME_LEN, MAX_RETRIES, MIN_URL_LEN_EXCLUSIVE, RESERVED_PATHS,
    RESERVED_TOPIC_PREFIX,
};
use super::error::ConfigError;
use super::model::{Profile, Source};

/// A name kyu accepts: 1..=MAX_NAME_LEN of lower-case letters, digits, dot,
/// dash, underscore. Used for profile names, subscriptions and topics.
pub(super) fn check_name(
    file: &str,
    profile: &str,
    field: &str,
    value: &str,
) -> Result<(), ConfigError> {
    let ok = !value.is_empty()
        && value.len() <= MAX_NAME_LEN
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

/// A kyu topic: a valid name that does not use the prefix kyu reserves.
pub(super) fn check_topic(file: &str, profile: &str, topic: &str) -> Result<(), ConfigError> {
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

/// An inbound path: absolute, and not one the service answers itself.
pub(super) fn check_path(file: &str, profile: &str, path: &str) -> Result<(), ConfigError> {
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

/// An http(s) address with something after the scheme. The length floor
/// is the historic rule (longer than `https://`), kept as it was.
pub(super) fn is_http_url(value: &str) -> bool {
    (value.starts_with("http://") || value.starts_with("https://"))
        && value.len() > MIN_URL_LEN_EXCLUSIVE
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
pub(super) fn check_template(
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
pub(super) fn check_lease_budget(file: &str, profile: &Profile) -> Result<(), ConfigError> {
    if !matches!(profile.source, Source::Kyu { .. }) {
        return Ok(());
    }
    let needed = profile.retry_budget_ms().saturating_add(LEASE_MARGIN_MS);
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

/// Numbers that must be positive or bounded before anything computes with
/// them: a zero timeout fails every attempt at once, a zero lease or
/// attempt cap makes the hub drop or dead-letter at the first claim, and an
/// unbounded retry count drives the budget arithmetic and the pause list.
pub(super) fn check_limits(file: &str, profile: &Profile) -> Result<(), ConfigError> {
    let out_of_range = |field: &'static str,
                        value: u64,
                        rule: &'static str,
                        remedy: &'static str| ConfigError::OutOfRange {
        file: file.to_string(),
        profile: profile.name.clone(),
        field,
        value,
        rule,
        remedy,
    };
    if profile.timeout_ms == 0 {
        return Err(out_of_range("timeout_ms", 0, "it must be at least 1", "set a timeout in milliseconds, e.g. 10000 — with 0 every attempt times out before it starts"));
    }
    if profile.retries > MAX_RETRIES {
        return Err(out_of_range("retries", u64::from(profile.retries), "at most 10 retries are allowed", "lower it — every retry doubles its pause, so past this the pauses alone outlast any lease"));
    }
    if profile.lease_ms == 0 {
        return Err(out_of_range("lease_ms", 0, "it must be at least 1", "set the lease in milliseconds, e.g. 60000 — a zero lease returns every message to the hub the moment it is claimed"));
    }
    if profile.max_attempts == 0 {
        return Err(out_of_range("max_attempts", 0, "it must be at least 1", "set how many times the hub may offer a message, e.g. 5 — zero would dead-letter every message unseen"));
    }
    Ok(())
}

/// The method and header names/values a url sink will send, parsed now so
/// a typo stops the service at startup instead of failing (and being
/// retried) on every message. Values are never echoed: they may be secrets.
pub(super) fn check_http_parts(file: &str, profile: &Profile) -> Result<(), ConfigError> {
    let bad =
        |what: String, problem: &'static str, remedy: &'static str| ConfigError::BadHttpPart {
            file: file.to_string(),
            profile: profile.name.clone(),
            what,
            problem,
            remedy,
        };
    if let super::model::Sink::Url { method, .. } = &profile.sink {
        if reqwest::Method::from_bytes(method.as_bytes()).is_err() {
            return Err(bad(
                format!("method '{method}'"),
                "not an HTTP method token",
                "use a plain method name such as POST, PUT or PATCH",
            ));
        }
    }
    for (name, value) in &profile.headers {
        if reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
            return Err(bad(
                format!("header name '{name}'"),
                "not a valid header name",
                "use letters, digits and dashes only",
            ));
        }
        if reqwest::header::HeaderValue::from_str(value.expose()).is_err() {
            return Err(bad(
                format!("the value of header '{name}'"),
                "it holds a character a header cannot carry, such as a line break",
                "remove it from the value (or from the variable it references)",
            ));
        }
    }
    Ok(())
}
