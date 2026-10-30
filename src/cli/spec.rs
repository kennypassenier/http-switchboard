//! What the kit and the operator are told about this binary: the
//! [`AppSpec`] and the usage text.

use chassis::AppSpec;

/// The kit's description of this service. One definition, because the
/// dry-run needs the same knob and section names the real start does.
pub(crate) fn spec() -> AppSpec {
    AppSpec {
        name: "http-switchboard",
        version: env!("CARGO_PKG_VERSION"),
        repository: Some("kennypassenier/http-switchboard"),
        ..Default::default()
    }
}

pub(crate) fn usage() -> String {
    "http-switchboard [--config <config.toml>] [--listen host:port] [--state-dir <dir>]\n\
     http-switchboard --check [--config <config.toml>]\n\
     http-switchboard --healthcheck [http://127.0.0.1:8080/healthz]\n\
     http-switchboard test --config <config.toml> --profile <name> --input <message.json>\n\
     Every knob is also an environment variable (HTTP_SWITCHBOARD_CONFIG, _LISTEN, …); --help lists them.\n"
        .to_string()
}
