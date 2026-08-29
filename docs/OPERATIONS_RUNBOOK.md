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
curl -s http://<host>:8080/healthz | jq
```

`200` with `"status":"ok"` means every profile is doing its job. `503`
with `"status":"degraded"` means at least one profile is failing, denied
or cut off from the hub — the body names which, and how long ago it last
succeeded. Uptime Kuma only needs the status code.

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

> **Honest status, 2026-08-30.** Steps 1, 4 and 5 have been run in the
> test suite and by hand; steps 2 and 3 have NOT — the container has not
> been deployed to Kenny's Proxmox yet, and the preset has not been added
> to the homelab repository. Until that drill is done, this is a plan and
> not a proven procedure, which is exactly the distinction M3 was written
> to force.
