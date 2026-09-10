//! One claim, held by a test: no document in this repository tells a reader
//! to run `chassis sync --protect`.
//!
//! The kit's own drift report recommends it, and following that recommendation
//! here would require two CI checks this project never produces — `main` would
//! then wait forever for checks that never arrive. `CLAUDE.md` carries the
//! reasoning; this test makes sure a later edit cannot quietly drop it, and
//! that a passage mentioning the command always carries the refusal with it.
//! The marker is the pair of words "never run": a bare "never" also matches
//! "checks that never arrive" two lines below, and did, while the warning
//! itself was gone.
//!
//! `docs/KIT.md` is excluded: `chassis sync` writes it and this project never
//! edits it by hand, so a failure there could not be repaired in this repo.
//! If the kit ever prints that recommendation into KIT.md, the answer is a
//! report to the kit, not an edit here.

use std::process::Command;

const COMMAND: &str = "--protect";
const EXCLUDED: [&str; 1] = ["docs/KIT.md"];

/// A paragraph is a block of lines with no blank line inside it.
fn paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n").map(|p| p.to_string()).collect()
}

fn tracked_markdown() -> Vec<String> {
    let out = Command::new("git")
        .args(["ls-files", "*.md"])
        .output()
        .expect("git ls-files");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !EXCLUDED.contains(&l.as_str()))
        .collect()
}

#[test]
fn l13_a_no_document_recommends_running_protect() {
    let mut offenders = Vec::new();
    let mut seen = 0usize;

    for file in tracked_markdown() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for paragraph in paragraphs(&text) {
            if !paragraph.contains(COMMAND) {
                continue;
            }
            seen += 1;
            if !paragraph.to_lowercase().contains("never run") {
                offenders.push(format!("{file}: {}", paragraph.trim()));
            }
        }
    }

    assert!(
        seen > 0,
        "no passage mentions `chassis sync {COMMAND}` any more. If the warning \
         was deliberately removed, remove this test in the same commit and say \
         why; an accidental deletion is what this line is here to catch."
    );
    assert!(
        offenders.is_empty(),
        "a passage names `chassis sync {COMMAND}` without the words \"never run\" \
         in the same paragraph (see the warning in CLAUDE.md):\n\n{}",
        offenders.join("\n\n")
    );
}

#[test]
fn l13_b_the_warning_names_the_two_checks_that_would_deadlock_main() {
    let claude = std::fs::read_to_string("CLAUDE.md").expect("CLAUDE.md");
    for check in ["cargo-deny", "container build"] {
        assert!(
            claude.contains(check),
            "the warning no longer names `{check}` — the reader cannot tell \
             which check `main` would wait for"
        );
    }
}
