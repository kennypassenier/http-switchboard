//! Secret values that cannot be printed by accident (AR6).

use std::fmt;

/// A value that must never reach a log line, an error message or a
/// backtrace. `Debug` and `Display` print a placeholder, so the only way
/// to obtain the real string is to ask for it by name.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The plaintext. Every call site is a place a secret could leak, so
    /// the name is deliberately ugly.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn k8_a_secret_never_prints_itself() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s}"), "***");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert!(!format!("{s:?} {s}").contains("hunter2"));
        assert_eq!(s.expose(), "hunter2");
    }
}
