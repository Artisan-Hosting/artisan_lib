# Identity Platform (Reorg) Guide

This document describes the new identity platform introduced by the identity reorganization work.

It is focused on the types and contracts under `src/identity/`, and how those identities are intended to be used by state/config, process supervision, and messaging layers.

## Why this exists

Historically, identity concepts were overloaded into a few fields (`app_id`, `git_id`, and legacy node fields).

The new model separates identity into explicit domains so the system can always answer:

- where did this event/state originate?
- which workload is this about?
- which runtime generation is this for?
- which actor currently has authority to publish canonical state?

## Frozen vocabulary contract

The migration keeps a frozen rename map in code as `identity::IDENTITY_RENAME_MAP`:

- `app_id -> workload_id`
- `git_id -> source_id`
- `NodeIdentity.id -> node_id`
- `runtime_id` (introduced)
- `authority_id` (introduced)

This map is a migration contract, not just documentation.

## Identity domains

## 1) Node domain (`identity::node`)

Purpose: durable host identity and persistence.

Main exports:

- `Identifier`: persisted/verifiable node identifier payload
- `NodeId`: domain newtype for node identity
- `SnowflakeIDGenerator`: snowflake-like 64-bit ID generator

Key behavior:

- persisted at `IDENTITYPATHSTR` (`/opt/artisan/.identity`)
- signature verification via hash truncation (`HASH_LENGTH`)
- node ID generation/persistence behavior intentionally unchanged

Use this when you need to identify the machine/manager boundary.

## 2) Workload domain (`identity::workload`)

Purpose: stable logical identity of a managed application/workload.

Main exports:

- `WorkloadId`: stable workload key
- `WorkloadIdentity { node_id, workload_id, source_id }`

Key behavior:

- `WorkloadId::from_git_auth` and `WorkloadIdentity::from_git_auth` reuse existing git-hash derivation logic
- `source_id` captures source-level identity (currently git-derived)

Use this when you need restart-stable identity for routing, policy, and history.

## 3) Runtime domain (`identity::runtime`)

Purpose: identity of a single runtime generation of a workload.

Main exports:

- `RuntimeId`
- `RuntimeIdentity { node_id, workload_id, runtime_id, generation, started_at, ended_at }`

Key behavior:

- `RuntimeId::generate` reuses existing identifier generation internals
- generation-specific metadata (`generation`, `started_at`, `ended_at`) makes stale update detection possible

Use this when correlating logs/metrics/state to one concrete run.

## 4) Authority domain (`identity::authority`)

Purpose: explicit authority for canonical runtime state updates.

Main exports:

- `AuthorityId`
- `AuthorityKind` (`Intermediate`, `Manager`, `Recovery`, `Custom`)
- `AuthorityIdentity { authority_id, runtime_id, kind, granted_at, expires_at }`

Key behavior:

- allows bounded authority sessions via `expires_at`
- ties authority to a specific `runtime_id`

Use this to prevent split-brain state publishing.

## Aggregated context (`identity::IdentityContext`)

`IdentityContext` bundles all four domains for snapshot/payload use:

- `node_id`
- `workload`
- `runtime`
- `authority`

This is the preferred identity envelope for cross-module transport and persistence composition.

## Module map

Current identity module layout:

- `src/identity/mod.rs` (domain root + re-exports + `IdentityContext`)
- `src/identity/node.rs`
- `src/identity/workload.rs`
- `src/identity/runtime.rs`
- `src/identity/authority.rs`

## Generation + lifecycle expectations

- `NodeId`: durable, persists across restarts.
- `WorkloadId`: stable per logical workload.
- `RuntimeId`: rotates per runtime generation.
- `AuthorityId`: rotates per authority grant/session.

High-level lifecycle:

1. resolve/load node identity (`NodeId`)
2. resolve stable workload identity (`WorkloadId` + `source_id`)
3. create runtime generation identity (`RuntimeIdentity`)
4. grant authority (`AuthorityIdentity`)
5. attach all of the above to payloads/snapshots via `IdentityContext`

## Invariants to preserve

- Node identity outlives any workload/runtime.
- Workload identity remains stable across restarts/rebuilds.
- Runtime identity is unique per generation.
- At most one authority is canonical for a given active runtime.
- Stale authority/runtime updates should be rejectable by higher layers.

## Practical usage snippets

```rust,no_run
use artisan_middleware::identity::{
    AuthorityIdentity, AuthorityKind, IdentityContext, NodeId, RuntimeIdentity, WorkloadId,
    WorkloadIdentity,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_id = NodeId::generate().await?;
    let workload_id = WorkloadId::new("billing-api");

    let workload = WorkloadIdentity::new(node_id, workload_id.clone(), "source-abc".into());
    let runtime = RuntimeIdentity::generate(node_id, workload_id, 1).await?;
    let authority = AuthorityIdentity::generate(runtime.runtime_id, AuthorityKind::Manager, None).await?;

    let _context = IdentityContext::new(node_id, workload, runtime, authority);
    Ok(())
}
```

Git-derived workload identity:

```rust,no_run
use artisan_middleware::git_actions::{GitAuth, GitServer};
use artisan_middleware::identity::{NodeId, WorkloadIdentity};
use dusa_collection_utils::core::types::stringy::Stringy;

fn example(auth: &GitAuth) {
    let node_id = NodeId(42);
    let workload = WorkloadIdentity::from_git_auth(node_id, auth);
    let _ = (workload.workload_id, workload.source_id);
}
```

## Migration status note

The identity platform types are now split by concern and available for adoption.

Some call sites in aggregator/portal/process/state still use legacy naming and will be migrated in later steps of `docs/implementation-plan-identity-reorg.md`.
