//! The small field-level rules: names, topics, paths and URLs. Each one
//! refuses with a [`ConfigError`] that says what to do.

use super::defaults::{MAX_NAME_LEN, MIN_URL_LEN_EXCLUSIVE, RESERVED_PATHS, RESERVED_TOPIC_PREFIX};
use super::error::ConfigError;

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
