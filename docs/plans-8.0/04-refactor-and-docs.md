# 8.0 plan 04: Refactor, doctests and documentation (closing step)

Part of the [artisan_middleware 8.0.0 plan set](README.md). Draft v2, 2026-10-26.

| | |
|---|---|
| **Workstreams** | W11 (restructure the crate, doc comments and doctests, written docs) |
| **When** | **Last.** After plans 01-03 are merged, so the moves do not collide with the rework and the docs describe the final API. The move map (W11.1a) and a `missing_docs` warning pass can start earlier. |
| **Depends on** | [01](01-identity-urn-access.md), [02](02-runtime-usage-and-config.md), [03](03-security-and-cleanup.md) all merged. |
| **Blocks** | The 8.0.0 publish and consumer pin bumps. |
| **Decisions owned** | D19, D20, D21 |

Conventions: `[B]` = breaking API/wire/disk change. `file:line` locations are at commit `7c44282` (before the restructure in [04](04-refactor-and-docs.md) moves files). W-numbers are stable across the plan set; see the [index](README.md).

---

## W11. Closing step: restructure the crate, doctests, and full documentation

**Runs last**, after W1-W10 land, so the move does not collide with the rework and so the docs describe the final API. Every
`file:line` reference elsewhere in this plan is valid at commit `7c44282` (pre-restructure); W11.1 produces a move map so those
references can still be followed.

### Why

`src/` is 26 flat files (plus `api/`, `enviornment/`) with unrelated concerns side by side, and a few files that hold several modules:
`aggregator.rs` (1091 lines: message types + `Status` + metrics + usage accounting + registered-apps persistence),
`portal.rs` (594: node data, API envelopes, project models, billing params), `process_manager.rs` (1076), `identity.rs` (786: typed
ids + node `Identifier` + snowflake + `Action`), `git_actions.rs` (773). Current doc coverage (pub items / `///` lines / doctest fences):
`aggregator` 38/208/2, `identity` 43/241/16, `process_manager` 45/267/2, `portal` 27/213/0, `git_actions` 18/151/0,
`config_bundle` 18/0/0, `historics` 6/0/0, `version` 2/0/0, `lib.rs` 26/0/0, `users` 3/1/0, `timestamp` 5/4/0, `urn` 9/15/0,
`runtime_bundle` 13/26/0, `systemd` 8/10/0. Only about six modules have a `//!` header. `docs/01-06` were summarised from
doc comments and still describe `notifications`; `docs/README.md` has the module map as a flat list.

### W11.1 Restructure (proposed hierarchy; D19 and D20 decide the shape)

```
src/
  lib.rs                 crate docs, feature/platform notes, re-exports (dusa_collection_utils, simple_comms?, apostle_client)
  taxonomy/              the platform's vocabulary; no I/O
    ids.rs               OrganizationId, ProjectId (+ derivation), InstanceId, NodeId, DomainId, VmId, SessionId, UserId, InviteId,
                         EnvironmentId, SecretRef, ais_name/strip_ais_prefix     (from identity.rs)
    urn.rs               Urn, UrnPattern, ResourceType                           (urn.rs)
    access.rs            Action, Role, Claims, TokenType                         (identity.rs Action + api/*)
    status.rs            Status and wire parsing                                 (aggregator.rs)
  node/                  things about the machine
    identifier.rs        node Identifier, snowflake generator                    (identity.rs)
    system.rs            systemd, users/ownership, DNS (network.rs), system_stats (W10.3)
    mtls.rs
  runtime/               running and watching processes
    supervisor/          handle, spawn, stop, liveness, stdio capture            (process_manager.rs, split by concern)
    monitor/             sampler, cgroup + /proc sources, watchdog               (resource_monitor.rs)
    control.rs           ToggleControl                                           (control.rs)
  usage/                 metering and billing inputs
    live.rs, accumulator.rs, record.rs, summary.rs, flush.rs                      (aggregator.rs usage half)
    ledger.rs                                                                      (historics.rs)
  messaging/             manager <-> portal <-> watchdog wire types
    app.rs               Command, AppStatus, AppMessage, Register/Update/...     (aggregator.rs message half)
    portal.rs            NodeInfo, ProjectSummary, ApiResponse, ErrorCode ...    (portal.rs)
  state/                 config and persistence
    runtime_state.rs     RuntimeState + StatePersistence                         (state_persistence.rs, W5.4)
    spec.rs              RuntimeSpec, validate, legacy importers                 (config.rs + enviornment/, W5.2)
    custom_config.rs, runtime_bundle.rs, env_file.rs
  security/
    encryption/          aead.rs (W10.1), self_keyed.rs (W10.4), recs_experimental.rs (private, W10.2)
    rbac_client.rs       (W6.1)
  git/                   git_actions.rs split: credentials, urls, actions
  util/                  timestamp, version, cli
```

- [ ] **W11.1a Move map first.** Write `docs/MODULE_MOVES.md` (old path -> new path, per public item) before moving anything; generate it
  from `cargo public-api` (or a script over `pub` items) so it is exact. It also feeds the migration guide.
- [ ] **W11.1b Do the moves mechanically**, one group per commit (`git mv` + `pub use`), tests green after each: taxonomy, node, runtime,
  usage, messaging, state, security, git, util. No behavior changes in these commits (review rule: diffs are moves and `use` lines only).
- [ ] **W11.1c Old import paths (D19).** Recommended: keep `#[doc(hidden)] pub use` shims at the old paths for the 8.0 series so
  consumers can re-pin first and rewrite imports later; remove in 8.1. Alternative: clean break (every consumer's imports change at once).
- [ ] **W11.1d Spelling (D20).** With W5 the `Enviornment*` types are deleted (legacy V1/V2 survive only as importers inside `spec`, correctly spelled),
  so the remaining spelling fix is the module path/names of anything that still carries it (`enviornment` module, `ApplicationType`'s home).
  `docs/README.md` says the spelling is preserved deliberately; 8.0 is the last cheap chance to fix it. Serde names/on-disk formats must not change.
- [ ] **W11.1e Tests live next to what they test.** `lib.rs` includes 13 files from `src/tests/` through `#[path]` mods; move to colocated
  `#[cfg(test)] mod tests` or a top-level `tests/` directory for the cross-module ones (`taxonomy_ids`, `aggregator` flows).
  `src/tests_broke/` handled in W7.1 before this step.
- [ ] **W11.1f Visibility pass.** Many items are `pub` only because modules were flat. Make internals `pub(crate)`, and decide the
  intended public API per group (what `lib.rs` re-exports, optional `prelude`). Run `cargo public-api diff 7.x..8.0` and review it as
  the release's API changelog.

### W11.2 Doc comments and doctests

- [ ] **Crate and module headers.** `//!` for `lib.rs` (what the crate is, feature/platform matrix: Linux-only modules, `cli` feature,
  relationship to `apostle_client`, how to read IDs/URNs) and every module group above; module header states purpose, invariants,
  failure behavior, and who consumes it.
- [ ] **Every public item documented**, enforced with `#![warn(missing_docs)]` during the work and `#![deny(missing_docs)]` at the end.
  Start with the zero-doc modules (`config_bundle`, `historics`, `version`, `users`, `timestamp`, `urn`, `systemd`, `runtime_bundle`).
  Document error cases, panics (there should be none, W7.4), units (core-seconds, MB, bytes, ms), and Linux-only behavior.
- [ ] **Doctests that run.** Prefer runnable `///` examples over `no_run`; use `ignore` only with a comment saying why. Examples must
  not touch real paths (`/opt/artisan/...`): use `tempfile` or pure functions. Target: every constructor, parser, and pure function has
  one example; async/Linux-only items use `#[cfg(target_os = "linux")]` in the example. Today 7 modules have fences, 2 have them as
  `no_run`; `git_actions` and `portal` have none.
- [ ] **Intra-doc links** instead of backticked names; CI gate `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` (fails on broken links
  and missing docs); `cargo test --doc` in CI (add to `.github/workflows/testing.yml` and `Makefile`).
- [ ] **docs.rs:** add `[package.metadata.docs.rs]` (`all-features`, and `--cfg docsrs` so Linux-only items show with `doc(cfg)`).

### W11.3 Written documentation in `docs/`

Replace the six summary notes with a structure a new contributor can read in order (keep the two plan docs and `spec-ish.md` under `docs/design/`):

```
docs/
  README.md                       what the crate is, install, module map (the W11.1 tree), platform support, versioning policy
  concepts/
    taxonomy-and-ids.md           resources, id formats, project-id derivation (W1), instance ids, URNs; links the vendored docs/platform/architecture/RESOURCE_TAXONOMY.md as source of truth
    wire-formats.md               Status, node ids as decimal strings, bincode vs JSON, on-disk formats and their versions
    usage-and-billing.md          counters, deltas, flush, schema versions, how Billing consumes it (W3)
    security-model.md             keyed AES-GCM vs self-keyed (limits), RBAC types, mTLS, what is NOT protected
  guides/
    supervise-a-process.md        spawn, monitor, stop, re-attach; the Rust process model explained for the people who inherit this code
    report-usage.md               LiveMetrics -> accumulator -> record -> summary, with a worked example and the units
    persist-state-and-config.md   AppState/AppConfig/Environment/runtime bundle
    encrypt-data.md               which function for which job
    send-mail.md                  apostle_client re-export, MailBundle
    git-workflows.md
  reference/                      one page per module group: purpose, public items table, errors, examples (generated tables + prose)
  migration/
    7-to-8.md                     every [B] item in this plan: old -> new, with code (from MODULE_MOVES.md and the changelog)
    MODULE_MOVES.md
  design/
    experimental-recs.md          (W10.2)
    decisions.md                  outcome of D1-D21 with date and rationale
    implementation-plan-identity-reorg.md, migration-plan-identity-state-config.md, spec-ish.md   (moved, status banner on each)
  platform/                       the vendored Platform docs (taxonomy, RBAC, portal API, cited plans) with their drift check; already in place, keep it
  CHANGELOG.md
```

- [ ] **Docs that cannot rot (D21).** Include guide code through `#![doc = include_str!(...)]` / `#[doc = include_str!("../docs/guides/x.md")]` on
  the matching module so their code blocks are compiled by `cargo test --doc`. Do not copy the Platform taxonomy tables by hand into other docs: link to
  the vendored [`RESOURCE_TAXONOMY.md`](../platform/architecture/RESOURCE_TAXONOMY.md) (`docs/platform/`, D21) and test the same facts in code
  (`tests/taxonomy_ids.rs` already pins the vectors).
- [ ] **Runnable `examples/`** matching the guides (supervise + stop a process tree, report usage and summarize, encrypt/decrypt, load a
  `MailBundle`); `cargo build --examples` in CI; replace the dead `examples/test_email.rs` (W8.5).
- [ ] **README.md at the repo root** (crates.io page): purpose, 30-line quick start, link to docs/ and docs.rs, MSRV, license.
- [ ] **Acceptance:** `cargo doc --no-deps` with `-D warnings` clean; `cargo test --doc` passes; `deny(missing_docs)` on; every `docs/` code
  block compiles or is explicitly marked; each `[B]` item in this plan has a line in `migration/7-to-8.md`; reviewer can find any public
  item from `docs/README.md` in two clicks.

---

## Decisions

- D19. After the restructure, keep `#[doc(hidden)] pub use` shims at the old module paths for the 8.0 series (recommended) vs clean break.
- D20. Fix the `enviornment`/`Enviornment_V1` spelling now (recommended, with serde names unchanged) vs leave it.
- D21. (Decided: vendored.) The Platform docs these plans cite are copied into [`docs/platform/`](../platform/README.md). Open: after 8.0, does the in-repo copy become the single source of truth (and `Platform/docs` a mirror), or stay a snapshot with the drift check?

## Verification

- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`, `cargo test --doc`, `cargo build --examples`.
- `cargo public-api diff` against 7.x reviewed and matched line for line to `migration/7-to-8.md`.
- A consumer (Welcome is the smallest) builds against 8.0 using only the old import paths via the shims (if D19 keeps them).
