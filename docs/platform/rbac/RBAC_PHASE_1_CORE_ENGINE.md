# RBAC Phase 1 — Core Policy Engine in `ais_auth`

Read `RBAC_IMPLEMENTATION_OVERVIEW.md` in this same directory first for shared vocabulary (`Action`, `ResourceType`, the elevated-token pattern, non-goals). This doc assumes it.

**Goal:** replace `ais_auth`'s hardcoded, Project-only `has_permission()` with a data-driven, resource-generic `evaluate_access()`, without breaking any existing caller. This is the foundation every other phase builds on — nothing else in this project can start until this lands.

**Repos touched:** `ais_auth`, `Libraries/artisan_lib`.

## Status (as of this revision)

**Done, verified compiling and passing (`cargo build`/`cargo test` clean on both crates):**
- §3: `Action` and `ResourceType` (`ResourceType` reused from the pre-existing `urn.rs`, made `pub` + serde-derived rather than duplicated), each with a round-trip + fail-closed-parsing test — `Libraries/artisan_lib/src/identity.rs` / `src/urn.rs`.
- §2: the `rbac_policies` / `resource_grants` / `access_audit_log` / `service_credentials` schema — `ais_auth/migrations/0005_rbac_policies.sql`, including the GLOBAL-seed matrix (90 rows, programmatically verified against the exact `has_org_permission` matrix it must reproduce).
- §4: `evaluate_access`, its `has_permission` compat wrapper, and `resolve_resource_org` (Project/Instance/Secret cases only — Domain/Session/Vm remain `None`-stubbed with `TODO(phase N)` comments for their respective phases) — `ais_auth/src/auth/permissions.rs`.
- §5: all 8 new RPCs (`EvaluateAccess`, `GrantResourcePermission`, `RevokeResourcePermission`, `ListResourceGrants`, `SetOrgPolicy`, `ListOrgPolicies`, `IssueServiceCredential`, `ValidateServiceCredential`) — proto messages in `accounts.proto`, `AuthService` methods in `service.rs` (same elevated-token / Super-vs-org-scoped-Admin pattern as every existing Phase F/G/H RPC; grant/revoke/list additionally check `permissions::resolve_resource_org` so a non-Super Admin can only touch resources verified to belong to their own org), and `grpc.rs` trait implementations. Also added `revoke_service_credential` in the DB layer (not wired to an RPC yet — no `RevokeServiceCredential` proto method was in the original spec; flagging here rather than silently adding scope).
- The full DB layer backing all of the above in `database/helpers.rs`: `policy_allows`, `resource_grant_exists`, `write_audit_log`, `insert_resource_grant`, `delete_resource_grant`, `list_resource_grants`, `upsert_org_policy`, `list_org_policies`, `create_service_credential`, `validate_service_credential`, `revoke_service_credential`.
- §6: the short-TTL (90s) `RbacDecisionCache` — `database/caching.rs` (type + cleaning loop), `global.rs` (`GLOBAL_STATE` field/getter), wired into `policy_allows`/`resource_grant_exists`, cleaning loop spawned in `main.rs` alongside the existing permission/org-id cache cleaners.
- §8 small fixes: `refresh_tokens` now re-fetches role/org fresh from `users` on every refresh; the dead `with_role` helper (and its now-unused `Role`/`Forbidden` imports) removed from `ais_auth/src/api/helpers.rs`.

**Not done, and why:**
- The `project_name`→`project_id` proto field rename — `PermissionRequest` is vendored into `portal` and `domain_management` too (per the taxonomy migration's known `ais_proto` gap), so this rename is a cross-repo change outside this phase's stated `ais_auth`/`artisan_lib` scope. Left as-is; tracked as a `RESOURCE_TAXONOMY.md` §11 follow-up, not silently dropped.
- §9's DB-backed regression tests (the GLOBAL-seed-matches-`has_org_permission` pin, `evaluate_access` fail-closed tests, cache-TTL test, audit-log test, service-credential round-trip test) — **this dev sandbox has no live MySQL instance available** (attempting to provision one hit a sandbox permission wall), and `ais_auth` has never had any DB-backed test in its history to build on. What's actually covered instead: pure-logic round-trip/fail-closed-parsing tests for `Action`/`ResourceType` (no DB needed), plus the GLOBAL-seed migration's 90-row matrix was verified programmatically (a Python script confirmed 0 duplicates and an exact match to the required role×resource_type×action shape) rather than via a real migration dry-run. Whoever has access to a real MySQL instance should still run the DB-backed tests this doc originally called for before considering Phase 1 fully proven.

**Blocking any commit right now:** `ais_auth/Cargo.toml` still points `artisan_middleware` at the local path (`{ path = "../../Libraries/artisan_lib" }`), not the registry — required since `Action`/`ResourceType` only exist in the local, unpublished copy of `artisan_lib`. Per the standing workflow rule this **must not be committed as-is**. This needs a decision from the platform owner: bump-and-publish `artisan_middleware` now so this can be committed, or hold it uncommitted until a DB is available to run the tests above first.

## 1. Background — what exists today

Read these in full before changing anything:

- `Libraries/artisan_lib/src/api/roles.rs` — the `Role` enum (`Super, Admin, Controller, Viewer, Audit, None`) and `has_org_permission(current_role, required_role) -> bool`, a hardcoded match statement this project replaces the *behavior* of (keep the function itself for now — nothing calls it directly except the compat wrapper you're about to build).
- `Libraries/artisan_lib/src/api/claims.rs` — the `Claims` struct (`sub, role, organization_id, exp, kind`) and `TokenType`.
- `ais_auth/src/auth/permissions.rs` — the current `has_permission(claims, project, role) -> bool`: Super bypass → org-match + `has_org_permission` → `get_permissions()` DB override (fail-closed on error). This becomes a compat wrapper around your new `evaluate_access`.
- `ais_auth/src/database/helpers.rs` — `get_permissions()` (the read-only side of a `permissions` table that has **no write path anywhere in the codebase** — this is one of the gaps you're closing), `get_runner_org_id()` (24h-cached org lookup for a project), `organization_exists()`, and the Phase F/H admin functions (`create_organization`, `update_user_org`, `create_invite`, `assign_runner_org`, etc.) — these are your reference pattern for every new DB function you write.
- `ais_auth/src/service.rs` — `require_admin`/`require_elevated_admin`, and the repeated pattern (`create_organization`, `update_user_org`, `create_invite`, `assign_runner_org`) of: check role via `require_admin`, check `elevated_token`, do the org-scoping check specific to that RPC. Copy this pattern exactly for every new mutating RPC below.
- `ais_auth/src/grpc.rs` — where RPCs get exposed. `HasPermission` (line ~123) is your reference for how a generic Claims-map request gets turned into a real `Claims` and dispatched.
- `ais_auth/proto/accounts.proto` — the current RPC/message surface.
- `ais_auth/migrations/` — `0001_org_admin.sql`, `0002_org_invites.sql`, `0003_org_uuid_expand.sql`. Migrations run automatically via `sqlx::migrate!("./migrations")` (`ais_auth/src/database/connections.rs`). Follow this numbering (your new migration is `0005_rbac_policies.sql` — `0004_org_uuid_contract.sql` lives in `migrations_manual/`, run by hand, not by the auto-runner; don't confuse the two directories).

## 2. New schema

Add `ais_auth/migrations/0005_rbac_policies.sql`:

```sql
CREATE TABLE rbac_policies (
    id CHAR(36) PRIMARY KEY,
    organization_id CHAR(36) NOT NULL,   -- '00000000-0000-0000-0000-000000000000' = GLOBAL default
    resource_type VARCHAR(32) NOT NULL,
    action VARCHAR(32) NOT NULL,
    role VARCHAR(32) NOT NULL,           -- 'admin' | 'controller' | 'viewer' | 'audit' -- never 'super' or 'none'
    allow BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    UNIQUE KEY uq_policy (organization_id, resource_type, action, role)
);

CREATE TABLE resource_grants (
    id CHAR(36) PRIMARY KEY,
    user_id CHAR(36) NOT NULL,
    resource_type VARCHAR(32) NOT NULL,
    resource_id VARCHAR(191) NOT NULL,   -- e.g. an 8-hex project_id, a numeric domain_id as a string,
                                          -- "{project_id}/{environment_id}" for secrets, a session_id, a vm_id
    action VARCHAR(32) NOT NULL,
    granted_by CHAR(36) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uq_grant (user_id, resource_type, resource_id, action)
);

CREATE TABLE access_audit_log (
    id CHAR(36) PRIMARY KEY,
    user_id CHAR(36) NOT NULL,
    organization_id CHAR(36) NOT NULL,
    resource_type VARCHAR(32) NOT NULL,
    resource_id VARCHAR(191) NOT NULL,
    action VARCHAR(32) NOT NULL,
    decision BOOLEAN NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_audit_org (organization_id, created_at)
);

CREATE TABLE service_credentials (
    id CHAR(36) PRIMARY KEY,
    organization_id CHAR(36) NOT NULL,
    service_name VARCHAR(64) NOT NULL,
    token_hash CHAR(64) UNIQUE NOT NULL,  -- sha256 hex, same pattern as invites.token_hash -- raw value never stored
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    revoked_at TIMESTAMP NULL,
    UNIQUE KEY uq_service (organization_id, service_name)
);
```

`service_credentials` is here (not deferred to Phase 3) because it's a small, self-contained table with no dependency on anything Phase 3-specific — building it alongside the other three avoids a second migration later. Phase 3's doc covers *using* it; you're only creating the table and the two RPCs below.

**Seed data** (same migration file, after the `CREATE TABLE`s): insert `rbac_policies` rows for `organization_id = '00000000-0000-0000-0000-000000000000'` that reproduce **exactly** what `has_org_permission` already allows today, so nothing regresses:

| role | resource_type | action | allow |
|---|---|---|---|
| admin | (every resource_type) | (every action) | true |
| controller | (every resource_type) | read | true |
| controller | (every resource_type) | control | true |
| viewer | (every resource_type) | read | true |
| audit | (every resource_type) | read | true |

`purchase` is deliberately **not** included in any row above except `admin` — it defaults to Admin/Super-only, and only an explicit org override (inserted later, by an org admin via `SetOrgPolicy`) can loosen it. Resource types for the seed: `organization, node, project, instance, domain, secret, environment, session, vm`.

## 3. `Action` / `ResourceType` in `artisan_middleware`

In `Libraries/artisan_lib/src/identity.rs` (where the existing taxonomy ID types and `ResourceType` already live per `RESOURCE_TAXONOMY.md` §8.3), add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Read,
    Write,
    Control,
    Delete,
    Grant,
    Purchase,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Write => "write",
            Action::Control => "control",
            Action::Delete => "delete",
            Action::Grant => "grant",
            Action::Purchase => "purchase",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Action::Read),
            "write" => Some(Action::Write),
            "control" => Some(Action::Control),
            "delete" => Some(Action::Delete),
            "grant" => Some(Action::Grant),
            "purchase" => Some(Action::Purchase),
            _ => None,
        }
    }
}
```

If `ResourceType` doesn't already exist as a real Rust enum in `identity.rs` (only in the taxonomy doc), add it the same way, with the same `as_str`/`from_str` pair, values matching §8.3: `Organization, Node, Project, Instance, Domain, Secret, Environment, Session, Vm`. Bump `artisan_middleware`'s version (check current version in `Libraries/artisan_lib/Cargo.toml` first, bump appropriately — coordinate with whoever else is touching this crate this week) and publish once this phase's other changes are also ready, since every other phase depends on this exact enum shape.

## 4. The evaluator

In `ais_auth/src/auth/permissions.rs`, add alongside the existing `has_permission`:

```rust
use artisan_middleware::identity::{Action, ResourceType};

pub async fn evaluate_access(
    claims: &Claims,
    resource_type: ResourceType,
    resource_id: &str,
    action: Action,
) -> bool {
    if claims.role == Role::Super {
        return true;
    }

    let resource_org = resolve_resource_org(resource_type, resource_id).await;

    let mut decision = false;

    if resource_org.as_deref() == Some(claims.organization_id.as_str()) {
        decision = policy_allows(&claims.organization_id, resource_type, action, claims.role)
            .await
            .unwrap_or(false)
            || policy_allows(GLOBAL_ORG, resource_type, action, claims.role)
                .await
                .unwrap_or(false);
    }

    if !decision {
        decision = resource_grant_exists(&claims.sub, resource_type, resource_id, action)
            .await
            .unwrap_or(false);
    }

    // audit the sensitive actions -- see step 6
    if matches!(action, Action::Purchase | Action::Grant | Action::Delete) {
        let _ = write_audit_log(claims, resource_type, resource_id, action, decision).await;
    }

    decision
}

pub const GLOBAL_ORG: &str = "00000000-0000-0000-0000-000000000000";
```

`resolve_resource_org` for this phase only needs the `Project`/`Instance`/`Secret` cases (reuse the existing `get_runner_org_id` from `database/helpers.rs` — a Secret's `resource_id` is `"{project_id}/{environment_id}"`, split on `/` and look up the `project_id` half). The `Domain`/`Session`/`Vm` cases are added by Phases 4/5/6 respectively when those services exist to call them — stub them to return `None` (fail-closed) for now with a `// TODO(phase N):` comment, don't guess at their shape.

**Compat wrapper** — replace the body of the existing `has_permission` (keep its signature identical, every existing caller in `portal`/`domain_management` stays unchanged):

```rust
pub async fn has_permission(claims: &Claims, project: &str, role: Role) -> bool {
    let action = match role {
        Role::Admin | Role::Super => Action::Control, // matches has_org_permission's "Admin has full access" today
        Role::Controller => Action::Control,
        Role::Viewer | Role::Audit => Action::Read,
        Role::None => return false,
    };
    evaluate_access(claims, ResourceType::Project, project, action).await
}
```

Check this mapping against `has_org_permission`'s actual current match arms before finalizing — the goal is that for every `(current_role, required_role)` pair `has_org_permission` returns `true` for today, the GLOBAL policy seed (step 2) plus this wrapper must also return `true`, and vice versa for `false`. Write the regression test in step 7 first, then adjust the seed/wrapper until it passes, rather than reasoning about it in the abstract.

## 5. New RPCs

Add to `ais_auth/proto/accounts.proto` (and update the `PermissionRequest.project_name` → `project_id` rename while you're in this file — it never got renamed despite `RESOURCE_TAXONOMY.md` claiming this phase done):

```proto
rpc EvaluateAccess(EvaluateAccessRequest) returns (PermissionResponse);
rpc GrantResourcePermission(GrantResourceRequest) returns (GrantResourceResponse);
rpc RevokeResourcePermission(RevokeResourceRequest) returns (RevokeResourceResponse);
rpc ListResourceGrants(ListResourceGrantsRequest) returns (ListResourceGrantsResponse);
rpc SetOrgPolicy(SetOrgPolicyRequest) returns (SetOrgPolicyResponse);
rpc ListOrgPolicies(ListOrgPoliciesRequest) returns (ListOrgPoliciesResponse);
rpc IssueServiceCredential(IssueServiceCredentialRequest) returns (IssueServiceCredentialResponse);
rpc ValidateServiceCredential(ValidateServiceCredentialRequest) returns (ValidateServiceCredentialResponse);

message EvaluateAccessRequest { map<string,string> claims = 1; string resource_type = 2; string resource_id = 3; string action = 4; }

message GrantResourceRequest { string elevated_token = 1; string target_user_id = 2; string resource_type = 3; string resource_id = 4; string action = 5; }
message GrantResourceResponse { bool success = 1; }
message RevokeResourceRequest { string elevated_token = 1; string target_user_id = 2; string resource_type = 3; string resource_id = 4; string action = 5; }
message RevokeResourceResponse { bool success = 1; }
message ListResourceGrantsRequest { string access_token = 1; string resource_type = 2; string resource_id = 3; }
message ResourceGrant { string user_id = 1; string resource_type = 2; string resource_id = 3; string action = 4; string granted_by = 5; int64 created_at = 6; }
message ListResourceGrantsResponse { repeated ResourceGrant grants = 1; }

message SetOrgPolicyRequest { string elevated_token = 1; string organization_id = 2; string resource_type = 3; string action = 4; string role = 5; bool allow = 6; }
message SetOrgPolicyResponse { bool success = 1; }
message ListOrgPoliciesRequest { string access_token = 1; string organization_id = 2; }
message OrgPolicy { string organization_id = 1; string resource_type = 2; string action = 3; string role = 4; bool allow = 5; }
message ListOrgPoliciesResponse { repeated OrgPolicy policies = 1; }

message IssueServiceCredentialRequest { string elevated_token = 1; string organization_id = 2; string service_name = 3; }
message IssueServiceCredentialResponse { string raw_credential = 1; } // shown once, never retrievable again
message ValidateServiceCredentialRequest { string raw_credential = 1; }
message ValidateServiceCredentialResponse { bool valid = 1; string organization_id = 2; string service_name = 3; }
```

**Gating** (mirror `service.rs`'s existing pattern exactly):
- `EvaluateAccess`: any valid access token, no elevation — it's a read-only decision, called by other services on every request (see Phase 3-6 docs).
- `GrantResourcePermission` / `RevokeResourcePermission` / `SetOrgPolicy` / `IssueServiceCredential`: `require_admin` + `elevated_token`, same as `update_user_org`. An `Admin` can only grant/set policy within their own org (same restriction pattern as `update_user_org`'s `Admin` branch — check `service.rs:400-407` for exactly how that's enforced); `Super` is unrestricted.
- `ListResourceGrants` / `ListOrgPolicies`: normal access token, no elevation (reads), but still org-scoped the same way `ListUsersInOrg` is.
- `ValidateServiceCredential`: no auth at all (it *is* the auth check — analogous to `ValidateToken` for JWTs).

Implement each RPC in `ais_auth/src/grpc.rs` following `HasPermission`'s existing shape, backed by new functions in `ais_auth/src/database/helpers.rs` (`insert_resource_grant`, `delete_resource_grant`, `list_resource_grants`, `upsert_org_policy`, `list_org_policies`, `policy_allows` (used by the evaluator itself), `create_service_credential`, `validate_service_credential`, `write_audit_log`) — copy the connection-pooling/error-handling style already used by `create_invite`/`get_permissions` in that file, including caching where it makes sense (see step 6).

## 6. Caching (don't skip this — it's foundational, not an optimization)

`evaluate_access` will be called by every service in Phases 3-6 on every single request once they're wired up. Without caching, that's a cross-service gRPC round trip plus 2-4 SQL queries per request, for every service on the platform. Add a short-TTL (60-120s) in-process cache in `ais_auth::auth::permissions` for the policy rows and grant lookups (a `moka` or hand-rolled `HashMap` + timestamp cache is fine — check what caching primitive `get_runner_org_id`'s existing 24h cache already uses in `database/helpers.rs` and reuse that same mechanism, just with a much shorter TTL). This must be short enough that `RevokeResourcePermission` feels like it takes effect within about a minute, not a day.

Also add a matching helper in `artisan_middleware` (e.g. `artisan_middleware::rbac_client`) that Phases 3-6 can use to call `EvaluateAccess` over gRPC with the *client-side* caching baked in, so each of those services doesn't reinvent this — check with whoever's doing Phase 1 first if this is landing in the same PR or a fast-follow, since Phases 3-6 need it to exist before they can be considered done, not just "call the RPC directly with no cache."

## 7. Audit logging

`write_audit_log` (referenced in step 4) inserts one row into `access_audit_log` for every `Purchase`/`Grant`/`Delete` decision (allowed or denied — both are worth recording). Don't log `Read`/`Write`/`Control` decisions; that volume isn't worth it for this pass.

## 8. Small fixes to bundle in this same phase

- `ais_auth/src/service.rs`'s `refresh_tokens` (~line 143) currently re-mints from the refresh token's own embedded claims. Change it to re-fetch `role`/`organization_id` fresh from `users` on every refresh (it already hits the DB to check the refresh token's validity — one more `SELECT` by `user_id` is cheap). This closes the gap where a role/org change doesn't take effect until the access token naturally expires.
- Delete `ais_auth::api::helpers::with_role` (dead code — grep confirms it's never called; also has its own bug, an exact role match instead of hierarchical, and mutates `claims.role` in place).

## 9. Done when

- `cargo build` clean on `ais_auth` and `Libraries/artisan_lib`, no local-path dependency overrides left in `Cargo.toml`.
- Unit tests in `ais_auth/src/auth/permissions.rs`'s existing test module (or a new one in the same file) proving: every `(current_role, required_role)` pair `has_org_permission` allows today, `has_permission`'s new compat wrapper also allows (regression pin); the GLOBAL seed migration reproduces this without any org-specific override row; `Action::Purchase` is denied for `Controller`/`Viewer`/`Audit` and allowed for `Admin`/`Super` against the GLOBAL seed; `evaluate_access` returns `false` (not panics, not `true`) when `resolve_resource_org` returns `None` or a DB call errors; a `RevokeResourcePermission` call is reflected by the cache within its TTL, not just eventually; a `Purchase`/`Grant`/`Delete` decision writes an `access_audit_log` row.
- `service_credentials` table exists with `IssueServiceCredential`/`ValidateServiceCredential` working end-to-end in a test (issue → validate → revoke → validate fails) — Phase 3 depends on this being real, not just schema.
- `artisan_middleware` published with the new `Action`/`ResourceType` enums before you tell whoever's doing Phases 3-6 they can start.
