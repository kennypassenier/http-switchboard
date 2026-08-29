# Fixtures

`alertmanager_firing.json` was **produced by Alertmanager itself**, not
written by hand: `prom/alertmanager:latest` was started with a webhook
receiver pointing at a local listener, an alert was posted to its
`/api/v2/alerts`, and the body it delivered was captured verbatim
(2026-08-29). Standing rule 9: where correctness depends on an external
tool's format, pin one real artifact that tool produced — synthetic
vectors only prove we agree with ourselves.

Worth noting, because it is exactly what a hand-written fixture would
have missed: the real body carries `notification_reason` and
`routeLabels`, and `endsAt` is the zero time `0001-01-01T00:00:00Z`
rather than absent.
