<!-- id-translations: enforced -->

# Identifier translations — http-switchboard

One row per identifier that has moved from the old shape to the house
scheme (standing rule 4, policy set 2026-09-09). Nothing is renamed in
bulk: an old identifier is translated only when it surfaces by itself.

The old name may not survive anywhere else in the tracked files once it
is listed here; `~/Projects/dev-procedure/hooks/check-ids.sh` refuses the
commit otherwise. This file is the one place the old names live.

| Old | New | Moved | Why it surfaced |
| --- | --- | ----- | --------------- |
| S1 | scope-flagship-1 | 2026-09-26 | The flagship criterion (one genuine alert on the phone) goes to Kenny in a form after the chain carried a test alert. The Phase 5 enforcement question that shared the name — commit gates — became ask-gates-1 in the same commit; its siblings S2-S7 there, and the success criteria S2-S7 in SCOPE.md, are untouched |
| M4 | feat-internet-1 | 2026-09-26 | The inbound-from-the-internet round is asked again in the 2026-09-26 form |
| W10 | feat-reload-1 | 2026-09-26 | Config reload without restart is re-rated in the 2026-09-26 form |
