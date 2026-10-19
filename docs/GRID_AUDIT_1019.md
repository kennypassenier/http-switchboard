# Grid and vertical alignment audit (19/10/2026)

Held until the kp-themes subgrid primitive and its 1 px alignment check
land (coordinator, 19/10/2026 06:59). Then every view moves onto it, the
check runs, the changes go to Kenny as one bundled demo, and 3.5.1 follows
only after his verdict.

Measured with a 1 px edge checker on 3.5.0 (two profiles, one kyu with the
hub unreachable, one HTTP): `/` 15 faults at 1280 px and 15 at 390 px;
after the two commits on this branch 11 and 12. `/translate`: 13 at both
widths, untouched.

Done on this branch:
- Needs-attention band items and problem alerts share row heights.
- View / Profile / Press-a-cell rows on one label column and equal tracks;
  "Quiet time allowed" at the card's end edge.

Still open:
- Last 24 hours: Page refresh label and chooser off the shared column;
  "Refresh now" ends 104 px short of the card edge; the history note sits
  outside the control track; history Profile and Tiles rows are flex;
  title and timestamp centres 2.5 px apart.
- Recheck table: budget column text at 561 px, meter at 571 px.
- Reloads: title and count centres 3.5 px apart.
- Settings head at 390 px: title and badge centres 32 px apart.
- Path of a profile head: name and state badge not on a grid.
- /translate: tool rows of unequal widths (27 px spread) and heights
  (28 vs 36 px); Wrap and Copy not in shared columns; the profile note
  starts 196 px left of the result column; input column bottom 34 px off
  the result card.
- Not audited: the Map view; history rows (no deliveries in the window).

Kit/package wishes: `.kp-code__tools` buttons 36 px vs `.kp-jsonfield__tools`
28 px; `.kp-refresh__bar` is flex, so "Refresh now" cannot sit on a track;
`.kp-attention` and `.kp-problems` are inline-size containers, which stops
a subgrid from sizing through them.
