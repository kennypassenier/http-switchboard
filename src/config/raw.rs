//! The raw file, exactly as written: the serde shapes `toml` fills in
//! before any rule runs. `deny_unknown_fields` turns a typo in a key into
//! a parse error instead of a silently ignored setting.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConfig {
    pub(super) kyu: Option<RawKyu>,
    pub(super) reporting: Option<RawReporting>,
    #[serde(default)]
    pub(super) profiles: Vec<RawProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawKyu {
    pub(super) base_url: String,
    pub(super) token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawReporting {
    pub(super) topic: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawProfile {
    pub(super) name: String,
    pub(super) subscription: Option<String>,
    pub(super) from: RawEndpoint,
    pub(super) to: RawEndpoint,
    pub(super) content_type: Option<String>,
    pub(super) body: String,
    #[serde(default)]
    pub(super) headers: BTreeMap<String, String>,
    pub(super) method: Option<String>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) retries: Option<u32>,
    pub(super) lease_ms: Option<u64>,
    pub(super) max_attempts: Option<u32>,
    pub(super) inbound_token: Option<String>,
    pub(super) forward_error_body: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawEndpoint {
    pub(super) http_path: Option<String>,
    pub(super) kyu_topic: Option<String>,
    pub(super) url: Option<String>,
}
