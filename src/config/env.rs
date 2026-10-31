//! Where the values behind `${VAR}` come from, and how a configured value
//! becomes a [`Secret`].

use super::error::ConfigError;
use crate::secret::Secret;

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

pub(super) fn resolve(
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
