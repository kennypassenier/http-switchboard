//! Every number and constant the configuration rules are built on, named
//! once with the reason it has the value it has.

/// kyu reserves this prefix for its own events and answers 403 to a
/// publish (verified in kyu's debugging guide).
pub(super) const RESERVED_TOPIC_PREFIX: &str = "kyu.";

/// Paths the service answers itself (AR11); a profile may not claim them.
pub(super) const RESERVED_PATHS: [&str; 2] = ["/healthz", "/metrics"];

/// Safety margin between the retry budget and the kyu lease (AR8): the
/// delivery must finish, and the ack must land, before the lease expires,
/// or kyu has already handed the message to someone else.
pub(super) const LEASE_MARGIN_MS: u64 = 5_000;

pub(super) const DEFAULT_TIMEOUT_MS: u64 = 10_000;
pub(super) const DEFAULT_RETRIES: u32 = 2;
/// kyu's own default lease is 30 s, which the shipped defaults do NOT fit
/// into: three attempts of ten seconds plus their pauses plus the margin
/// is 38 s. Found by the L1 test that checks the budget against the
/// defaults — the first version of this file shipped a default config it
/// would itself have refused. The lease is a per-subscription policy the
/// service pushes to the hub (AR8), so the honest fix is to ask for a
/// lease that fits rather than to quietly retry less.
pub(super) const DEFAULT_LEASE_MS: u64 = 60_000;
pub(super) const DEFAULT_MAX_ATTEMPTS: u32 = 5;
/// First pause between attempts; each following pause doubles.
pub(super) const BACKOFF_BASE_MS: u64 = 1_000;

/// The longest a profile, subscription or topic name may be; kyu refuses
/// anything longer.
pub(super) const MAX_NAME_LEN: usize = 64;

/// The HTTP method a url destination uses when the profile names none.
pub(super) const DEFAULT_METHOD: &str = "POST";

/// `forward_error_body` is off unless a profile asks for it (W12).
pub(super) const DEFAULT_FORWARD_ERROR_BODY: bool = false;

/// A URL must be longer than this many bytes (the length of `https://`).
pub(super) const MIN_URL_LEN_EXCLUSIVE: usize = 8;
