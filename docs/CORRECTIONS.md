# Corrections

Live-found faults in this project, one section each, with the nine fields
FORM_PROTOCOL §8 requires. The form Kenny signs is a summary; this is the
record.

## C1 · The README documented a command the released binary refuses

**Signed off by Kenny 2026-09-09 ("Klopt", all nine fields as written).**

1. **What went wrong.** `README.md` line 74 showed
   `http-switchboard /etc/http-switchboard/config.toml`. The released
   2.0.0 binary answers
   `error: unrecognized subcommand '/etc/http-switchboard/config.toml'`.
   The status block above it still said "not yet released" and named 99
   tests, three days and one release after both stopped being true.
2. **Which gate let it through.** The Phase 8 document approval and the
   2.0.0 release report. The chassis migration moved the config path from
   a positional argument to `--config` and wrote that change into the
   CHANGELOG's own Migration section — and the README's "Running it"
   block was never run again.
3. **Where else the same fault sits.** Measured 2026-09-09 across
   `README.md` and every file in `docs/`: nowhere else. The runbook, the
   handover and the debugging guide all carry a dated 2.0.0 note. Only
   those two places in the README were left behind. The fault is not
   "the README is stale" but "a command in a document was never executed
   by anything", which is why the measure below is about executing them
   rather than about reviewing them.
4. **How recurrence is prevented.** `tests/l10_docs.rs` extracts every
   `http-switchboard …` line from the fenced blocks of `README.md` and
   `docs/USER_GUIDE.md`, folds shell line continuations, and hands each
   one to the built binary with a config path that does not exist. The
   test fails on `unrecognized subcommand`, `unexpected argument`,
   `invalid value` or `unexpected value`, and also fails if it finds
   fewer than four commands — a test that greps for its own input must
   not pass by finding nothing.
5. **What the remedy costs.** About 150 lines of test, and one
   convention: a command in those two documents is written literally, one
   per line, inside a fenced block. It also spends a few seconds per run
   starting the binary once per documented command.
6. **Who enforces it.** Code. `cargo test` runs it, so the pre-commit
   gate runs it and CI runs it again. Not discipline.
7. **How and when it was measured.** At the commit of this round that
   touches the README — the test was written first and failed on exactly
   line 74, naming the command and the binary's answer, and passed after
   the fix. Done 2026-09-09; the loop is closed.
8. **The fallback if the measurement fails.** If extracting commands from
   Markdown turns out to be brittle — multi-line blocks, placeholders the
   parser mangles — the test narrows to a single hard assertion: no
   documented command passes a bare path as its first argument. That
   narrowing is written down here rather than quietly applied.
9. **When the measure is reviewed.** At the next kit upgrade that changes
   the command line — the next `chassis sync` that reports drift on the
   CLI surface.

## C2 · Three documents described health endpoints the service no longer has

**Signed off by Kenny 2026-09-09 ("Klopt", all nine fields as written).
Measurement done the same day — see field 7.** Found during the 3.0.0
release gate; the documents themselves were corrected before the tag,
because they were simply wrong.

1. **What went wrong.** `README.md`, `docs/OPERATIONS_RUNBOOK.md` and
   `docs/HANDOVER_HOMELAB.md` all described the 1.x split: plain
   `/healthz` as lenient liveness, `?strict=1` as strict readiness.
   Measured against a running 3.0.0 on 2026-09-09:
   `curl -o /dev/null -w %{http_code} /healthz` → **503** with one
   profile failing, identical to `?strict=1`. The handover went furthest
   and told the homelab session to point the container healthcheck at
   `/healthz` — which would restart this service every time Home
   Assistant is down, the exact failure that point was written to
   prevent.
2. **Which gate let it through.** The 2.0.0 release gate, where the kit
   took the endpoint over; and then correction C1 in this same round,
   whose sweep answered "nowhere else" after measuring only *commands*.
3. **Where else the same fault sits.** The fault is not "the health docs
   are stale". It is **a document making a claim about behaviour that
   nothing executes** — commands are one surface of that, prose is
   another, and C1 fixed only the surface the fault first appeared on.
   That is the trap FORM_PROTOCOL §8 field 3 names. Measured now across
   all documents: the health claims were the remaining instance; the
   shipped `Dockerfile` and `deploy/service.yml` were always correct.
4. **How recurrence is prevented.** Built as `tests/l11_health_claims.rs`,
   and it went one better than the proposal. Asserting "the prose is true"
   against prose is brittle, so the test inverts it: it **measures** every
   cell of a health table against a running service — `/healthz`,
   `/healthz?strict=1` and `--healthcheck`, each with every profile
   working and with one failing — and then requires `README.md` and
   `docs/OPERATIONS_RUNBOOK.md` to contain exactly that table between
   `<!-- health-table:start -->` markers. The binary writes the
   document's table, so a document cannot drift from it. Two facts are
   additionally pinned as assertions rather than prose: the two paths
   answer alike (the 1.x split is gone), and `--healthcheck` exits 0
   while a profile is failing.
5. **What the remedy costs.** About 210 lines of test, and a running
   service inside it (the kit harness adopted as T1 provides one). It
   constrains the two documents to carry a generated block rather than a
   hand-written sentence — which is the point.
6. **Who or what enforces it.** Code: the same suite the gate and CI
   already run.
7. **How and when it is measured. Done 2026-09-09.** The test was written
   first and failed twice, both times usefully. The first failure was its
   own: a blocking `Command::output()` on a single-threaded test runtime
   stopped the very server it was probing, so `--healthcheck` read exit 1
   in every state — a measurement that would have contradicted the
   correct one taken against a separate process. Moved to a blocking
   thread on a multi-threaded runtime, it then failed for the right
   reason: neither document carried the table. Both were given it, and
   the test went green. The loop is closed.
8. **The fallback if the measurement fails.** If tying prose to status
   codes proves too brittle to keep honest, the narrower fallback is a
   single test asserting that no document contains the string
   `?strict=1` — the marker of the retired split — written down as a
   reduced list rather than quietly applied.
9. **When the measure is reviewed.** At the next kit upgrade that changes
   what `/healthz` answers.

## fix-3 and fix-4 · Two documented instructions that nothing ever executed

**Signed off by Kenny 2026-09-10 ("Klopt", all nine fields as written).**
Found during the kit 2.0.0 round; both were repaired before the commit
that carries them, because they were simply false.

1. **What went wrong.** `http-switchboard test --config <a real config>`
   answered "the file is not valid TOML" on a file that is valid TOML and
   that the service starts from happily: the verb runs before the kit
   parses anything, so nothing stripped the kit's own knobs and table
   sections. And two container commands in the operations runbook reached
   into the image at `/opt/http-switchboard/bin/…`, which is the native
   install path; the image has always used `/usr/local/bin/…`.
2. **Which gate let it through.** The documented-command test existed but
   read only lines beginning with this service's own name. A `docker` line
   and the behaviour of a verb against a real file both fell outside it.
3. **Where else the same fault sits.** The property is "a document
   prescribes an action that nothing executes". Searched with
   `grep -rn 'docker exec\|docker run\|http-switchboard ' README.md docs/*.md`,
   which returns every prescribed action rather than only the ones this
   project's name opens. The two repaired places were the last two that no
   test covered.
4. **How recurrence is prevented.** Two tests, both built and green:
   `w4_the_dry_run_reads_a_real_config_and_not_only_the_example` runs the
   verb against a config carrying kit knobs and a `[[notify.webhook]]`
   table, and `l10_a_documented_container_command_uses_the_path_the_image_has`
   compares every documented container command against the Dockerfile's
   own `COPY` target.
5. **What the remedy costs.** About ninety lines of test, and the
   convention that a path in a container command is written literally in
   the document rather than described.
6. **Who or what enforces it.** Code: both run in the commit tier of the
   gates and again in CI.
7. **How and when it was measured. Done 2026-09-10.** Each was driven red
   first — the dry-run test on the misleading TOML message, the container
   test on the exact runbook line, naming both paths — and green after the
   fix.
8. **The fallback if the measurement fails.** If extracting commands from
   Markdown proves brittle, the narrower fallback is a single assertion:
   no document names a path under `/opt/` inside a docker line. That
   narrowing is written down here rather than quietly applied.
9. **When the measure is reviewed.** At the next kit upgrade that changes
   the command line or the image path.

**A note the correction did not need but the round produced.** A third
instance of the same property was found in the same pass and repaired
without its own correction: a runbook paragraph still presented
`?strict=1` as the strict probe, two paragraphs below the correction from
2026-09-09 that removed exactly that claim. It is recorded here because
the count matters: this property has now surfaced five times in this
project, and each measure so far has been written for the surface the
fault appeared on rather than for the property itself.

## fix-5 · A release note described another repository's change from memory

**What the document said.** The `[3.1.1]` section of `CHANGELOG.md` said
that kit 2.0.2 "stops `chassis sync --protect` from tightening the branch
protection". That is not what 2.0.2 did. It turns `enforce_admins` off in
the protection `--protect` writes; *which* checks `--protect` would
require is untouched. Written that way, the sentence reads as the opposite
of the warning committed to `CLAUDE.md` an hour earlier: this project must
never run that command, because it would demand two checks this project's
CI never produces and leave `main` waiting forever.

1. **What the fault actually was.** The entry was written from the memory
   of a conversation about the kit, not from the kit's own release notes.
   The kit's text was two commands away: `sed -n '/^## \[2.0.2\]/,…'` on
   its `CHANGELOG.md`, which is what settled it afterwards.
2. **Which gate let it through.** None looks at prose about another
   repository. `fmt`, `clippy` and 110 tests all passed, and the release
   tier added `cargo deny`, the container build and the real-kyu suite —
   none of which reads a sentence.
3. **Where else the same fault sits.** The property is "a document
   describes another repository's behaviour from memory". Checked the
   other place this round produced: the `CLAUDE.md` warning also carried
   an unmeasured causal claim, "Since kit 2.0.2 `chassis sync --remote`
   reports…". The drift was only ever measured on 2.0.2; whether older kit
   versions reported it was never tested. Corrected to say what was
   measured and when.
4. **How recurrence is prevented.** `tests/l13_protect_warning.rs`. It
   holds the claim that matters — no document in this repository may name
   `chassis sync --protect` without the words "never run" in the same
   paragraph — plus a second test requiring the warning to keep naming the
   two checks that would deadlock `main`. It cannot check that a sentence
   about the kit is true; it can make the one dangerous sentence
   impossible to write.
5. **What the remedy costs.** About eighty lines of test, and one
   convention: a passage naming that command carries its refusal in the
   same paragraph, so a reader who sees only the paragraph sees both.
6. **Who or what enforces it.** Code: the commit tier of the gates, and
   again in CI.
7. **How and when it was measured. Done 2026-09-10.** Driven red by
   removing `Never` from the warning in `CLAUDE.md` — and the first
   version of the test stayed **green**, because it looked for the bare
   word "never", which also matches "checks that never arrive" two lines
   below. The marker was tightened to the pair "never run", driven red
   again, and this time it failed; green with the warning restored.
8. **The fallback if the measurement fails.** If a legitimate passage ever
   needs to name the command without refusing it — a report to the kit,
   say — the fallback is an explicit allow-list of file plus paragraph in
   the test, written down there rather than loosening the marker.
9. **When the measure is reviewed.** At the next kit release that changes
   what `chassis sync --protect` writes, or when this project's CI grows
   the two checks, at which point the warning becomes wrong and both tests
   should be deleted in the same commit.

**What this cost in the release.** Nothing recoverable: the sentence is in
the tagged `v3.1.1` and stays there. The published release notes on GitHub
carry different text and never had it — measured with
`gh release view v3.1.1 --json body`. The correction lives on `main`.
