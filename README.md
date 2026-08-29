# HTTPSwitchboard

A switchboard is the panel where an incoming call is connected to the line
it actually needs to reach: the operator decides where it goes, not the
caller. That is this project's job. A system sends a message in the shape
it happens to speak; HTTPSwitchboard translates it and delivers it where
*our* configuration says — the sender never knows the receiver, and the
receiver never knows the sender.

> **Status: milestone L0.** The skeleton and its gates exist; no feature is
> built yet. See `docs/REALIZATION_PLAN.md` for what lands when.

The first customer is Prometheus Alertmanager, whose webhook format is
fixed and does not fit Home Assistant's receiver. The chain:

```
Alertmanager     →  kyu topic alerts.raw
HTTPSwitchboard  →  subscribes, translates
                 →  kyu topic alerts.homelab
hub-bridge       →  Home Assistant webhook  →  notification dispatcher
```

## Development

This project follows the development procedure in `~/Projects/dev-procedure`.
Every phase gate is recorded in `docs/`.

**One-time setup after cloning** — the gates are git-native, but
`core.hooksPath` is local config a clone cannot carry:

```bash
git config core.hooksPath .githooks
```

From then on a commit is refused unless `cargo fmt --check`, `cargo clippy
-D warnings` and the full test suite pass, and unless the message carries
the feature IDs it implements (`[K3, W2]`, or `[meta]` for infrastructure).
CI re-runs the same gates on every branch; red blocks `main`.

## Licence

MIT or Apache-2.0, at your option.
