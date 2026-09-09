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

**Awaiting Kenny's sign-off** (found 2026-09-09, during the 3.0.0 release
gate; the documents themselves are already corrected because they were
simply wrong).

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
4. **How recurrence is prevented.** The proposal: extend
   `tests/l10_docs.rs` from "every documented command parses" to "every
   documented claim about a status code is true" — the test starts the
   service, and for each documented `curl … /healthz` line with a stated
   status, asserts the service really answers it. Concretely: a table in
   the test of (path, condition, expected status) that the documents and
   the test share, so a document and the binary cannot drift apart
   silently.
5. **What the remedy costs.** Roughly 60 more lines of test and a running
   service inside it (the harness of T1 already provides one). It also
   constrains how the documents state a status: as a number, next to the
   path.
6. **Who or what enforces it.** Code: the same suite the gate and CI
   already run.
7. **How and when it is measured.** At the first commit after this one
   that touches a health claim in any document — the new assertion must
   be red before that document is corrected and green after. Until that
   measurement has happened this stays open in this project.
8. **The fallback if the measurement fails.** If tying prose to status
   codes proves too brittle to keep honest, the narrower fallback is a
   single test asserting that no document contains the string
   `?strict=1` — the marker of the retired split — written down as a
   reduced list rather than quietly applied.
9. **When the measure is reviewed.** At the next kit upgrade that changes
   what `/healthz` answers.
