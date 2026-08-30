# Operations runbook — HTTPSwitchboard

Numbered procedures. Written from what was actually run, not from what
should work; anything not yet exercised says so.

## 1 · Check a config before restarting anything

```bash
http-switchboard --check-config /etc/http-switchboard/config.toml
```

Exit 0 and a profile count means it would start. Any other exit prints
the file, the profile, what is wrong and what to do about it. Run this
before every restart — the service refuses to start on a bad config
(K10), so this turns a failed restart into a caught typo.

## 2 · See what a profile would send, without sending it

```bash
http-switchboard test \
  --config /etc/http-switchboard/config.toml \
  --profile alertmanager \
  --input recorded-message.json
```

Prints the destination, the content type, the header NAMES (never their
values) and the rendered body. Nothing is sent. Inside the container,
which has no shell:

```bash
docker run --rm --entrypoint /usr/local/bin/http-switchboard \
  -v /path/config.toml:/c.toml:ro -v /path/message.json:/m.json:ro \
  ghcr.io/kennypassenier/http-switchboard:latest \
  test --config /c.toml --profile alertmanager --input /m.json
```

## 3 · Is it working, and what does "working" mean here

```bash
curl -s http://<host>:8080/healthz | jq            # is the process alive?
curl -si http://<host>:8080/healthz?strict=1 | head -1   # is it doing its job?
```

Two questions, two answers, on purpose. Plain `/healthz` is **liveness**:
200 while the process can serve, whatever the profiles are doing. That is
what the container's own healthcheck asks, so the orchestrator never
restarts this service because Home Assistant is down — and every such
restart would reset the pump state, turning one failure event into one
per restart.

`?strict=1` is the one that goes **503** when any profile is failing,
denied or cut off. **Point Uptime Kuma at that one.** Either way the body
names each profile, its state and how long ago it last succeeded.

Counters, including delivery duration, are at `/metrics` in Prometheus
format. Neither endpoint ever echoes message content.

## 4 · The hub refuses us (`state: "denied"`)

Almost always a rotated `KYU_TOKEN` with this service not restarted.
Mint an app token on kyu's Apps page, put it in the environment the
service starts in — through the homelab vault, which resolves it from
latch at deploy time — and redeploy. The service reads secrets once, at
startup, on purpose.

## 5 · Restore from zero

**Not yet exercised — see the note at the end.** The intended procedure:

1. `git clone` this repository.
2. Deploy the preset from `~/Projects/homelab` onto a NEW container (A1
   forbids the orchestrator managing an existing guest).
3. The orchestrator resolves `KYU_TOKEN` with `latch cat` and composes it
   into the container's environment from the host vault.
4. Copy the deployment config into place and run `--check-config`.
5. Publish one recorded alert onto `alerts.raw` and confirm it arrives in
   Home Assistant.

There is no state to restore beyond that: the service stores nothing
(NG3). The config lives in git, the secret in latch, the container in the
preset.

> **Drilled on 2026-08-30 — this is a proven procedure, not a plan.**
> On a scratch container (`192-scratch-http-switchboard`, 10.10.10.92,
> deleted afterwards) the whole thing was destroyed and rebuilt from
> nothing: `pct destroy`, recreate, push the binary, the config, the unit
> and the secret, `systemctl enable --now`, and one message published on
> the real hub arrived at Home Assistant again. Timings from that run:
> the rebuilt service was active within 5 seconds and the message was
> delivered on its first poll.
>
> One step remains untested: deploying **through the homelab preset**
> rather than by hand. That belongs to the homelab project, which is
> where the preset lands (Kenny's sequence: fold it in just before the
> retrospective).

## 6 · The container, when there is no shell to help you

The image is distroless: no shell, no curl, no `ps`. Three things still
work from outside it:

```bash
docker logs <container>                    # JSON lines, one per message
docker exec <container> /usr/local/bin/http-switchboard --healthcheck \
  http://127.0.0.1:8080/healthz            # the binary asks itself
docker run --rm --entrypoint /usr/local/bin/http-switchboard \
  -v /path/config.toml:/c.toml:ro -v /path/message.json:/m.json:ro \
  <image> test --config /c.toml --profile <name> --input /m.json
```

A container that restarts in a loop is almost always refusing its
config — the reason is the last line on stderr in `docker logs`, and it
names the file, the profile and the remedy.

## 7 · Rotating the hub token

The service reads secrets once, at startup, on purpose. So:

1. Mint a new app token on kyu's Apps page.
2. Put it in the environment the service starts in (on the homelab, the
   orchestrator composes it into the container from the host vault).
3. Redeploy or restart.

Forgetting step 3 shows up as `state: "denied"` on every profile, with a
log line naming the Apps page. It is not silent.

### The Home Assistant webhook id, and one accepted exposure

The alert automation's webhook id is deployment configuration and is
deliberately not written down in this repository, which is public.

**Accepted, by Kenny's decision at the Phase 8 gate (2026-08-30):** the
id was written into `docs/SCOPE.md` when the automation was first
recorded, and that commit was pushed before the mistake was caught. It
has been removed from the file, but it remains in the pushed history.
Kenny chose **not** to rotate it: the automation is `local_only`, so the
id is only usable from inside the LAN. Recorded here rather than left as
a silent hole — if the trigger ever loses `local_only`, rotating it is
the first thing to do.
