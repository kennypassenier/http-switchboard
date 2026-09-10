//! L12 — the checks CI stopped running are the checks the release tier
//! runs (standing rule 38, applied 2026-09-10).
//!
//! Kenny's test policy says to run as much as possible locally, to test
//! only what changed at commit time, to run the whole suite at a release,
//! and to send to GitHub Actions only what has to go there. Applying it
//! left this project with one CI job and three checks that moved into
//! `.claude/hooks/gates.project.sh`.
//!
//! Two lists that promise the same thing and nothing comparing them is
//! exactly the shape rule 38 was written about: the short list stays green
//! while the complete list is red. So this test lays them side by side.
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

/// The checks that used to be their own CI job and now live in the
/// release tier. Each is named by the command that would run it, so the
/// assertion cannot pass on a comment that merely mentions the word.
const MOVED: [(&str, &str); 3] = [
    ("cargo-deny", "cargo deny check all"),
    ("the container build", "docker build"),
    ("the real-kyu end-to-end suite", "cargo test --test l4_pump"),
];

#[test]
fn l12_ci_runs_one_job_because_the_rest_runs_locally() {
    let ci = read(".github/workflows/ci.yml");
    let jobs: Vec<&str> = ci
        .lines()
        .skip_while(|l| !l.starts_with("jobs:"))
        .filter(|l| {
            let bytes = l.as_bytes();
            bytes.len() > 3 && &l[..2] == "  " && bytes[2] != b' ' && l.trim_end().ends_with(':')
        })
        .collect();
    assert_eq!(
        jobs.len(),
        1,
        "CI is meant to carry only the one check branch protection needs; it declares {jobs:?}. \
         What now: either move the extra job's check into the release tier of \
         .claude/hooks/gates.project.sh, or say here why CI grew a second job."
    );
    assert!(
        ci.contains(r#"branches: ["**"]"#),
        "CI must run on every branch: branch protection is strict, the flow is branch → green → \
         fast-forward, and a branch push that produces no checks can never reach main."
    );
}

#[test]
fn l12_every_check_ci_gave_up_is_run_by_the_release_tier() {
    let gates = read(".claude/hooks/gates.project.sh");
    let (_, release_tier) = gates
        .split_once("# ── release tier ──")
        .expect("gates.project.sh must mark where its release tier starts");

    for (name, command) in MOVED {
        assert!(
            release_tier.contains(command),
            "{name} runs in neither CI nor the release tier — it runs nowhere.\n  \
             expected the release tier to run: {command}\n\
             What now: add it back to .claude/hooks/gates.project.sh, or add its job to \
             .github/workflows/ci.yml and update this list."
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
