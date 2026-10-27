# 8.0 plan 01: Identity, URNs and access

Part of the [artisan_middleware 8.0.0 plan set](README.md). Draft v2, 2026-10-26.

| | |
|---|---|
| **Workstreams** | W1 (org-scoped project ids), W2 (typed ids, URN, wire retyping), W6 (RBAC pieces in the library) |
| **When** | Start first. Spec doc fixes (W1.5, W2.4, W2.10) before code; W1.1-W2.3 gate `8.0.0-rc.1`. |
| **Depends on** | Nothing (source of truth is [`platform/architecture/RESOURCE_TAXONOMY.md`](../platform/architecture/RESOURCE_TAXONOMY.md)). |
| **Blocks** | [02](02-runtime-usage-and-config.md) W5 (W2.5-W2.9 retyping and W5.4 slim `AppStatus` ship as one wire break); [04](04-refactor-and-docs.md). |
| **Decisions owned** | D1, D2, D3, D4, D12, D14 (D9 is cross-cutting, see the index) |

Conventions: `[B]` = breaking API/wire/disk change. `file:line` locations are at commit `7c44282` (before the restructure in [04](04-refactor-and-docs.md) moves files). W-numbers are stable across the plan set; see the [index](README.md).

---

## W1. Project identity: organization-scoped ids (the headline 8.0 change)

Why: two orgs deploying the same `user/repo/branch` get the same id today, so the second is refused
(`projects.rs:13-18`; [PORTAL_API §6 #11](../platform/api/PORTAL_API.md)). This plan's W1.5, W2.4 and W2.10 edit the local copies of the taxonomy and `PLATFORM_OVERVIEW.md`, then copy them back (see [`platform/README.md`](../platform/README.md)).

- [ ] **W1.1 [B] New derivation.** `identity.rs:476-481` `ProjectId::from_parts(user, repo, branch)` and `identity.rs:515`
  `generate_project_id`: add the organization to the hash input (`"{organization_id}-{branch}-{repo}-{user}"`, exact
  string frozen in the taxonomy doc). Signature takes `&OrganizationId`. Keep `ProjectId::parse` (8 lowercase hex).
  Intent: ids unique per org. Keep the old function as `legacy_project_id(user, repo, branch)` so existing ids stay
  reproducible/verifiable.
- [ ] **W1.2 [B] `GitAuth` carries identity instead of recomputing it.** `git_actions.rs:31-37` (`user`, `repo`, `branch`) and
  `:336` `generate_id()`. Add `organization_id: Option<OrganizationId>` and a stored `project_id: Option<ProjectId>`;
  `generate_id()` returns the stored id when present, else the legacy derivation. Intent: **existing projects keep their
  ids** (their `ais_<id>` units, `/var/www/ais/<id>` checkouts, ledgers and policy files do not move); only new
  projects use the org-scoped hash. This also makes ais_auth's atomic claim the single arbiter of uniqueness.
- [ ] **W1.3 Make the 8-hex width a conscious choice.** 32 bits of hash space is shared across all orgs. Decide (D1) keep 8
  vs widen; if widened, `ProjectId::parse`, URN grammar, systemd names and every path change at once, so do it now or never.
- [ ] **W1.4 Fix the callers that bypass the helpers.** `Platform/watchdog/src/functions/inventory.rs:264`
  `format!("ais_{}", project.generate_id())` should be `ais_name(..)` (taxonomy §4.3). Audit for other `format!("ais_` sites.
- [ ] **W1.5 Update the spec.** [`RESOURCE_TAXONOMY.md`](../platform/architecture/RESOURCE_TAXONOMY.md) §4.1 (algorithm + the test-vector table, which must be recomputed,
  not hand-written) and add the legacy-id rule.
- [ ] **W1.6 Tests.** Extend `src/tests/taxonomy_ids.rs`: known vectors for new + legacy derivation, same repo under two
  orgs yields different ids, stored id wins over derivation, `GitAuth` JSON written by 7.x (no new fields) still loads.
- Consumer ripple (not library, track separately): GitMonitor `auth_items` data on nodes (`git.rs` ~25 call sites of
  `auth.generate_id()`), portal `handler/repos.rs:146,349,367,550,591,867`, `projects.rs:226,353,421`, `address.rs:76`,
  `v1.rs:1352`, `system/tunnel.rs:263`; ais_auth claim RPC must store/return the id; portal `POST /v1/projects` stops
  refusing the second org.

---

## W2. Typed ids everywhere + URN as the canonical string (aligned to the docs)

Today the library already has the id newtypes (`identity.rs:456-668`) and `Urn`/`ResourceType` (`urn.rs`), but almost no
struct uses them: `aggregator.rs:52` and `systemd.rs:7` alias `type ID = Stringy`, and every message carries `Stringy`.

- [ ] **W2.1 [B] Validating deserialization.** `ProjectId`, `UuidId`/`OrganizationId`/`InstanceId`/`SessionId` use
  `#[serde(transparent)]` (`identity.rs:455, 523, 556`), so `serde_json::from_str::<ProjectId>("\"zzz\"")` succeeds and
  bypasses `parse()`. Switch to `#[serde(try_from = "String", into = "String")]` so malformed ids are rejected at the
  boundary (fail closed). Add `EnvironmentId` validation (slug rules from the doc; D2).
- [ ] **W2.2 Missing id types.** `ResourceType` has `User` and `Invite` (`urn.rs:19-28`) but no `UserId`/`InviteId`
  newtypes. Add both (UUID), per taxonomy §3 / §5.
- [ ] **W2.3 [B] `Urn` rebuilt around typed ids** (`urn.rs:106-181`):
  - remove the free-form `Urn::new(ResourceType, impl Into<String>)`; construct via `From<ProjectId>`, `From<OrganizationId>`,
    `From<InstanceId>`, `From<NodeId>`, `From<DomainId>`, `From<VmId>`, `From<SessionId>`, `From<UserId>`, `From<InviteId>`,
    `From<EnvironmentId>`, `From<SecretRef>`;
  - `Urn::parse` validates the id grammar per type (taxonomy §5 table) — project 8-hex, UUID types, u64 types, secret
    `project/env/key` (split into a real `SecretRef`, fixing hand `split('/')` in `ais_auth/src/auth/permissions.rs:109`);
  - typed accessors `as_project()`, `as_secret()`, ...; `impl FromStr`/`TryFrom<&str>`; `Serialize`/`Deserialize` as the
    URN string (for audit rows and logs);
  - `ResourceType::from_str` (`urn.rs:55`) shadows `std::str::FromStr`; implement the trait instead.
- [ ] **W2.4 `Subscription` is real but undocumented.** In code (`urn.rs:28`), absent from [`RESOURCE_TAXONOMY.md`](../platform/architecture/RESOURCE_TAXONOMY.md) §5, §8.3, §10.
  Add it there: id is the owning `organization_id` (see `ais_auth/.../permissions.rs:115-123`) — document that rule.
- [ ] **W2.5 [B] Replace `Stringy` ids in messages with typed ids.** `aggregator.rs`: `Command.project_id`, `CommandResponse`,
  `RegisterApp`, `DeregisterApp`, `UpdateApp`, `AppStatus` (:745-764), `LiveMetrics` (:300); `portal.rs`: `ProjectInfo.project_id`
  (:34), `NodeInfo.projects`/`NodeDetails.projects` (:158,:194), `ProjectDetails.id` (:330), `CommandResponse`/
  `CommandStatusResponse` ids (:454,:480), `NodeReloadResult.id` (:277). Use `ProjectId`, `InstanceId`, `NodeId`.
- [ ] **W2.6 [B] Add the missing `instance_id`.** `AppStatus` has `project_id` + `git_id` but no instance. Portal fills
  `LiveMetrics` with `project_id: app.git_id` and `instance_id: project_info.get_id()` (`portal/src/system/periodic.rs:168-173`),
  and `ProjectInfo::get_id()` (`portal.rs:40-56`) is `base64(lz4("{node_id}-{project_id}"))` truncated to 100 chars, using the
  deprecated `base64::encode` (hence `#[allow(deprecated)]`). That is the billing key today: opaque, not a UUID, not what
  taxonomy §3.3 specifies. Replace with a real `InstanceId` (UUIDv4) minted when the supervisor first spawns the project on a
  node and **persisted in the state file** so restarts keep it (D12). Drop `git_id` (a repo *is* its project, §3.10).
  Removing `get_id()` also removes the only uses of the `lz4` and `base64` dependencies.
- [ ] **W2.7 [B] Node ids as strings on JSON.** `portal.rs:308` `ProjectSummary.nodes: Vec<u64>` -> `Vec<NodeId>` (serializes
  as a decimal string, §3.4). Add `Identifier::node_id()` (`identity.rs:176-`) so the `.id` u64 stops leaking.
- [ ] **W2.8 [B] Names.** `app_name` still exists in `config.rs:18`, `enviornment/definitions.rs:343`, `aggregator.rs:862`
  (`RegisterApp`), `state_persistence.rs:104,267`, `config_bundle.rs:35`. Taxonomy §4.3 retires `app_name` for the
  deployable unit, and it is now **dropped** (D3, decided): every use becomes `project_id`, with an optional `display_name` label in
  `RuntimeSpec.identity` (W5). `RegisterApp.app_name` (`aggregator.rs:862`) becomes `project_id` + optional `display_name`. The state file path
  `/opt/artisan/tmp/.{app_name}.state` (`state_persistence.rs:267`) is re-keyed on `project_id` and must still find existing files (W5.4c).
- [ ] **W2.9 [B] `Status` over the wire.** Docs: never a bare string; fail-loud. Library has `as_str_name`/`from_str_name`
  (`aggregator.rs:134-178`) but `portal.rs` `CommandStatusResponse.status: String` (:498) and `ApiResponse.status`
  (:75, intentionally `"success"|"error"`, leave) need review. Add the metric/log hook the doc requires for unrecognized
  values (a counter helper callers can reuse; today every caller must remember).
- [ ] **W2.10 Doc fixes that belong to this item.** [`PLATFORM_OVERVIEW.md`](../platform/architecture/PLATFORM_OVERVIEW.md)`:369,389,415,445,466` still show retired URN forms
  (`runner:{id}`, `repo:{user}/{repo}/{branch}`, `secret:{runner_id}:{environment}:{id}`); point them at taxonomy §5.
  `urn.rs:3` header cites "RESOURCE_TAXONOMY.md at the repository root"; change it to the in-repo copy, `docs/platform/architecture/RESOURCE_TAXONOMY.md`.
- [ ] **W2.11 Tests.** Per-type URN grammar (accept/reject table), round-trip typed id <-> URN, serde rejects malformed ids,
  `ResourceType` spellings pinned once (portal copies them, see W6.2).

---

## W6. RBAC pieces that belong in the library

- [ ] **W6.1 `rbac_client`.** [RBAC Phase 1 §6](../platform/rbac/RBAC_PHASE_1_CORE_ENGINE.md) asks for `artisan_middleware::rbac_client` (client-side TTL-cached
  `EvaluateAccess` helper) so services do not each reinvent it. It does not exist here; portal has its own
  (`portal/src/system/grpc.rs` `evaluate_access`, cached, fail-closed). Move that into the library behind a trait that does
  not force `tonic` on every consumer (the `mtls.rs` precedent: no tonic dep).
- [ ] **W6.2 Delete duplicate enums.** `portal/src/system/grpc.rs:413-428` re-declares `ResourceType` (2 variants) and `Action`
  with a test pinning wire spellings; Billing's `auth.rs:81-86` documents a string field. Use the library enums.
- [ ] **W6.3 `Claims`** (`src/api/claims.rs`): check the claim struct against the [RBAC docs](../platform/rbac/RBAC_IMPLEMENTATION_OVERVIEW.md) (`organization_id`, role, kind);
  `whoami` spelling `org_id` is portal ([PORTAL_API §6 #10](../platform/api/PORTAL_API.md)) but lands with this release.
- [ ] **W6.4 Resolver in one place.** `resolve_resource_org` (`ais_auth/.../permissions.rs:102-130`) matches on `ResourceType`
  and a raw `&str`; after W2.3 it should match on the typed `Urn`/id. `evaluate_access(claims, ResourceType, &str, Action)`
  signature stays (docs); add an overload taking `&Urn`.

---

## Decisions

- D1. Project id width: keep 8 hex (recommended; ais_auth claim arbitrates collisions) vs widen to 12/16 (breaks every path).
- D2. `EnvironmentId` grammar (lowercase slug `[a-z0-9-]{1,32}` recommended) and whether it is project-scoped in URNs
  (docs say bare `urn:artisan:environment:<id>`; keep).
- D3. (Decided: `app_name` is dropped.) `project_id` is the key everywhere; optional `display_name` label only. Remaining check: watchdog's use (`watchdog/src/scripts/mod.rs`, 3 sites) and the runner's `CARGO_PKG_NAME` assignment in `generic_runner/config.rs` `get_config`.
- D4. Reorg vocabulary: adopt taxonomy names (recommended) vs amend taxonomy.
- D12. `InstanceId` lifetime: persisted per (project, node) across restarts (recommended, keeps the billing key stable) vs new per spawn.
- D14. Super-role usage: audit-log every action, only sensitive ones (current RBAC rule), and/or notify via Apostle.

## Verification

- IDs: round-trip every id type through serde JSON, bincode (the Manager/portal tunnel format) and URN; load a 7.x `GitAuth` file and a 7.x state file.
- Two organizations deploy the same repo and branch against a throwaway node: both succeed with different ids; an existing project keeps its old id and paths.
- Fail closed: malformed ids and URNs are rejected at deserialization, not at first use.
