//! L12 — nothing runs on GitHub Actions; the gates run locally
//! (standing rule 38, applied 2026-09-10; CI dropped 2026-09-29).
//!
//! Kenny's test policy says to run as much as possible locally, to test
//! only what changed at commit time and to run the whole suite at a
//! release. Applying it first left this project with one CI job and three
//! checks that moved into `.claude/hooks/gates.project.sh`; since
//! 2026-09-29 GitHub Actions builds nothing at all, and `chassis release`'s
//! gate plus that release tier run everything CI ever ran.
//!
//! A check that "moved" and a place it moved to, with nothing comparing
//! them, is exactly the shape rule 38 was written about. So this test
//! holds the repository to having no CI workflow, and the release tier to
//! running the checks that used to be CI jobs.
//!
//! Placement: integration test — it reads the repository. Timing: commit
//! subset; it is three file reads and costs nothing.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// The checks that used to be CI jobs and now live in the release tier. Each is named by the command that would run it, so the
/// assertion cannot pass on a comment that merely mentions the word.
const MOVED: [(&str, &str); 3] = [
    ("cargo-deny", "cargo deny check all"),
    ("the container build", "docker build"),
    ("the real-kyu end-to-end suite", "cargo test --test l4_pump"),
];

#[test]
fn l12_the_repository_carries_no_ci_workflow() {
    let ci = root().join(".github/workflows/ci.yml");
    assert!(
        !ci.exists(),
        "{} exists, but GitHub Actions builds nothing here: the gate runs locally, through \
         `chassis release` and the release tier of .claude/hooks/gates.project.sh. \
         What now: move the workflow's checks into that release tier and delete the file, \
         or say here why a CI workflow came back.",
        ci.display()
    );
}

#[test]
fn l12_every_check_ci_used_to_run_is_run_by_the_release_tier() {
    let gates = read(".claude/hooks/gates.project.sh");
    let (_, release_tier) = gates
        .split_once("# ── release tier ──")
        .expect("gates.project.sh must mark where its release tier starts");

    for (name, command) in MOVED {
        assert!(
            release_tier.contains(command),
            "{name} is not run by the release tier — nothing else on GitHub runs it either.\n  \
             expected the release tier to run: {command}\n\
             What now: add it back to .claude/hooks/gates.project.sh."
        );
    }
}

#[test]
fn l12_the_release_tier_is_reachable_without_anyone_remembering_a_flag() {
    // The tier is chosen from the repository — a commit that moves the
    // package version is the release commit, and nothing else is. If that
    // detection is ever replaced by an environment variable somebody has
    // to set, the three checks above quietly stop running.
    let gates = read(".claude/hooks/gates.project.sh");
    assert!(
        gates.contains("git diff --cached") && gates.contains("refs/tags/v"),
        "the release tier must be detected from the repository, not from a flag a person sets. \
         What now: keep the version-versus-tag check, or write down here what replaced it."
    );
}
