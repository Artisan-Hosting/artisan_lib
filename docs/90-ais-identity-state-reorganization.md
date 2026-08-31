# AIS Identity and State/Configuration Reorganization - Consolidated Documentation

This consolidated documentation combines information from the implementation plans, migration strategies, identity specifications, and state application concepts to provide a complete overview of the AIS identity and state system reorganization.

## 1. Identity Model Overview

### 1.1 Purpose
The AIS Identity Model defines how the platform names, distinguishes, and tracks:
- machines / nodes
- workloads / managed applications
- individual runtime generations
- the component currently authorized to report or mutate state

Its purpose is to prevent ambiguity as AIS grows from a simple runner pattern into a multi-application system with orchestration, recovery, monitoring, and policy enforcement.

### 1.2 Identity Domains
The AIS model distinguishes four primary identity domains:

#### 1.2.1 Node Identity
Definition: A durable identifier assigned to a node, host, or AIS manager installation.
Purpose: Used to identify the machine or system boundary that owns local resources and runs intermediates.
Characteristics:
- persistent across restarts
- durable on disk
- cryptographically verifiable if needed
- used for registration and trust
- not tied to any one workload

#### 1.2.2 Workload Identity
Definition: A stable identifier for a managed application, project, or deployment target.
Purpose: Represents the logical application slot AIS is managing, regardless of which specific process generation is currently running.
Characteristics:
- stable across rebuilds and restarts
- may be derived from Git identity, config, declared name, or a generated internal ID
- used for policy, billing, routing, and historical tracking
- should not change just because the process was restarted

#### 1.2.3 Runtime Identity
Definition: An ephemeral identifier for one specific execution generation of a workload.
Purpose: Represents one concrete run of the intermediate + client application lifecycle.
Characteristics:
- changes on rebuild/restart/re-initialization
- scoped under a workload
- used to correlate:
  * PID
  * metrics
  * logs
  * heartbeats
  * current snapshot data
- allows stale updates from prior runs to be rejected

#### 1.2.4 Authority Identity
Definition: A bounded identity for the component currently authorized to publish or mutate canonical state for a runtime.
Purpose: Allows AIS to distinguish between:
- the intermediate while healthy
- the lifecycle manager during recovery/takeover
- a future dedicated state service or recovery process
Characteristics:
- ephemeral
- scoped to a runtime or runtime generation
- transferable under explicit rules
- used to prevent split-brain state updates

## 2. Implementation Strategy

### 2.1 Migration Principles
- Keep behavior first, then rename/move types.
- Keep algorithms unchanged in process supervision, monitoring, git actions, encryption, and notifications.
- Add adapters before deleting old fields.
- Keep serde backward compatibility (`alias`, dual-read, dual-write phases).
- Migrate call sites in thin slices, module-by-module.
- Do not combine behavior changes with structural changes in the same PR.

### 2.2 Target Model
Identity domains:
- `node_id`: durable host identity.
- `workload_id`: stable logical app identity across restarts.
- `runtime_id`: one generation of workload execution.
- `authority_id`: actor currently allowed to publish canonical runtime state.

State/config split:
- `WorkloadConfig`: intended/static configuration wrapper around `Enviornment`/`Enviornment_V2`.
- `RuntimeState`: live/ephemeral state.
- `WorkloadSnapshot`: combined transport/persistence view when needed.

## 3. Technical Implementation Details

### 3.1 Type Introduction Strategy (Non-breaking first)
#### 3.1.1 Add newtype IDs (serde transparent)
Add small wrappers around `Stringy` so call sites keep similar ergonomics:
- `NodeId(Stringy)`
- `WorkloadId(Stringy)`
- `RuntimeId(Stringy)`
- `AuthorityId(Stringy)`

Implement:
- `Clone`, `Eq`, `Ord`, `Hash`, `Display`, `Serialize`, `Deserialize`
- `From<Stringy>`, `From<&str>`, `AsRef<str>`

#### 3.1.2 Add identity envelope structs
- `WorkloadIdentity { workload_id, source_id, node_id }`
- `RuntimeIdentity { runtime_id, workload_id, generation, started_at, ended_at }`
- `AuthorityIdentity { authority_id, kind, runtime_id, granted_at, expires_at }`
- `IdentityContext { node, workload, runtime, authority }`

### 3.2 Wire and persistence compatibility plan
#### 3.2.1 `aggregator::AppStatus` transition
Current:
- `app_id`, `git_id`, `app_data`, `metrics`, `timestamp`, `expected_status`.

Transition shape:
- keep current fields
- add `identity: Option<IdentityContext>`
- add `runtime: Option<RuntimeState>` or `runtime_ref` in first pass

Serde compatibility:
- use `#[serde(alias = "app_id")]` on `workload_id` when finalizing
- preserve ability to deserialize old payloads

#### 3.2.2 State file transition
Current state file writes encrypted TOML of `AppState`.
Plan:
- `v1` reader: existing `AppState` format
- `v2` reader/writer: `RuntimeState` + config reference/snapshot
- release N: write v1+v2 (dual write) behind flag
- release N+1: default v2 write, still read v1
- release N+2: remove v1 write, keep v1 read for one more cycle

### 3.3 State/Config separation plan
#### 3.3.1 Introduce new types
- `RuntimeState` from runtime-only fields currently in `AppState`:
  - status, pid, last_updated, started_at, event_counter, error_log, stdout, stderr
- `WorkloadConfig` from `Enviornment`/`Enviornment_V2` (+ custom where applicable)
- `WorkloadRuntimeBundle` (temporary adapter), replacing current implicit coupling

#### 3.3.2 Keep old API surface with adapters
In `state_persistence`:
- keep `AppState` for now
- add conversion:
  - `impl From<AppState> for RuntimeState`
  - `impl RuntimeState { fn with_config(self, config: WorkloadConfig) -> AppState }`

## 4. PR Sequencing (Recommended)

### PR 1: Foundation types only
- Add identity newtypes and context structs.
- No behavior changes.
- Add unit tests for serde/display/hash/equality.

### PR 2: Add runtime/authority fields to messages
- Add optional identity context fields to `AppStatus`, messages, and portal models.
- Keep old fields; add conversion helpers.

### PR 3: Introduce `RuntimeState` and conversion shims
- Add new state types and `From` conversions.
- Keep `AppState` as compatibility wrapper.

### PR 4: Persistence v2 + dual read/write
- Add versioned state persistence format.
- Keep old readers active.

### PR 5: Update internal call sites module-by-module
- `process_manager` and `aggregator` consume new identity/state shapes via adapters.
- No logic changes in kill/monitor/update behavior.

### PR 6: Flip defaults
- Default new wire/persistence shape.
- Old shape still readable.

### PR 7: Remove old fields
- Remove `app_id`/`git_id` only after consumers are migrated.
- Remove temporary `AppConfig` conversion adapters once no paths depend on them.

## 5. Core Behaviors

### 5.1 Publish live snapshot
Called by the intermediate while healthy and active.
Behavior:
1. Validate `snapshot.identity.authority.is_valid_now()` and that the authority `runtime_id` matches the snapshot runtime.
2. Persist `current_snapshots` row with serialized `WorkloadSnapshot`.
3. Update indexed fields (`status`, `pid`, `last_update_at`, `event_counter`).
4. Append notable events when thresholds/flags change.
5. Optionally mirror into existing aggregator status (`AppStatus`) for compatibility.

### 5.2 Heartbeat
Cheap periodic liveness extension.
Behavior:
- Update `authority_leases.last_heartbeat_at` only.
- Optionally reject heartbeats from stale/non-current `authority_id`.

### 5.3 Append event
Append-only journal write used by intermediate, manager, and recovery components.

### 5.4 Takeover if stale
Used when the intermediate stops heartbeating.
Behavior:
1. Determine staleness using `last_heartbeat_at` + threshold (seconds).
2. Mint a new `AuthorityIdentity` with `AuthorityKind::Recovery`.
3. Mark runtime state to terminal or warning based on policy (`Status::Warning` vs `Status::Stopped`).
4. Append a `StateEvent` with the reason and prior authority details.

## 6. Security Design

### 6.1 IPC access control
Unix socket permissions plus peer credential verification where possible.

### 6.2 Limited key surface
Prefer a model where only the state service encrypts/decrypts durable snapshot blobs.

### 6.3 Encrypted persistence
Use `encryption::{simple_encrypt, simple_decrypt}` for sealed snapshots and sensitive fields. Keep operational metadata queryable without decryption.

### 6.4 Integrity and attribution
Every event and mutation is attributed via `AuthorityIdentity`.

### 6.5 Crash-safe writes
Use SQLite transactions and WAL mode. On boot, reconcile stale leases before accepting writes.