//! Every way a configuration can be refused, each with the file, the
//! profile where one applies, what is wrong and what to do about it.

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

    #[error("{file}, profile '{profile}': forward_error_body is set, but this profile's source is a kyu topic. What now: remove it — a message from the hub has no sender waiting for an answer, so there is nobody to forward anything to; the failure already reaches you through the log and the self-report events.")]
    ForwardOnKyuSource { file: String, profile: String },

    #[error("{file}, profile '{profile}': inbound_token is set, but this service stopped guarding its own door in 3.0.0. What now: remove the line and issue the sender a client token instead — `chassis clients issue <sender> --url http://<host>:8080 --token-env HTTP_SWITCHBOARD_TOKEN` prints it once. The token then goes in the same `authorization: Bearer` header the sender already sends, and revoking it takes effect the same second.")]
    InboundTokenRetired { file: String, profile: String },

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

    #[error("{file}, profile '{profile}': {problem}")]
    Template {
        file: String,
        profile: String,
        problem: String,
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
