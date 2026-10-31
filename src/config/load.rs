//! Loading: the file's text in, a validated [`Config`] out. Pure (the
//! caller supplies the text and an environment lookup), so every rule is
//! a unit test with no ambient I/O (AR1).

use std::collections::BTreeSet;

use super::checks::{check_topic, is_http_url};
use super::env::{resolve, EnvLookup};
use super::error::ConfigError;
use super::model::{Config, Kyu, Reporting};
use super::profile::validate_profile;
use super::raw::{RawConfig, RawKyu, RawReporting};

/// Parse and validate a whole configuration file. `file` is only used to
/// label errors.
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

    let kyu = raw.kyu.map(|k| kyu_of(file, k, env)).transpose()?;
    let reporting = raw
        .reporting
        .map(|r| reporting_of(file, r, kyu.as_ref()))
        .transpose()?;

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

/// The `[kyu]` section: a usable address, and the token resolved from the
/// environment when it is a `${VAR}` reference.
fn kyu_of(file: &str, k: RawKyu, env: &dyn EnvLookup) -> Result<Kyu, ConfigError> {
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
    Ok(Kyu {
        base_url: k.base_url.trim_end_matches('/').to_string(),
        token,
    })
}

/// The `[reporting]` section. The self-report topic is validated exactly
/// like a profile's, and for the same reason: a name kyu refuses (or
/// reserves) starts up green and then fails 403 on every publish, so the
/// one feature whose job is to make silence visible would fail silently.
/// Found in the Phase 7 audit.
fn reporting_of(file: &str, r: RawReporting, kyu: Option<&Kyu>) -> Result<Reporting, ConfigError> {
    check_topic(file, "[reporting]", &r.topic)?;
    if kyu.is_none() {
        return Err(ConfigError::MissingKyuSection {
            file: file.to_string(),
            profile: "[reporting]".to_string(),
        });
    }
    Ok(Reporting { topic: r.topic })
}
