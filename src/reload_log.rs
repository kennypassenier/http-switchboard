//! [feat-dash-kp11-40] The last reloads, for the dashboard's history.
//!
//! Before this, a reload that was refused existed only as a journal line
//! and a metric. The dashboard's "Reloads" card needs the story: when,
//! what happened, which profiles changed or why it did not apply.
//!
//! The history is **in memory** and capped at [`CAPACITY`] entries; it
//! starts empty after every restart, and the page says so. It never holds
//! a secret: a reason is cut from the error text of the config loader,
//! which can quote the file (a TOML parse error prints the offending line,
//! a type error prints the offending value, and a line of a profile can be
//! a header value), so [`redact`] keeps only the sentence and drops every
//! quoted value and every echoed line of the file.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::reload::Outcome;

/// How many reloads are kept; the oldest falls out first.
pub const CAPACITY: usize = 50;
/// The longest reason that is kept, in characters.
const REASON_MAX: usize = 400;

/// One reload, as the browser sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReloadEntry {
    /// Counts up from 1 within one run of the service.
    pub seq: u64,
    pub at_unix: u64,
    /// `applied`, `needs-restart` or `invalid`.
    pub outcome: &'static str,
    /// The profiles whose contents changed (applied only).
    pub changed: Vec<String>,
    /// Why it did not apply, without secrets (not applied only).
    pub reason: Option<String>,
}

/// What the route answers.
#[derive(Debug, Serialize)]
pub struct ReloadView {
    /// Identifies this run of the service: `seq` restarts at 1 with it.
    pub boot: String,
    /// Reloads that fell out of the capped history.
    pub dropped: u64,
    /// Newest first.
    pub reloads: Vec<ReloadEntry>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: VecDeque<ReloadEntry>,
    next_seq: u64,
    dropped: u64,
}

#[derive(Debug)]
pub struct ReloadLog {
    boot: String,
    inner: Mutex<Inner>,
}

impl Default for ReloadLog {
    fn default() -> Self {
        Self::new()
    }
}

impl ReloadLog {
    pub fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        Self {
            boot: format!("{nanos:x}"),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Remember one reload.
    pub fn push(&self, outcome: &Outcome) {
        self.push_at(outcome, now_unix());
    }

    fn push_at(&self, outcome: &Outcome, at_unix: u64) {
        let (label, changed, reason) = match outcome {
            Outcome::Applied { changed } => ("applied", changed.clone(), None),
            Outcome::NeedsRestart { reason } => ("needs-restart", Vec::new(), Some(redact(reason))),
            Outcome::Invalid { reason } => ("invalid", Vec::new(), Some(redact(reason))),
        };
        let mut inner = self.inner.lock().expect("reload log lock");
        inner.next_seq += 1;
        let seq = inner.next_seq;
        inner.entries.push_front(ReloadEntry {
            seq,
            at_unix,
            outcome: label,
            changed,
            reason,
        });
        while inner.entries.len() > CAPACITY {
            inner.entries.pop_back();
            inner.dropped += 1;
        }
    }

    /// The history, newest first.
    pub fn view(&self) -> ReloadView {
        let inner = self.inner.lock().expect("reload log lock");
        ReloadView {
            boot: self.boot.clone(),
            dropped: inner.dropped,
            reloads: inner.entries.iter().cloned().collect(),
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// The reason without anything the file said: the loader's own advice
/// ("What now: …", which the page words itself) is cut, the lines a TOML
/// error echoes from the file (`12 | key = value`, `  | ^^^`) are dropped,
/// every double-quoted run (a value) becomes `"…"`, and the result is one
/// line of at most [`REASON_MAX`] characters.
pub fn redact(reason: &str) -> String {
    let sentence = reason.split(" What now:").next().unwrap_or(reason);
    let kept: Vec<&str> = sentence
        .lines()
        .filter(|line| !is_echoed_line(line))
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != ".")
        .collect();
    let mut out = String::new();
    let mut quoted = false;
    for c in kept.join("; ").chars() {
        match (c, quoted) {
            ('"', false) => {
                quoted = true;
                out.push_str("\"…\"");
            }
            ('"', true) => quoted = false,
            (_, true) => {}
            (c, false) => out.push(c),
        }
    }
    if out.chars().count() > REASON_MAX {
        out = out.chars().take(REASON_MAX).collect::<String>() + "…";
    }
    out
}

/// A line of a TOML error's excerpt: an optional line number, then `|`.
fn is_echoed_line(line: &str) -> bool {
    let rest = line
        .trim_start()
        .trim_start_matches(|c: char| c.is_ascii_digit());
    rest.trim_start().starts_with('|')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_history_is_capped_and_counts_what_fell_out() {
        let log = ReloadLog::new();
        for _ in 0..CAPACITY + 3 {
            log.push_at(&Outcome::Applied { changed: vec![] }, 1);
        }
        let view = log.view();
        assert_eq!(view.reloads.len(), CAPACITY);
        assert_eq!(view.dropped, 3);
        assert_eq!(view.reloads[0].seq, (CAPACITY + 3) as u64, "newest first");
    }

    #[test]
    fn a_reason_never_carries_the_file() {
        let toml = "cfg.toml: is not valid TOML: TOML parse error at line 7, column 3\n  |\n7 | Authorization = \"Bearer s3cret\"\n  |   ^\ninvalid type: string \"s3cret\", expected u64\n. What now: fix it.";
        let out = redact(toml);
        assert!(!out.contains("s3cret"), "{out}");
        assert!(!out.contains("Authorization"), "{out}");
        assert!(!out.contains("What now"), "{out}");
        assert!(out.contains("line 7, column 3; "), "{out}");
        assert!(!out.contains('\n'));
    }
}
