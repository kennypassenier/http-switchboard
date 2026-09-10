# Prompt for the Homelab Rust session — upgrade http-switchboard on CT 109

Copy everything below the line into a session opened in
`~/Projects/homelab`.

---

http-switchboard 3.0.0 is released (tag `v3.0.0` = `f400d57` on `main`, GitHub release with the binary and `SHA256SUMS`). CT 109 still runs 1.x, and
this upgrade crosses **two major versions at once** — the command line
changed in 2.0.0 and the door changed in 3.0.0. Take it through the
homelab procedure; this is that project's work, not the switchboard's.

**The glibc question is gone as of kit 2.0.0 (2026-09-10).** An earlier
version of this text did not mention it at all, and between then and now
it mattered: every release asset built on the previous kit needed
`GLIBC_2.39`, CT 109 runs Debian 12 with glibc 2.36, and the binary would
not have started there — a restart loop under `Restart=always`, which is
how the Homelab Rust session lost three rollouts in one day.

The kit's `feat-build-1` made the release asset a **static musl binary on
a distroless image**, and the release workflow now refuses to publish one
that links shared libraries.

**Which release you take decides whether that helps you.** The tagged
**3.0.0** asset was built before this and still needs `GLIBC_2.39`: on
Debian 12 it does not start, and under `Restart=always` that is a restart
loop. The **first release cut from kit 2.0.0** is static and runs on
Debian 12 and 13 alike. So: take that release, or move CT 109 to Debian
13 first — either works, and doing both is fine. Check before installing:

```bash
objdump -T <the downloaded binary> | grep -c GLIBC_
```

Zero means static, and the host's Debian version stops mattering.

**What is on CT 109 today** (measured 2026-09-09 from the Proxmox host):

- unit `http-switchboard`, `active`
- binary `/usr/local/bin/http-switchboard`, a 1.x build
- config `/appdata/kyu/http-switchboard-config/config.toml` (0600 root)
- env `/appdata/kyu/http-switchboard-config/token.env` (0600 root), which
  holds **only** `KYU_TOKEN`
- exactly one profile, `alertmanager`: from the kyu topic `alerts.raw` to
  a Home Assistant webhook. No profile with an `http_path`, no
  `inbound_token`.

**Where it must end up** — the switchboard repo ships all four files:

- binary `/opt/http-switchboard/bin/http-switchboard`, owned by the
  `http-switchboard` user
- unit from `deploy/http-switchboard.service` (`Type=notify`,
  `ExecStartPre=… --check`, `EnvironmentFile=…/token.env`,
  `HTTP_SWITCHBOARD_STATE_DIR=/appdata/kyu/http-switchboard-config`)
- stack file `deploy/service.yml` (vmid 109, `update_cmd` via
  `systemd-run`, `release_repo`/`release_asset` for self-update)
- state directory unchanged: `/appdata/kyu/http-switchboard-config`

**The one thing that will break the upgrade if it is done in the wrong
order.** 3.0.0 compiles the chassis dashboard in, because the inbound
door now runs on chassis client tokens. That makes two secrets
mandatory — `HTTP_SWITCHBOARD_TOKEN` and `HTTP_SWITCHBOARD_SECRET_KEY` —
and the unit runs `--check` as `ExecStartPre`, so it refuses **before**
the service ever binds. Swapping only the binary leaves CT 109 with a
stopped service.

The order that works:

1. On CT 109, `/opt/http-switchboard/bin/http-switchboard gen-secret` on
   a real terminal. It refuses a pipe on purpose, so a secret cannot land
   in a log.
2. Put both printed lines into
   `/appdata/kyu/http-switchboard-config/token.env`, next to the existing
   `KYU_TOKEN`. Keep the file 0640 `root:http-switchboard`.
3. Run `--check` under the real environment and confirm it passes.
4. Only then swap the binary and `systemctl restart`.

Keep the 1.x binary beside the new one until step 4 has proven itself;
that is the rollback.

**The second thing, and it is a trap this handover previously got
wrong.** Do **not** point any healthcheck at `/healthz` over HTTP. Since
the kit took that endpoint over, `/healthz` answers **503 as soon as a
profile is failing** — measured on a running 3.0.0, and the same with or
without `?strict=1`. An orchestrator probing it would restart the service
every time *Home Assistant* is down, and every restart resets the pump
state, turning "exactly one failure event" into one per restart.

The liveness answer is the binary's own:
`http-switchboard --healthcheck` prints `alive=true status=degraded` and
**exits 0** while a profile is failing. The shipped `Dockerfile` and
`deploy/service.yml` already call it. Uptime Kuma watches `/healthz` over
HTTP; nothing that can restart the service does.

**What does not change.** The same unit name, the same state directory,
the same port, the same one profile. That profile reads from a kyu topic
and never passes through the door, so **no sender needs a token today** —
`chassis clients issue` exists for the first one that does, which will be
Alertmanager if it is ever pointed straight at this service instead of at
kyu.

**Two smaller notes.**

- Self-update is on, supervised: the homelab runs the `update_cmd` in
  `deploy/service.yml` on its schedule. `--check` runs again as part of
  the staged probe, so the two secrets must be in place for updates to
  work at all.
- The deployed config belongs in neither repository. `kennypassenier/
  homelab` is public too (checked 2026-08-30). The real config lives in
  `/appdata/kyu/http-switchboard-config/` and restic backs it up.

**Do not start before the release is signed.** The self-updater refuses a
release without `SHA256SUMS.minisig` and `VERSION`, and so should you:
check that both assets exist on
`https://github.com/kennypassenier/http-switchboard/releases/tag/v3.0.0`
before pulling anything down, and verify the binary's checksum against
the signed manifest with the key baked into the shipped software.

**When it is done, the switchboard side would like to know two things:**
whether `--check` passed on the first try, and what `--healthcheck`
answered after the restart. Both go back to the http-switchboard session
so its runbook can record the drill.
