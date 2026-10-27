# artisan_middleware 8.0.0: plan set

Draft v2, 2026-10-26. Split from the single plan `docs/my-goal-is-for-synthetic-avalanche.md` (left in place; it is the
unsplit original and will drift once these are edited, so delete it when you are happy with the split). Work-item numbers
(`W1`..`W11`, `D1`..`D25`) are unchanged so references still resolve.

## Context

8.0.0 is already the version in `Cargo.toml` (bumped in `7c44282` for a stopgap CPU fix). It is a deliberate breaking
release. Three things drive it:

1. **Docs already promise it.** [`platform/api/PORTAL_API.md`](../platform/api/PORTAL_API.md) §6 #11/#13 and `portal/src/api/handler/projects.rs:13-18`
   defer the cross-org project-id fix to "v8.0.0": include the organization in the id, "in the shared `artisan_middleware`
   derivation and every node-side path built from it".
2. **Billing numbers are wrong** (egress, CPU, RAM, tree measurement) and Billing's overage code is built on them.
3. **Process supervision/monitoring** was written before the team understood Rust's process model; the kill path, liveness
   and sampling all have real bugs. User is fine reworking it almost entirely.

**Source of truth for IDs/URNs:** [`platform/architecture/RESOURCE_TAXONOMY.md`](../platform/architecture/RESOURCE_TAXONOMY.md) (normative: "where code disagrees,
the code is wrong") and [`platform/rbac/RBAC_IMPLEMENTATION_OVERVIEW.md`](../platform/rbac/RBAC_IMPLEMENTATION_OVERVIEW.md). Rules from those docs this plan follows:
typed ids stay the wire form (proto `string`/`uint64`, JSON decimal string for u64); URN is the canonical *string form*
of a resource, not a replacement for the `(ResourceType, id)` pair that `evaluate_access` takes; `Runner`/`Repo` are not
resource types; `ais_` is a process-naming prefix only (`ais_name()`/`strip_ais_prefix()`); fail closed; pre-GA clean
breaks are fine but "half-migrated is not" ([`platform/notes/agent_eng.md`](../platform/notes/agent_eng.md)).

Every Platform doc these plans cite is copied into [`docs/platform/`](../platform/README.md) (13 files, with source paths, hashes and a drift check); all links below point at those copies.

**Crate:** `Libraries/artisan_lib` publishes `artisan_middleware`. Consumers pin crates.io, so every item ends in
publish + pin bump. Never commit a path override.

Conventions: `[B]` = breaking API/wire/disk change. `file:line` locations are at commit `7c44282` (before the restructure in [04](04-refactor-and-docs.md) moves files). W-numbers are stable across the plan set; see the [index](README.md).

## The plans

| Plan | Workstreams | One line |
|---|---|---|
| [01 Identity, URNs and access](01-identity-urn-access.md) | W1, W2, W6 | Org-scoped project ids (the headline 8.0 change), typed ids and validated URNs, retyped wire messages, `instance_id`, RBAC client pieces |
| [02 Runtime, usage and config/state](02-runtime-usage-and-config.md) | W3, W4, W5 | Counter-based billing inputs and the egress fix, a rebuilt process supervisor and sampler, `RuntimeSpec` / overrides partition / `RuntimeState` replacing `AppConfig`, `Enviornment*` and `AppState` |
| [03 Security and cleanup](03-security-and-cleanup.md) | W7, W8, W9, W10 | Keyed AES-GCM API, honest `self_keyed_*` rename, RECS kept private and experimental, `apostle_client` re-export, FIXME and dependency cleanup, hygiene |
| [04 Refactor, doctests and docs](04-refactor-and-docs.md) | W11 | Closing step: group the crate into a hierarchy, document every public item with running doctests, rewrite `docs/` |

## Suggested order across plans

1. **[02] W3.4** (egress sum) as a patch release now: a one-function bug that misbills today.
2. **[01]** spec docs first: W1.5, W2.4, W2.10 (the taxonomy is the source of truth; code follows).
3. **[01]** W2.1-W2.3 and W1.1-W1.2 (ids, URN, derivation), then publish `8.0.0-rc.1` so consumers can start compiling.
4. **[02]** W4 (supervisor) with its tests first, then W3.1-W3.3 and W3.5-W3.7 on top of the W4.4 counters.
5. **[01] + [02]** W2.5-W2.9 message retyping together with W5 (one wire break, not three).
6. **[03]** W10 (encryption) in parallel from the start; W10.2 early, W10.4 renames only after W10.1.
7. **[03]** W8 any time after your `apostle_client` commit is published.
8. **[01]** W6 and **[03]** W7/W9, publish 8.0.0, then consumer pin bumps in this order: `ais_proto`, `ais_auth`, Billing, portal / watchdog / Manager / GitMonitor, the rest.
9. **[04]** W11 last.

## Decisions

Each plan lists the decisions it owns, recommendation first. Index:

| # | Decision | Plan |
|---|---|---|
| D1 | Project id width: keep 8 hex (recommended; ais_auth claim arbitrates collisions) vs widen to 12/16 (breaks every path). | [01-identity-urn-access](01-identity-urn-access.md) |
| D2 | `EnvironmentId` grammar (lowercase slug `[a-z0-9-]{1,32}` recommended) and whether it is project-scoped in URNs | [01-identity-urn-access](01-identity-urn-access.md) |
| D3 | (Decided: `app_name` is dropped.) `project_id` is the key everywhere; optional `display_name` label only. Remaining check: watchdog's use (`watchdo... | [01-identity-urn-access](01-identity-urn-access.md) |
| D4 | Reorg vocabulary: adopt taxonomy names (recommended) vs amend taxonomy. | [01-identity-urn-access](01-identity-urn-access.md) |
| D5 | Supervisor death: should apps survive a watchdog crash (re-attach design, recommended) or die with it | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D6 | cgroup v2 guaranteed on every node? If yes drop the /proc subtree fallback in 8.1. | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D7 | Memory billing basis: cgroup `memory.current` (recommended) vs summed RSS (double-counts shared pages). | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D8 | Grandfathered v1/v2 usage records: exclude (current) vs re-rate. | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D9 | Hard cut vs dual-read for `GitAuth` JSON and `UsageRecord` on disk (7.x files must load or must be migrated). | [README](README.md) |
| D10 | Module name for the mail client: only `artisan_middleware::apostle_client` (recommended, clean break) vs also an alias `notifications`. | [03-security-and-cleanup](03-security-and-cleanup.md) |
| D11 | Keep the crate-root `pub use simple_comms` or reach it only through `apostle_client`. | [03-security-and-cleanup](03-security-and-cleanup.md) |
| D12 | `InstanceId` lifetime: persisted per (project, node) across restarts (recommended, keeps the billing key stable) vs new per spawn. | [01-identity-urn-access](01-identity-urn-access.md) |
| D13 | (Resolved into W5: `AppConfig` is replaced by `RuntimeSpec`; remaining questions are D22-D25.) The blocker list is the consumer migration in W5.5. | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D14 | Super-role usage: audit-log every action, only sensitive ones (current RBAC rule), and/or notify via Apostle. | [01-identity-urn-access](01-identity-urn-access.md) |
| D15 | Names for the renamed `simple_*`: `self_keyed_encrypt/decrypt` (recommended, describes the property) vs `obfuscate/deobfuscate` (blunter). | [03-security-and-cleanup](03-security-and-cleanup.md) |
| D16 | Where does the key for the keyed AES-GCM API come from: a root-only node key file (`/opt/artisan/...`, 0600), derived from the | [03-security-and-cleanup](03-security-and-cleanup.md) |
| D17 | `get_system_stats`: keep a re-export at `resource_monitor::get_system_stats` for one release, or update Welcome in the same release. | [03-security-and-cleanup](03-security-and-cleanup.md) |
| D18 | RECS write-up: what was it for, and what should "repurposing" target? (Needed for `docs/experimental-recs.md`.) | [03-security-and-cleanup](03-security-and-cleanup.md) |
| D19 | After the restructure, keep `#[doc(hidden)] pub use` shims at the old module paths for the 8.0 series (recommended) vs clean break. | [04-refactor-and-docs](04-refactor-and-docs.md) |
| D20 | Fix the `enviornment`/`Enviornment_V1` spelling now (recommended, with serde names unchanged) vs leave it. | [04-refactor-and-docs](04-refactor-and-docs.md) |
| D21 | (Decided: vendored.) Cited Platform docs are copied into `docs/platform/`. Open: does the in-repo copy become the single source after 8.0, or stay a snapshot? | [04-refactor-and-docs](04-refactor-and-docs.md) |
| D22 | (Decided: an overrides partition inside the bundle.) Remaining choice: storage backing A (reserved path prefix, recommended, no `acai_core` change)... | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D23 | (Reduced: `max_cpu_usage` is dropped.) The new optional `limits.cpu_millicores` is plan-derived; confirm the plan catalog stores CPU in a unit that... | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D24 | (Decided: yes.) System apps (Manager, GitMonitor, aggregator, Security, Kaelum) move onto the same bundle + `RuntimeSpec`; their service-specific s... | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |
| D25 | Persist app logs at all across restarts, or only in the watchdog ledger? | [02-runtime-usage-and-config](02-runtime-usage-and-config.md) |

D9 (hard cut vs dual-read for `GitAuth` JSON and `UsageRecord` on disk) spans plans 01 and 02: pick one policy for every 7.x file that must load or be migrated.
Decided so far: D3 (`app_name` dropped), D13 (`AppConfig` replaced by `RuntimeSpec`), D21 (Platform docs vendored into `docs/platform/`), D22 (overrides partition inside the bundle, backing storage still open), D24 (system apps join the bundle path).

## Consumer ripple summary (what each repo must do after publish)

| Repo | Work |
|---|---|
| `ais_proto` (not stood up yet, [taxonomy §11](../platform/architecture/RESOURCE_TAXONOMY.md)) | Needed so `accounts.proto`/`watchdog.proto` changes (project/instance ids, org-scoped claim) are made once |
| `ais_auth` | Claim RPC returns/stores org-scoped `project_id`; `resolve_resource_org` on typed ids; `whoami` `organization_id`; migration additive-safe |
| `portal` | Stop refusing second org; `LiveMetrics` mapping; usage routes; use library `ResourceType`/`Action`; `projects.rs` known-limit comment removed |
| `Billing` | `PoolUsage` from new summary; v3 records; per-subscription sums |
| `watchdog` | New supervisor API; cgroup counters; ledger schema (counters not %), `inventory.rs:264` |
| `Manager`, `GitMonitor` | `GitAuth` new fields; `generate_id()` call sites; node-side data migration |
| `runners/generic_runner` | New supervisor API; `CD-*` tasks in [`per-app-2026-10`](../platform/plans/per-app-2026-10/README.md) overlap here |
| `dashboard`, `artisan_cli` | Project/instance naming; node ids as strings |
| `domain_management`, `ais_secretserver`, `RunpodManager`, `ais_vm`, `keystore`, `project_Kaelum` | Re-pin 8.0.0; typed-id compile fixes; no behavior change expected |
| `Platform/Welcome` | `get_system_stats` import path if it moves (W10.3) |
| `Platform/ServerTest`, `Software/website_monitor` | Replace `artisan_middleware::notifications` with `apostle_client` (W8); provision a `MailBundle` |

## Cross-plan verification

- Library: `cargo test -- --test-threads=1`, `cargo clippy --all-targets`.
- Integration (path override locally, never committed): build `ais_auth`, `portal`, `Billing`, `watchdog`, `Manager`, `GitMonitor`, `generic_runner` against the local crate; run their test suites; then a throwaway node smoke test: deploy as org A and org B the same repo and branch, both succeed with different ids, usage for both shows up in a Billing summary.
- Per-plan checks are at the end of each plan.

## Not inspected

`identity.rs` 1-400 (snowflake generator, node `Identifier`), `encryption.rs`, `control.rs`, `systemd.rs`,
`network.rs`, `custom_config.rs`; `apostle_client` beyond its `Email`/`lib.rs` surface (its `MailBundle` format, whether v1.1.2 is published to crates.io, the uncommitted `test.conf`); watchdog `main.rs` supervise loop beyond call sites; runners' `child.rs`; Billing invoice/rollover code;
`ais_proto` (stub); Manager and GitMonitor internals; the pricing workbook. Which nodes run cgroup v2.
