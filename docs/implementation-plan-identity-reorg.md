# Implementation Plan: Identity + State/Config Reorganization (No Backward Compatibility)

This plan is based on [spec-ish.md](./spec-ish.md) and [migration-plan-identity-state-config.md](./migration-plan-identity-state-config.md), but assumes a **hard schema cut** (no legacy compatibility layers).

## Goal

Restructure identity and separate runtime state from configuration while minimizing complete logic rewrites.
As part of that split, use `WorkloadConfig` as a semantic wrapper around `Enviornment` (`Enviornment_V2`) as the canonical workload configuration model.

## 1. Lock vocabulary first

Finalize and freeze this rename map before code changes:

- `app_id -> workload_id`
- `git_id -> source_id`
- `NodeIdentity.id -> node_id` (within identity-domain structs)
- introduce `runtime_id`
- introduce `authority_id`

Treat this as the contract for all module updates.

## 2. Add new domain types before touching behavior

Create data types first (no behavioral changes yet):

- `identity::{NodeId, WorkloadId, RuntimeId, AuthorityId}`
- `identity::{WorkloadIdentity, RuntimeIdentity, AuthorityIdentity, IdentityContext}`
- `state::RuntimeState`
- `config::WorkloadConfig`
- `state::WorkloadSnapshot`

Reuse existing generation logic:

- workload/source derivation from git hash logic in [git_actions.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/git_actions.rs)
- node identity generation/persistence logic in [identity.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/identity.rs)

## 3. Split identity module by concern (move/rename, don’t rewrite)

Refactor [identity.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/identity.rs) into:

- `identity/node.rs`
- `identity/workload.rs`
- `identity/runtime.rs`
- `identity/authority.rs`

Keep algorithms and crypto unchanged. This pass should be mostly mechanical relocation + naming updates.

## 4. Separate runtime state from config (main structural change)

In [state_persistence.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/state_persistence.rs), split current mixed shape:

- `RuntimeState`: status, pid, timestamps, counters, error log, stdout/stderr
- `WorkloadConfig`: semantic config wrapper around `Enviornment` (`Enviornment_V2`)

Replace [config_bundle.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/config_bundle.rs) `ApplicationConfig` with:

- `WorkloadSnapshot { identity, config, runtime, custom }`

Keep behavior in `update_state`, `log_error`, and `wind_down_state` functionally the same, but retarget fields to the new structures.

Use environment-backed config references in runtime/config paths:

- build v2 config using `Enviornment::new_v2()` + setters + `finalize()`
- store/transmit through `WorkloadConfig`

## 5. Mechanically rename message/payload fields

Update [aggregator.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/aggregator.rs):

- replace overloaded `app_id`/`git_id`
- use identity-domain structs
- attach runtime identity/state explicitly

Update [portal.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/portal.rs) models to the same terminology.

Do this as a dedicated rename/schema pass with no algorithm edits.

## 6. Replace persistence format once

Hard-cut state persistence format:

- remove old mixed `AppState` persistence shape
- persist new snapshot/runtime shape only

Keep encryption layer unchanged in [encryption.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/encryption.rs).

## 7. Rewire process/state call sites last

Update [process_manager.rs](/home/dwhitfield/Developer/Artisan_Hosting/Libraries/artisan_lib/src/process_manager.rs):

- swap from old `AppState` references to `RuntimeState`/snapshot interfaces
- keep process supervision logic (spawn/kill/monitor) unchanged

This contains churn to field access and function signatures, not behavior.

## 8. Stabilization + validation

Fix compile errors in this order:

1. identity modules
2. state/config modules
3. aggregator
4. portal
5. process_manager
6. tests

Add tests for:

- workload ID stability across restart/rebuild
- runtime ID changes per generation
- authority ID/session validity
- new persistence round-trip
- unchanged process monitor behavior

## Logic churn profile

### Mostly move/rename (low risk)

- node identity generation
- git/source ID generation
- encryption/decryption
- process monitor/kill algorithms

### Real rewrites required

- state object model split (`AppState` separation)
- aggregator/portal payload shapes
- state persistence schema

## Recommended execution slices

1. New types/modules added and compiling
2. Identity module split
3. State/config split + `WorkloadConfig` (`Enviornment_V2`) cutover
4. Aggregator + portal schema update
5. Persistence cutover
6. Process-manager call-site rewiring
7. Cleanup/removal of obsolete types/files

## File-level checklist (starter)

- [ ] Add `src/identity/` submodules and re-export from `identity` module entrypoint
- [ ] Introduce ID newtypes and identity context structs
- [ ] Introduce `RuntimeState`, `WorkloadConfig`, `WorkloadSnapshot`
- [ ] Refactor `state_persistence` APIs to new types
- [ ] Refactor `config_bundle` into snapshot-oriented composition
- [ ] Use `WorkloadConfig` (`Enviornment_V2`) builder/finalize flow across runtime call sites
- [ ] Remove `AppConfig`-centric call paths from state/portal/config-bundle modules
- [ ] Rename identity fields in `aggregator` structs and functions
- [ ] Rename payload fields in `portal` structs
- [ ] Update `process_manager` state mutation call signatures
- [ ] Replace persistence schema writes/reads with new shape
- [ ] Update tests to new domain names and state/config split
