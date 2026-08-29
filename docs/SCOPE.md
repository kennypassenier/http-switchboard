# Scope — HTTPSwitchboard

Phase 0 output. **Approved via the Phase 0 gate form on 2026-08-29** —
every statement below reflects Kenny's actual answer, not the draft.
Seventeen statements were approved unchanged in the first round; six
were reopened in a second round and are marked *(amended at the gate)*.
Frozen except through a mini-round (`FORM_PROTOCOL.md` §5) once later
phases are under way.

**Naming note, 2026-08-29:** the hub was renamed `mailbox` → `kyu`
(version 2.0.0) on the same day this document was written; every
mention here follows the new name. The HTTP verbs, the address
(`10.10.10.9:8080`) and the contract are unchanged — only names moved
(`MAILBOX_*` → `KYU_*`). No decision in this document changed.

## Naming

**HTTPSwitchboard** (Kenny, 2026-08-29, after three rounds). A
switchboard is the panel where an incoming call is connected to the
line it actually needs to reach; the operator, not the caller, decides
where it goes. That is this project's job exactly: the sender never
knows the destination, our configuration does.

Repository, binary and package are `http-switchboard` — lowercase
kebab, like every other project under this procedure, and as package
registries require. `HTTPSwitchboard` is the name in prose: README,
documentation, conversation.

Alternatives considered and dropped: `rosetta` (apt for translation but
does not announce HTTP, and collides with Apple's), `hooksmith` /
`hookshift` (Kenny rejected the "hook" family outright), `recast`
(good sound, too many meanings), `patchbay`, `conduit`, `gasket`.

**Action item for Phase 8:** this rationale opens `README.md`, so the
name is never a mystery to a future reader.

## Goals

- **G1 · Mission.** One service on the home network that accepts a
  message in one shape and delivers it in another shape to a
  destination that *our* configuration chooses. It is generic by
  construction: Alertmanager is its first customer, not its subject.
  The sending system needs no knowledge of the receiver, and the
  receiver needs no knowledge of the sender.

- **G2 · The profile is the whole model.** A profile names a source, a
  translation and exactly one destination, and lives in a config file.
  A new coupling is a new profile — no code, no release. One profile
  has one destination: fan-out is several profiles on the same source,
  so that "delivered or not" stays a question with two answers instead
  of four, and a failing log destination cannot disturb a phone
  notification.

- **G3 · Two shapes of source, two shapes of destination.** A profile's
  source is either an incoming HTTP request on a path, or a kyu
  topic it subscribes to. Its destination is either a URL it POSTs to,
  or a kyu topic it publishes to. The kyu side is what gives
  the chain a persistence layer without this service owning one.

- **G4 · The translation is a Jinja template.** Field mapping,
  arithmetic, defaults and conditionals in the same language Kenny
  already writes in Home Assistant automations, so nothing new has to
  be learned or remembered:
  `{{ (bytes | int / 1073741824) | round(2) }} GB`.

- **G5 · The whole envelope is translatable, not just the body.**
  Method, target URL or path, headers, content-type and body. That
  includes authentication headers the source cannot set itself — a
  Bearer token for kyu, an API key for a receiver. Secret values
  come from the environment via `latch run`; the config file holds only
  the name (`${KYU_TOKEN}`), never the value.

- **G6 · The first customer, end to end** *(amended at the gate)*.
  Alertmanager does not POST to this service at all: it publishes its
  fixed `alerts` JSON **straight onto a kyu topic**, which its
  webhook config supports (a free-form URL plus a bearer token). The
  chain is:

  ```
  Alertmanager      →  kyu topic  alerts.raw
  HTTPSwitchboard   →  subscribes, translates
                    →  kyu topic  alerts.homelab
  hub-bridge        →  new HA webhook →  script.notification_dispatch
  ```

  The message is therefore durable from the very first hop: this
  service being down, restarting or under development never loses an
  alert. Delivering the new HA webhook and its automation is part of
  this project, not an assumed pre-existing thing; the existing
  `automation.homelab_ops_webhook` is left untouched.

  **Consequence, accepted at the gate:** the flagship path does not
  exercise the HTTP ingress. The S2 proof profile therefore uses the
  HTTP ingress, so both source shapes are proven.

- **G7 · Acknowledge only after delivery** *(added at the gate, Kenny's
  find)*. When a profile's source is a kyu topic, a message is
  acknowledged only after the destination accepted it. A refused
  delivery is left unacknowledged, so the hub redelivers it and, after
  its retries are exhausted, leaves it visibly as a dead letter.
  Acknowledging on receipt would delete exactly the messages worth
  keeping. This is the same order hub-bridge uses: ack only on a 2xx
  from the receiver.

## Non-goals

- **NG1 · No filtering, no dropping.** Everything that arrives is
  translated and forwarded; deciding what deserves attention is the
  receiver's job. Named consequence: Alertmanager also fires a webhook
  when an alert is over (`"status": "resolved"`), so the new HA
  automation carries the `firing` condition. That condition is a
  requirement of this project's HA deliverable, not an implementation
  detail of the automation.

- **NG2 · No aggregation, counting, summing or batching in v1.** One
  message in, one message out. The design keeps a place where a
  collecting stage can be slotted in later without a rebuild, but
  waiting means remembering, and remembering would make this a service
  with state — with a restart question, a power-loss question and a
  backup question it does not have today. Adding up numbers over time
  is Prometheus and Grafana's job; collecting notifications into one
  digest is already the HA dispatcher's hourly bulletin.

- **NG3 · No storage of its own: no queue, no database, no spool.** The
  service is stateless by construction, so `kill -9` at any moment
  costs nothing by definition and there is no state to back up.
  Durability, redelivery and dead letters are kyu's job, which is
  why it sits in the chain.

- **NG4 · Not a poller.** It never goes out on a timer to fetch
  something. It reacts to what arrives — an HTTP request or a message
  on a topic — and nothing else.

- **NG5 · Outbound to the internet yes, inbound from the internet not
  yet** *(amended at the gate)*. A profile may deliver to a URL outside
  the house; that needs no network change and is in scope now.
  Accepting requests **from** the internet is deliberately postponed to
  its own Phase 2 round, because it is a design of its own rather than
  a yes/no. The measured obstacle: everything public arrives through
  one Cloudflare Tunnel and is gated by a single Cloudflare Access
  application on `*.kp-soft.dev` whose policy demands an interactive
  login (one-time PIN or Google, three addresses). No webhook sender
  can satisfy that. The two roads — an Access service token for senders
  we configure ourselves, or an Access bypass on one hostname with this
  service verifying tokens and signatures itself — differ enough to
  deserve their own decision. When that round happens, authentication
  stops being optional, rate limiting becomes mandatory, and the
  architecture-critic and `/security-review` passes become mandatory
  rather than recommended.

- **NG6 · No Home Assistant knowledge inside the project.** No HA
  service calls, no HA API token, no entity names. HA is reached the
  way any other receiver is: an HTTP endpoint described in a profile.
  A transformer that knows about the house is no longer a transformer.

- **NG7 · No web dashboard in v1.** The config file is the
  documentation, and logs plus kyu's own dashboard are where
  traffic is observed.

## Success criteria

- **S1 · The flagship: one real alert on the phone.** A genuine
  Alertmanager alert travels the whole chain and arrives as a
  notification through Kenny's dispatcher — no test curl, no throwaway
  script. At that moment Alertmanager comes off hold.

- **S2 · A new coupling costs a config block.** Proven by adding a
  second profile with a different source shape and a different output
  shape, against a fake source, without touching code or cutting a
  release. That profile uses the HTTP ingress, so the source shape the
  flagship does not exercise is proven here.

- **S3 · A hard kill loses nothing.** `kill -9` at any moment, restart,
  and the only possible damage is a duplicate delivery — never a loss —
  because the service holds nothing and kyu holds the position.

- **S4 · Nothing disappears quietly.** A destination that refuses does
  not swallow a message: it is either already durable on the hub or the
  failure is visible with a remedy in the message. A config that does
  not parse fails closed at startup and says what is wrong and how to
  fix it, rather than starting half-working.

- **S5 · The translation is provable offline.** A real recorded
  Alertmanager payload is pinned as a regression fixture together with
  its expected output, so the mapping is tested against something the
  real tool produced rather than against something we imagined it
  produces.

## Hard constraints

- **C1 · It runs in the homelab as a new managed guest.** The homelab
  orchestrator refuses to manage pre-existing guests, so this cannot be
  deployed onto CT 113 (Prometheus) or LXC 109 (kyu) by it.
  ↳ *A1 = the orchestrator's whitelist-only rule with a hardcoded
  NO_TOUCH list (VMID 100-107, 111, 201-203).* Deployment therefore
  arrives as a preset in `~/Projects/homelab` for a new container. The
  concrete preset is a Phase 2 mandatory item.

- **C2 · Secrets come from latch, never read by this project.** The
  project never reads a `.env` file itself.

  **Dated correction, 2026-08-29 (Phase 4 gate, AR13):** this clause
  originally said the process is started under `latch run`. That is
  not how it reaches the deployment chosen in C1. The homelab
  orchestrator resolves secrets at *deploy* time with `latch cat` and
  ships them into the host vault at `/var/lib/homelab/secrets/`
  (root-only), which composes them into the container's environment.
  So: still latch as the source of truth, still never in git, still
  never read by this project — but the values do land on the host's
  disk, deliberately, and the vault's permissions are the control.
  This is also the only reading under which the container returns by
  itself after a power cut.
  ↳ *latch = Kenny's encrypted .env manager; `latch run --env <env> --
  <cmd>` injects secrets into the child process without writing them to
  disk.*

- **C3 · The hub's address is configuration, not a constant**
  *(amended at the gate)*. It lives in the config file so the hub can
  move, or a second hub can be pointed at for testing, without a
  release; the token beside it is a reference latch fills in:

  ```yaml
  kyu:
    base_url: http://10.10.10.9:8080
    token: ${KYU_TOKEN}
  ```

  The default is today's instance, `10.10.10.9:8080`, over the three
  verbs kyu promises in 2.0.0. That instance is a plain binary
  under systemd on LXC 109, not a container of the orchestrator, so
  nothing may assume its preset is running. Delivery is at-least-once,
  so every consumer in this chain tolerates a duplicate.

- **C4 · The Home Assistant side ships with this project**
  *(worked example approved at the gate)*. A new webhook with its own
  id, `local_only: true` (hub-bridge is inside the house), POST only,
  and an automation that filters on `firing` before calling
  `script.notification_dispatch`. The agreed shape, from Alertmanager's
  fixed payload through to the notification:

  ```
  in   {"alerts":[{"status":"firing",
                   "labels":{"alertname":"FilesystemFull",
                             "instance":"10.10.10.6:9100",
                             "severity":"critical"},
                   "annotations":{"summary":"Filesystem 92% full on lxc-media"}}]}

  out  {"alert":"{{ alerts.0.labels.alertname }}",
        "status":"{{ alerts.0.status }}",
        "severity":"{{ alerts.0.labels.severity | default('warning') }}",
        "instance":"{{ alerts.0.labels.instance | default('unknown') }}",
        "summary":"{{ alerts.0.annotations.summary }}"}

  HA   condition: {{ trigger.json.status == 'firing' }}
       script.notification_dispatch
         title:    "Homelab-alarm: {{ trigger.json.alert }}"
         message:  "{{ trigger.json.summary }} ({{ trigger.json.instance }})"
         priority: critical | warning | info, derived from severity
         ack_id:   "alert_{{ trigger.json.alert }}"
  ```

  `ack_id` is per alert name, so a repeat replaces its own notification
  instead of stacking. Claude creates this automation through the HA
  API on Kenny's explicit per-action go.

- **C5 · Target platform is a Linux container in the homelab.**
  Language, libraries and runtime are a Phase 3 decision and are
  deliberately not settled here — including the obvious-looking one:
  kyu and hub-bridge are Rust and `minijinja` is a Rust crate, but
  obvious is not the same as decided.

- **C6 · A destination never comes from the incoming message.** Only
  the config decides where something is delivered. Without that rule an
  outward-facing profile turns this service into an open relay that a
  stranger can aim anywhere. It holds now, and it is what makes the
  postponed inbound-from-internet round survivable later.

## Build vs buy — the Phase 1 record

Researched 2026-08-29 and put to Kenny as a decision form, one item per
credible alternative. **All six were answered "build our own".** The
reasons are recorded here because a rejected alternative that is not
written down comes back as a question every six months.

- **Home Assistant does it itself.** HA's webhook trigger exposes the
  whole payload as `trigger.json`, so a new automation could parse
  Alertmanager directly — no new software at all. Rejected because it
  only ever solves couplings that END at Home Assistant, the mapping
  would live in HA's automation store instead of git, and S5 (a pinned
  real payload as a regression fixture) becomes impossible. Recorded
  honestly: if this project were only about Alertmanager → HA, this
  alternative would have won.

- **Bento** (MIT fork of Benthos, actively maintained). Covers roughly
  80% of the scope: HTTP in, HTTP out, YAML config in git, small
  footprint. Rejected on two counts — Bloblang would be the second
  template language G4 exists to avoid, and its input → pipeline →
  output model cannot express G7: there is no "do this after the
  output, if the output succeeded", which is exactly what
  ack-after-delivery is. **If G7 is ever dropped, Bento is the first
  thing to re-examine.**

- **Vector** (MPL-2.0, Rust, Datadog). Same two objections, plus it is
  built for telemetry: it batches by default and is tuned for volume,
  not for single alerts that must each be delivered and acknowledged
  individually. Worth remembering for a future Loki/log pipeline.

- **Node-RED** (Apache-2.0). The only bought option that CAN express
  the poll → deliver → ack loop. Rejected on maintainability: its
  config is a coordinate-laden JSON flow that git cannot meaningfully
  diff, the logic lives in a GUI rather than a text file, testing is
  manual, and it is a second automation platform beside Home Assistant.

- **n8n.** Rejected on licence (Sustainable Use Licence, not open
  source) and size (2 GB RAM documented as the minimum, PostgreSQL for
  production webhooks) for a job that amounts to reshaping a message.

- **A throwaway script on the hub.** The option Kenny explicitly
  refused at the start: twenty lines with no tests, no gates and no
  documentation, that breaks quietly in three months on a renamed
  field, and whose second coupling is a second script.

**Decisive finding.** The sharpest filter was not the template language
but G7. Poll-then-acknowledge-only-after-successful-delivery is an
ordering most off-the-shelf pipelines structurally cannot express.
