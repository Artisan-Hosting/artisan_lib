# AIS Identity + State/Config Reorganization Migration Plan

This plan is based on [spec-ish.md](./spec-ish.md) and the current library shape.

Goal: introduce explicit identity domains and separate runtime state from configuration with minimal logic churn.

## 1. Current pain points in this codebase

- `app_id` is overloaded across command routing, app status, and persistence in [`src/aggregator.rs`](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/aggregator.rs).
- `AppState` includes both runtime state and static config in [`src/state_persistence.rs`](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/state_persistence.rs).
- `ApplicationConfig` duplicates config ownership while also storing `AppState` in [`src/config_bundle.rs`](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/config_bundle.rs).
- Identity is currently mostly node-scoped (`NodeIdentity`) in [`src/identity.rs`](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/identity.rs); runtime/authority identity is missing.

## 2. Migration principles (to minimize churn)

- Keep behavior first, then rename/move types.
- Keep algorithms unchanged in process supervision, monitoring, git actions, encryption, and notifications.
- Add adapters before deleting old fields.
- Keep serde backward compatibility (`alias`, dual-read, dual-write phases).
- Migrate call sites in thin slices, module-by-module.
- Do not combine behavior changes with structural changes in the same PR.

## 3. Target model

Identity domains:

- `node_id`: durable host identity.
- `workload_id`: stable logical app identity across restarts.
- `runtime_id`: one generation of workload execution.
- `authority_id`: actor currently allowed to publish canonical runtime state.

State/config split:

- `WorkloadConfig`: intended/static configuration.
- `RuntimeState`: live/ephemeral state.
- `WorkloadSnapshot`: combined transport/persistence view when needed.

## 4. Proposed module layout

```text
src/
  identity/
    mod.rs
    node.rs
    workload.rs
    runtime.rs
    authority.rs
    legacy.rs          # conversion shims from current fields
  state/
    mod.rs
    runtime.rs         # RuntimeState
    snapshot.rs        # WorkloadSnapshot
    persistence.rs
  config/
    mod.rs             # optional later split from config.rs
    workload.rs        # WorkloadConfig + loaders/validation
```

If you want lowest churn in this repo, keep existing file names initially and only add submodules first:

- Keep `identity.rs`, `state_persistence.rs`, `config.rs`, `config_bundle.rs`.
- Re-export new types from old modules.
- Move internals later after migration is stable.

## 5. Type introduction strategy (non-breaking first)

## 5.1 Add newtype IDs (serde transparent)

Add small wrappers around `Stringy` so call sites keep similar ergonomics:

- `NodeId(Stringy)`
- `WorkloadId(Stringy)`
- `RuntimeId(Stringy)`
- `AuthorityId(Stringy)`

Implement:

- `Clone`, `Eq`, `Ord`, `Hash`, `Display`, `Serialize`, `Deserialize`
- `From<Stringy>`, `From<&str>`, `AsRef<str>`

Why this minimizes churn:

- existing map keys and display/logging continue to work
- strong typing can be introduced without rewriting logic

## 5.2 Add identity envelope structs

- `WorkloadIdentity { workload_id, source_id, node_id }`
- `RuntimeIdentity { runtime_id, workload_id, generation, started_at, ended_at }`
- `AuthorityIdentity { authority_id, kind, runtime_id, granted_at, expires_at }`
- `IdentityContext { node, workload, runtime, authority }`

Keep old fields during transition:

- `app_id`, `git_id` remain temporarily
- new identity context added as optional fields

## 6. Wire and persistence compatibility plan

## 6.1 `aggregator::AppStatus` transition

Current:

- `app_id`, `git_id`, `app_data`, `metrics`, `timestamp`, `expected_status`.

Transition shape:

- keep current fields
- add `identity: Option<IdentityContext>`
- add `runtime: Option<RuntimeState>` or `runtime_ref` in first pass

Serde compatibility:

- use `#[serde(alias = "app_id")]` on `workload_id` when finalizing
- preserve ability to deserialize old payloads

## 6.2 state file transition

Current state file writes encrypted TOML of `AppState`.

Plan:

- `v1` reader: existing `AppState` format
- `v2` reader/writer: `RuntimeState` + config reference/snapshot
- release N: write v1+v2 (dual write) behind flag
- release N+1: default v2 write, still read v1
- release N+2: remove v1 write, keep v1 read for one more cycle

## 7. State/config separation plan

## 7.1 Introduce new types

- `RuntimeState` from runtime-only fields currently in `AppState`:
  - status, pid, last_updated, started_at, event_counter, error_log, stdout, stderr
- `WorkloadConfig` from `AppConfig` (+ `enviornment`/custom where applicable)
- `WorkloadRuntimeBundle` (temporary adapter), replacing current implicit coupling

## 7.2 Keep old API surface with adapters

In `state_persistence`:

- keep `AppState` for now
- add conversion:
  - `impl From<AppState> for RuntimeState`
  - `impl RuntimeState { fn with_config(self, config: AppConfig) -> AppState }`

This lets existing process_manager code continue to call:

- `update_state(...)`
- `log_error(...)`
- `wind_down_state(...)`

without changing internal logic immediately.

## 8. Identity generation plan

## 8.1 Node identity

Keep current `NodeIdentity` generation/persistence logic untouched initially.

## 8.2 Workload identity

Use existing git-derived identity as seed (`generate_git_project_id`) and increase hash length in controlled step (8 -> 12 or 20 as noted in spec-ish).

## 8.3 Runtime identity

Introduce deterministic format:

- `<epoch_ms>-<short_workload>-<rand_hex>`

Generated at process spawn/start generation boundaries only.

## 8.4 Authority identity

Derive from runtime session actor:

- `<runtime_id>-<actor_kind>-<nonce>`

Use heartbeat expiry to reject stale updates.

## 9. PR sequencing (recommended)

## PR 1: Foundation types only

- Add identity newtypes and context structs.
- No behavior changes.
- Add unit tests for serde/display/hash/equality.

## PR 2: Add runtime/authority fields to messages

- Add optional identity context fields to `AppStatus`, messages, and portal models.
- Keep old fields; add conversion helpers.

## PR 3: Introduce `RuntimeState` and conversion shims

- Add new state types and `From` conversions.
- Keep `AppState` as compatibility wrapper.

## PR 4: Persistence v2 + dual read/write

- Add versioned state persistence format.
- Keep old readers active.

## PR 5: Update internal call sites module-by-module

- `process_manager` and `aggregator` consume new identity/state shapes via adapters.
- No logic changes in kill/monitor/update behavior.

## PR 6: Flip defaults

- Default new wire/persistence shape.
- Old shape still readable.

## PR 7: Remove old fields

- Remove `app_id`/`git_id` only after consumers are migrated.
- Remove `AppState.config` once no paths depend on it.

## 10. Specific low-churn mapping guide

- `app_id` -> `workload_id`
- `git_id` -> `source_id`
- `AppState` -> `RuntimeState` (+ adapter)
- `ApplicationConfig` -> split into `WorkloadConfig` + runtime snapshot wrapper
- `NodeIdentity` remains as node domain type, moved under `identity::node`

## 11. Guardrails to avoid churn spikes

- Use compile-time deprecation markers first, not immediate deletions.
- Add `legacy` conversion module and keep it until final cleanup release.
- Keep old integration tests running during transition.
- Add snapshot tests for old/new serialized payloads.
- Freeze non-migration feature work during PR 3-6.

## 12. Validation checklist before cutover

- old payloads deserialize into new internal models
- stale runtime updates are rejected via runtime/authority checks
- restart creates new `runtime_id` and new authority
- workload identity remains stable across restart/rebuild
- no change in process monitor behavior
- no change in state error logging behavior

## 13. Suggested deprecation window

- Release N: introduce new types and optional fields, dual read/write
- Release N+1: default new formats, old reads retained
- Release N+2: remove old writes and old fields, retain explicit migration notes

## 14. Immediate implementation tasks in this repo

1. Add `identity::types` (newtype IDs + context structs) with no call-site usage.
2. Add `RuntimeState` type and conversion to/from current `AppState`.
3. Add optional identity context to `aggregator::AppStatus`.
4. Add versioned state file header for persistence format.
5. Add compatibility tests for v1 and v2 state payloads.

This path keeps logic churn low because almost all first-pass changes are type-layer and serialization-layer changes, not behavior-layer rewrites.
