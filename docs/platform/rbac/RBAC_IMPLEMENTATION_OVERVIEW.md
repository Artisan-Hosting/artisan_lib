# RBAC / Multi-Tenancy Implementation — Overview & Index

**Status:** Active implementation plan, approved. This is the authoritative index for the RBAC/multi-tenancy project. `RBAC_DRAFT.md` in this same directory is superseded scratch material (an informal chat transcript) — the schema/evaluator design below is its logical conclusion, refined and extended. `RESOURCE_TAXONOMY.md` remains authoritative for identifier naming; this project builds the authorization layer on top of that already-standardized taxonomy.

## Why this project exists

The taxonomy migration standardized *naming* (`organization_id`, `project_id`, etc.) across the platform, but *authorization* is still ad hoc. `ais_auth`'s `has_permission()` is a hardcoded 3-tier check that only covers one resource type (Project), and an audit found several services have **no authorization at all**:

- `ais_secretserver` — no access-token field on its proto; trusts any `runner_id`/`environment_id` handed to it.
- `RunpodManager` — `list_sessions` enumerates every tenant's GPU sessions with no org filter.
- `project_Kaelum`'s control router — mounted with zero auth middleware.
- `ais_vm` — authorizes by raw per-user ownership row only; no `organization_id` column exists in its schema.
- `portal` locally reimplements weaker, ad hoc versions of the org/role check in three places instead of deferring to `ais_auth`.

This project builds a real, data-driven, fine-grained (`resource_type` × `action`) permission engine in `ais_auth`, closes every gap above, and adds mTLS as a transport-layer hardening pass underneath all of it.

## How this is organized

Each phase below has its own implementation doc (`RBAC_PHASE_N_*.md` in this directory), written to be executed independently by someone (or some model) with **no context beyond that single doc plus the codebase itself**. Read this overview first for shared vocabulary and cross-phase dependencies, then read only the phase doc(s) you've been assigned.

| Phase | Doc | Depends on | Touches |
|---|---|---|---|
| 1 | `RBAC_PHASE_1_CORE_ENGINE.md` | nothing (foundation) | `ais_auth`, `artisan_middleware` |
| 2 | `RBAC_PHASE_2_MTLS.md` | nothing (independent of Phase 1's schema, but should land before/alongside 3-5 since those services are edited twice otherwise) | `artisan_middleware`, `ais_auth`, `domain_management`, `ais_secretserver`, `RunpodManager`, `keystore`, `project_Kaelum` |
| 3 | `RBAC_PHASE_3_SECRETS.md` | Phase 1 (needs `evaluate_access`), ideally Phase 2 done first | `ais_secretserver`, `secret.proto` (5 vendor copies → `ais_proto`), `ais_auth` (service credentials) |
| 4 | `RBAC_PHASE_4_SESSIONS_KAELUM.md` | Phase 1, ideally Phase 2 first | `RunpodManager`, `session_manager.proto` (4 vendor copies → `ais_proto`), `project_Kaelum` |
| 5 | `RBAC_PHASE_5_AIS_VM.md` | Phase 1 | `ais_vm` |
| 6 | `RBAC_PHASE_6_DOMAIN_PURCHASE.md` | Phase 1 | `domain_management`, `artisan_middleware` (new `Action::Purchase`) |
| 7 | `RBAC_PHASE_7_PORTAL_CLEANUP.md` (stretch) | Phase 1 | `portal`, `ais_auth` |
| 8 | `RBAC_PHASE_8_HARDENING.md` (stretch) | Phase 2 | `ais_proto` (remaining files) |

Phases 3, 4, 5, 6 are independent of each other and can be worked in parallel once Phase 1 (and ideally Phase 2) land. Phase 7 and 8 are stretch goals — don't block on them.

## Shared vocabulary (every phase doc assumes this)

### `Action` enum (new, in `artisan_middleware::identity`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Read,
    Write,
    Control,
    Delete,
    Grant,
    Purchase,
}
```

Serializes to lowercase strings (`"read"`, `"write"`, ...) matching the `VARCHAR(32)` columns in the new tables — **do not use a SQL `ENUM` column for this anywhere**; validate the vocabulary in Rust instead. (`RESOURCE_TAXONOMY.md` already documents `ais_vm`'s `VmStatus` SQL `ENUM` as a wart — "any variant change requires a paired `ALTER TABLE`" — don't repeat that mistake here, especially since this project adds `Purchase` on day one and will likely add more actions later.)

### `ResourceType` enum

Already exists conceptually in `RESOURCE_TAXONOMY.md` §8.3 (`Organization, Node, Project, Instance, Domain, Secret, Environment, Session, Vm`, plus `User`/`Invite` which aren't RBAC-relevant here). Reuse it; add variants only if a phase doc explicitly says to.

### `Role` enum (existing, unchanged — `Libraries/artisan_lib/src/api/roles.rs`)

`Super, Admin, Controller, Viewer, Audit, None`. This project does **not** change this enum or add custom/dynamic roles — see Non-Goals below.

### The evaluator: `ais_auth::auth::permissions::evaluate_access`

Every phase past Phase 1 wires a service into this function (built in Phase 1):

```rust
pub async fn evaluate_access(
    claims: &Claims,
    resource_type: ResourceType,
    resource_id: &str,
    action: Action,
) -> bool
```

Behavior (see `RBAC_PHASE_1_CORE_ENGINE.md` for the full implementation):
1. `claims.role == Role::Super` → always `true`.
2. Resolve the resource's owning `organization_id`. If it matches the caller's `organization_id`, check `rbac_policies` for that org (falling back to the `GLOBAL` sentinel org row `00000000-0000-0000-0000-000000000000`) for `(resource_type, action, claims.role)`.
3. Otherwise (or if step 2 didn't allow), check `resource_grants` for an explicit `(user_id, resource_type, resource_id, action)` row.
4. Anything else → `false`.

**Fail-closed is a hard invariant**: any DB error, timeout, or unresolvable resource org returns `false`, never `true`. This applies to every caller in every phase, not just Phase 1's own implementation.

### Elevated-token pattern (existing, reuse — don't invent a new one)

Every *sensitive write* RPC in `ais_auth` (creating an org, changing a user's role, creating an invite, and now anything in this project that mutates policy/grants) requires an `elevated_token` (from `POST admin/elevate` re-entering the caller's password), not just a normal access token. See `ais_auth/src/service.rs`'s existing `create_organization`, `update_user_org`, `create_invite` for the pattern to copy. Any new mutating RPC this project adds follows the same shape.

### Non-goals (apply across all phases)

- **No fully dynamic, org-defined custom roles.** `Role` stays the fixed enum. What varies per org/resource is what each role is *allowed to do* (via `rbac_policies`), not the set of roles that exist.
- **No automatic mTLS cert rotation / ACME-style issuance service** (Phase 2). Manually-issued ~90-day leaf certs plus a cheap expiry-alert check are the intentional scope for this pass.
- **No changes to Manager↔portal's tunnel (Noise protocol) or watchdog's transport (root-only Unix socket)** — both already have equivalent-or-stronger protection than mTLS would add; touching them is out of scope.
- **No implementing `domain_management`'s actual purchase/checkout/DNS-provisioning business logic or moving Stripe into `Billing`** (Phase 6 only defines the permission *shape* for when that lands later, as a separate project).

## Cross-cutting rules for whoever implements any phase

1. **Never commit a local-path `artisan_middleware` dependency override.** If you need to point a crate at an unpublished local version of `artisan_middleware` during development, it MUST be reverted to the registry version pin before any commit — the platform's CD tooling manages library version bumps itself, and a committed path override breaks that. (This bit the taxonomy migration five times — don't repeat it.)
2. **Bump `artisan_middleware` deliberately, not silently.** If your phase changes `artisan_middleware`, bump its version (this project's changes are additive to the type system but the `Role`/proto changes are breaking elsewhere — coordinate the exact version number with whoever's doing Phase 1, since it lands first).
3. **Every new migration must be additive-safe on a live fleet** — new tables and nullable/defaulted columns only, seeded so existing behavior is reproduced exactly before anything switches over. Don't drop or rename existing columns as part of this project.
4. **Test against the existing test module in the crate you're touching**, don't build a new harness. Every phase doc below names the specific test file/module to extend.
5. **Report back with a diff, not just "done."** Since multiple people/models are working phases in parallel, the integration step needs to see exactly what changed — don't squash away the reasoning in your commit messages either; follow the existing commit-message style in each repo's `git log` (detailed, explains what was reviewed/fixed and why, not just "add RBAC").
