# Artisan Hosting Platform — Resource Taxonomy

**Status:** Normative spec. This document is authoritative for identifier naming, ID formats, URNs, and status vocabulary across every service in the platform. Where code disagrees with this document, the code is wrong and should be fixed to match — see the migration roadmap in §9 for sequencing.

This replaces an earlier draft of the same name that was lost except for §4.2 onward (neither `Platform/` nor this file's own directory is a git repo, so no history survived); this rewrite also corrects two decisions that draft got wrong: it proposed keeping both `runner_id` and `project_id` as sanctioned public/internal aliases, and it left `organization_id`'s type ambiguous between UUID and integer. Both are resolved below.

---

## 1. Purpose

Identifier and status naming diverged crate-by-crate as the platform grew: the same deployable-unit resource is called `runner_id`, `project_id`, `app_id`, `project_name`, `app_name`, or `runner_name` depending on which service you're reading, and `org_id`/`organization_id` coexist because of a PK-vs-FK naming split in `ais_auth`'s schema. Nine independent status vocabularies exist where one was intended, several of which silently swallow unrecognized values instead of erroring.

This document defines, once, for every resource in the platform:

- its position in the resource hierarchy,
- the single canonical name and type for its identifier,
- its URN form,
- its status vocabulary (if it has one), and
- how that identifier and status cross a service boundary (gRPC, HTTP/JSON, SQL).

## 2. Resource Hierarchy

```
                              ORGANIZATION (organization_id, UUID)
                                        │
        ┌───────────────────┬──────────┼──────────┬───────────────────┐
        ▼                   ▼          ▼          ▼                   ▼
      NODE               PROJECT     DOMAIN     SESSION               VM
  (compute server)   (deployable   (ingress/   (Runpod GPU        (Proxmox/
                       unit)         certs)      compute)          hypervisor)
                          │
                          ▼
                      INSTANCE
                (one running copy of
                 a project on one node)
                          │
                 ┌────────┴────────┐
                 ▼                 ▼
             SECRET             REPO
        (scoped to project    (git source,
         + environment)        not its own
                                identity space)
```

Relationships:
- Organizations own Nodes, Projects, Domains, Sessions, VMs, and Repos.
- A Project has one or more Instances (one per node it runs on; more than one only for HA deployments).
- A Domain optionally attaches to a Project (nullable — "unassigned" is a normal, durable state).
- A Secret is scoped to a `(project, environment)` pair.
- A Repo is a git source configuration; it is not itself a first-class resource with its own identity space (see §3.9).

## 3. Resource Type Definitions

### 3.1 Organization

The tenancy boundary. Owns every other resource below it (directly or transitively).

- **ID field:** `organization_id`
- **Type:** UUID, `String` on the wire, `CHAR(36)` in SQL
- **URN:** `urn:artisan:organization:<uuid>`
- **Status:** none (an organization exists or doesn't; suspension/billing state is a separate concern, not modeled here)

### 3.2 Project

**The single name for what earlier drafts and much of the current code call `runner`, `app`, or `project` interchangeably.** A Project is the deployable unit a customer thinks of as "my app" — it may run as a single instance (non-HA) or as a group of instances across nodes (HA).

- **ID field:** `project_id`
- **Type:** `String`, 8 lowercase hex characters — unchanged from the existing algorithm: `sha256("{branch}-{repo}-{user}")[0..8]`. See §4.1 for the canonical generator.
- **URN:** `urn:artisan:project:<hex8>`
- **Status:** the canonical `Status` enum, §7.1.
- **"Runner" is retired as a resource name.** It survives only as an informal term for the *supervisor process/binary* that executes a project instance — e.g. the `generic_runner` (`ais_runner`) binary, `kaelum_runner`. That is a different concept (the executor) from the resource being executed (the project) and does not need its own identifier or resource type. Do not use "runner" as a field name, proto message name, DB table name, or API resource name going forward.

### 3.3 Instance

One running copy of a Project on one Node. This is the thing that actually consumes CPU/memory and has a PID.

- **ID field:** `instance_id`
- **Type:** `String`, UUIDv4 — chosen over a counter because instances are created by whichever node/runner instantiates them, with no central coordinator to hand out sequential IDs.
- **URN:** `urn:artisan:instance:<uuid>`
- **Status:** the canonical `Status` enum, §7.1 (an instance's status is what watchdog/Manager actually track; a Project's aggregate status, where shown, is derived from its instances).

### 3.4 Node

A compute server running a Manager + Watchdog pair.

- **ID field:** `node_id`
- **Type:** `u64` internally (wraps `Identifier.id` from `artisan_middleware::identity`). **On HTTP/JSON boundaries, encode as a decimal string** (e.g. `"42"`, not `42`) to avoid JavaScript's float-precision loss above 2^53 — this is a deliberate wire-encoding rule, not a type disagreement between services.
- **URN:** `urn:artisan:node:<u64>`
- **Status:** the canonical `Status` enum, §7.1.

### 3.5 Domain

A registered or BYO domain, with its DNS records, certificates, and (optionally) an attached Project.

- **ID field:** `domain_id`
- **Type:** `u64` internally; `fqdn` (`String`) is a secondary lookup key, not an identity. RPCs address a domain via a `oneof selector { domain_id, fqdn }` rather than the three competing schemes (`id_or_fqdn` string, bare `domain_id`, bare `id`+`fqdn`) that exist in the current proto — collapse to one going forward.
- **URN:** `urn:artisan:domain:<u64>` (use the numeric ID, not the fqdn, since a domain can in principle be renewed under a changed fqdn while remaining "the same" resource)
- **Status:** `DomainStatus` — a legitimate, more granular superset of the canonical `Status` enum specific to domain lifecycle (payment → registration → DNS → issuance → active → renewal → error/removed). See §7.2.

### 3.6 Secret

A key/value pair scoped to a Project and an Environment.

- **ID field:** composite — `SecretRef { project_id, environment_id, key }`. Not a bare scalar; do not invent a synthetic `secret_id`.
- **URN:** `urn:artisan:secret:<project_id>/<environment_id>/<key>`
- **Status:** none.

### 3.7 Environment

A named deployment scope (`prod`, `staging`, `dev`, ...) under a Project.

- **ID field:** `environment_id`
- **Type:** `String`
- **URN:** `urn:artisan:environment:<environment_id>`
- Note: `ais_secretserver` is currently the only service that spells this `env_id` — rename to `environment_id` there (§9, Phase 3).

### 3.8 Session

A Runpod GPU compute session (distinct from a dashboard browser login session, which is an unrelated cookie-based concept and is out of scope for this taxonomy).

- **ID field:** `session_id`
- **Type:** `String`, UUID
- **URN:** `urn:artisan:session:<uuid>`
- **Status:** `SessionStatus` — a legitimate superset of canonical `Status` with GPU-session-specific states (`Pending`, `Archived`) not meaningful for a Project instance. See §7.2.

### 3.9 VM

A Proxmox-managed virtual machine.

- **ID field:** `vm_id`
- **Type:** `u64` (the current internal `u16`/`u64` split within `ais_vm` should be widened to `u64` uniformly; the HTTP/OpenAPI layer's `vmid` spelling should be renamed to `vm_id` to match).
- **URN:** `urn:artisan:vm:<u64>`
- **Status:** `VmStatus` — bound directly to a MySQL `ENUM` column; any variant change requires a paired `ALTER TABLE`.

### 3.10 Repo — not an independently-identified resource

A repo (git source configuration: user/repo/branch/server/token) is **not** given its own identifier namespace. Its "ID" is the same 8-hex-character hash used for `project_id`, because a repo *is* a project's source — there is exactly one canonical generator (§4.1), and no separate `repo_id` concept should be introduced. Where code today treats `repo_id` as if it were a distinct field, it is really `project_id` under another name.

## 4. ID Generation and Formats

### 4.1 Canonical Project ID Generator

```rust
// artisan_middleware::identity — the single canonical implementation.
// Replaces the two duplicate implementations that used to exist in the
// codebase (a standalone hash computation inside GitAuth::generate_id, and
// the free function generate_git_project_id in git_actions) — both computed
// this identical hash via separate code paths.
pub fn generate_project_id(user: &str, repo: &str, branch: &str) -> ProjectId {
    let hash_input = format!("{}-{}-{}", branch, repo, user);
    let hash = create_hash(hash_input); // dusa_collection_utils
    ProjectId(truncate(&hash, 8))
}
```

`GitAuth::generate_id(&self) -> Stringy` (in `artisan_middleware::git_actions`) survives as a convenience method for callers that already hold a `GitAuth`; it now delegates directly to `identity::generate_project_id` rather than computing the hash itself, so there is exactly one implementation. `GitMonitor` (the crate that used to call the now-removed `generate_git_project_id` free function) was repointed to `git_item.generate_id()`.

| `user` | `repo` | `branch` | Generated `project_id` |
|---|---|---|---|
| `user1` | `website` | `main` | `63eff31e` |
| `acme-org` | `api` | `develop` | `920b821c` |
| `acme-org` | `backend` | `release-v2` | `3e2223db` |
| `artisans` | `portal` | `main` | `1990e9a5` |

(These are genuinely computed SHA-256 values, verified against the implementation below — the equivalent table in the lost prior draft of this document used fabricated example hashes that did not match the real algorithm; do not reintroduce hand-written example hashes without computing them.)

Properties (unchanged from the original algorithm): idempotent, deterministic, collision-resistant (32 bits of entropy), URL-safe.

### 4.2 ID Type Summary

| Resource | Field | Rust type | Wire type |
|---|---|---|---|
| Organization | `organization_id` | `OrganizationId(String)` UUID | proto `string`, SQL `CHAR(36)` |
| Project | `project_id` | `ProjectId(String)` 8-hex | proto `string` |
| Instance | `instance_id` | `InstanceId(String)` UUIDv4 | proto `string` |
| Node | `node_id` | `NodeId(u64)` | proto `uint64`; decimal string in JSON/HTTP |
| Domain | `domain_id` | `DomainId(u64)` | proto `uint64` (string in JSON); `fqdn` as secondary lookup key |
| Secret | *(composite)* | `SecretRef{project_id, environment_id, key}` | 3 proto fields |
| Environment | `environment_id` | `EnvironmentId(String)` | proto `string` |
| Session | `session_id` | `SessionId(String)` UUID | proto `string` |
| Vm | `vm_id` | `VmId(u64)` | proto `uint64`; HTTP field `vm_id` |
| Repo | *(not a resource)* | uses `ProjectId` | — |

### 4.3 Field Naming Rules

- `organization_id` — never `org_id`. This applies to the SQL primary key in `ais_auth.organizations` as well as every foreign key; there is no PK/FK spelling distinction anymore.
- `project_id` — never `runner_id`, `app_id`, `project_name`, `app_name`, or `runner_name` when referring to the deployable-unit resource.
- `instance_id` — never `app_id` (the current codebase's `AppMessage` variants carry `app_id: ID` where `ID = Stringy`; that field should become `instance_id: InstanceId` or `project_id: ProjectId` depending on which the message actually addresses — see the per-crate migration notes in §9).
- The `ais_` prefix (e.g. `ais_manager`, `ais_63c35f4b`) is a **process/systemd-naming convention**, not part of a resource's identity. Converting between "the process name" and "the resource id" must go through the shared, tested, prefix-anchored helpers `ais_name()` / `strip_ais_prefix()` in `artisan_middleware::identity` — never ad hoc `.replace("ais_", "")`, which is not prefix-anchored and corrupts any identifier that happens to contain the substring `ais_` anywhere other than as a leading prefix.

## 5. URN Scheme

```
Format: urn:artisan:<resource-type>:<id>
Composite ids join their parts with "/" inside the id segment.
```

| Resource | URN Pattern | Example |
|---|---|---|
| Organization | `urn:artisan:organization:<uuid>` | `urn:artisan:organization:550e8400-e29b-41d4-a716-446655440000` |
| User | `urn:artisan:user:<uuid>` | |
| Node | `urn:artisan:node:<u64>` | `urn:artisan:node:42` |
| Project | `urn:artisan:project:<hex8>` | `urn:artisan:project:63c35f4b` |
| Instance | `urn:artisan:instance:<uuid>` | |
| Domain | `urn:artisan:domain:<u64>` | `urn:artisan:domain:17` |
| Secret | `urn:artisan:secret:<project_id>/<environment_id>/<key>` | `urn:artisan:secret:63c35f4b/prod/API_KEY` |
| Environment | `urn:artisan:environment:<environment_id>` | `urn:artisan:environment:prod` |
| Session | `urn:artisan:session:<uuid>` | |
| Vm | `urn:artisan:vm:<u64>` | `urn:artisan:vm:100` |
| Invite | `urn:artisan:invite:<uuid>` | |

There is no separate `urn:artisan:repo:...` — a repo is addressed via its `project_id` (§3.10).

## 6. API Endpoint Conventions

```
BASE URL: https://api.artisanhosting.net/v1

RESPONSE FORMAT:
{
  "status": "success" | "error",
  "data": { ... },
  "errors": [ ... ]
}
```

Resource endpoints use `project`, not `runner`/`app`, as the noun. The `/projects` collection does not exist in the current code — this is new API surface, not a rename of an existing route:

```
PROJECTS
  GET    /projects                          List all projects
  GET    /projects/{project_id}             Get project details
  GET    /projects/{project_id}/instances    List instances for a project
  GET    /projects/{project_id}/config      Get project config (multi-node)
  POST   /projects/{project_id}/config      Update project config

INSTANCES
  GET    /instances/{instance_id}           Get instance details
  POST   /instances/{instance_id}/start|stop|restart

NODES
  GET    /nodes
  GET    /nodes/{node_id}
  GET/POST /nodes/{node_id}/git-config

DOMAINS
  GET    /domains
  POST   /domains
  GET    /domains/{domain_id}
  POST   /domains/{domain_id}/attach        body: { project_id }

SECRETS
  GET    /secrets?project_id=&environment_id=
  POST   /secrets
  POST   /secrets/update
  POST   /secrets/delete

ORGANIZATIONS (admin)
  GET/POST /organizations
  GET      /organizations/{organization_id}
  POST     /organizations/{organization_id}/users
  POST     /organizations/{organization_id}/projects
```

Existing CORS policy (GET/POST/OPTIONS only, hence POST for updates/deletes instead of PUT/DELETE) is unrelated to this taxonomy work and is not changed by it.

## 7. Status Vocabulary

### 7.1 Canonical `Status` (artisan_middleware::aggregator::Status)

The single status enum used for Nodes, Projects, and Instances:

```rust
pub enum Status {
    Starting,
    Running,
    Idle,
    Stopping,
    Stopped,
    Warning,
    Building,
    Error,     // new — see rationale below
    Unknown,   // a legitimate VALUE (e.g. "not yet reported"), never a silent parse fallback
}
```

`Error` is new relative to the prior draft of this document — every domain-specific status enum in the platform already has an error-shaped state (`DomainStatus::Error`, `SessionStatus::Error`, `VmStatus::Error`) except this one, which is the enum everything else nominally maps onto.

**Wire crossing rule:** `Status` must cross a gRPC boundary as a real proto enum, never a bare `string`. Parsing must use `Status::from_str_name(s) -> Result<Status, StatusParseError>` / `Status::as_str_name() -> &'static str` (prost-enum convention). The colorized terminal `Display` impl must never be used for wire serialization (it already isn't, in practice, but nothing enforces that today).

**Fail-loud rule:** no call site may collapse a parse failure into a variant silently. Every unrecognized wire value must be logged (with the raw value) and counted via a metric, then mapped to a *visible* fallback (`Warning`), never treated as equivalent to `Stopped`/`Unknown` for the purposes of deciding whether to record metrics. (The current `Manager/src/applications/watchdog_sync.rs` does exactly the thing this rule forbids: an unrecognized status string silently becomes `Unknown`, and `Unknown` is then treated identically to `Stopped`, so the app's metrics are silently dropped.)

### 7.2 Domain-specific status enums (legitimate supersets, not divergence)

These are allowed to exist because they model states canonical `Status` has no business knowing about (payment, DNS propagation, GPU orphaning). Each should document, in its own crate, which of its variants correspond to canonical `Status` values, so operators can reason across resource types:

- `DomainStatus` (domain_management): `PendingPayment, Registering, ProvisioningDns, PendingDns, Issuing, Active, Renewing, Error, Removed`
- `SessionStatus` (RunpodManager): `Pending, Active, Stopping, Stopped, Error, Archived`
- `VmStatus` (ais_vm): `Running, Stopped, Suspended, Error`
- `KeyStatus` (keystore): `Active, NearExpiration, Expired, Invalid`

What is **not** legitimate: a second definition of an enum that already exists (keystore currently has two independent `KeyStatus` definitions in the same crate — one hand-written, one a checked-in generated-prost copy, with no conversion between them — this must be consolidated to one, §9 Phase 3), or a status enum that overlaps canonical `Status` without being a documented superset (`project_Kaelum`'s `OperationalStatus` imports canonical `Status` *and* defines its own alongside it — its `Disabled{reason}` variant is legitimately Kaelum-specific and should be documented as an extension, not left as an undocumented parallel vocabulary).

## 8. Implementation Guidelines

### 8.1 Where taxonomy code lives

```
artisan_middleware/ (Libraries/artisan_lib — already depended on by ~17 of 19 crates)
├── src/
│   ├── identity.rs          # OrganizationId, ProjectId, InstanceId, NodeId, DomainId,
│   │                        # EnvironmentId, SessionId, VmId, SecretRef newtypes;
│   │                        # ais_name()/strip_ais_prefix() (generalized from
│   │                        # watchdog's ApplicationIdentifiers pattern);
│   │                        # generate_project_id() (the single canonical generator)
│   ├── urn.rs               # Urn type: parse/format per §5
│   └── aggregator.rs        # Status enum (extended with Error), as_str_name/from_str_name

ais_proto/ (new crate/repo — .proto SOURCE FILES ONLY, no generated bindings)
├── accounts.proto           # owned by ais_auth; seed this first (already byte-identical
│                             # across its 3 current vendor copies)
├── watchdog.proto
├── secret.proto
└── session_manager.proto
```

Consuming crates keep whatever `tonic-build`/`tonic-prost-build` version they're already on — `ais_proto` fixes the vendor-and-drift problem (proto files currently have 2-4 diverging md5s across their copies) without forcing a toolchain unification, which is unnecessary risk for this migration.

### 8.2 ID Type Module (shape)

```rust
// artisan_middleware::identity

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrganizationId(String); // UUID

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(String); // 8-hex

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(String); // UUIDv4

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DomainId(u64);

pub struct SecretRef {
    pub project_id: ProjectId,
    pub environment_id: EnvironmentId,
    pub key: String,
}

pub const AIS_PREFIX: &str = "ais_";

/// Prefix-anchored, invertible: `strip_ais_prefix(&ais_name(x)) == Some(x)` for all `x`.
pub fn ais_name(component: &str) -> String {
    format!("{AIS_PREFIX}{component}")
}

pub fn strip_ais_prefix(name: &str) -> Option<&str> {
    name.strip_prefix(AIS_PREFIX)
}

pub fn generate_project_id(user: &str, repo: &str, branch: &str) -> ProjectId { /* §4.1 */ }
```

### 8.3 Resource Type Enum (unchanged shape from the prior draft, updated members)

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceType {
    Organization,
    User,
    Node,
    Project,
    Instance,
    Domain,
    Secret,
    Environment,
    Session,
    Vm,
    Invite,
}
```

`Runner` and `Repo` are deliberately absent from this list — see §3.2 and §3.10.

## 9. Migration Roadmap

See `PLATFORM_OVERVIEW.md` → "Recommendation: Resource Taxonomy Alignment" for the current phase status, and the working plan for full phase-by-phase detail. Status as of this revision:

- **Phase 0** — done: this document + `PLATFORM_OVERVIEW.md` rewritten; local dev unblock for crates with a broken `artisan_middleware` path dependency.
- **Phase 1** — done: `artisan_middleware` 7.0.0 published to crates.io with the types in §4.2/§8.
- **Phase 2** — done for code; **partially done for data**: `ais_auth` is on 7.0.0, `accounts.proto` fields renamed, and the `organization_id` UUID *expand* migration (`0003_org_uuid_expand.sql`) has shipped and backfills the new column alongside the legacy `BIGINT` one. The *contract* migration (dropping the legacy columns, adding NOT NULL/FK constraints) is **intentionally deferred** to a coordinated maintenance window paired with a new binary rollout — this is the expand/contract pattern working as designed, not an oversight. `ais_proto` (the shared `.proto`-only repo) was **never actually stood up** — see §11.
- **Phase 3** — done: domain_management, artisan_cli, ais_secretserver (`env_id`→`environment_id`), ais_vm, keystore (`KeyStatus` consolidated to one definition), RunpodManager, GitMonitor, Billing, Welcome, project_Kaelum, dashboard all repointed to `artisan_middleware` 7.0.0 with their taxonomy renames applied.
- **Phase 4** — done: portal + Manager + watchdog (the bincode fleet-tunnel wire-format group) moved together; each repointed to the registry `artisan_middleware = "7.0.0"` and reviewed/fixed (see each crate's commit history for specifics — notably watchdog's SQLite ledger migration and portal's `ais_`-prefix handling had real bugs beyond the mechanical rename, now fixed).
- **Phase 5** — in progress: see §11 for what cleanup actually found and what's still open.

## 11. Phase 5 Cleanup Findings and Deferred Items

- **Local-path override audit**: found and fixed five crates (`domain_management`, `Billing`, `project_Kaelum`, `Welcome`, `GitMonitor`) that had **committed** a temporary `artisan_middleware = { path = "../../Libraries/artisan_lib" }` override instead of reverting to the registry pin before commit, in violation of the standing workflow rule (temporary path overrides are fine for local dev; they must never survive to a commit, because the platform's CD tooling manages library version bumps itself and a path override defeats that). All five reverted to `artisan_middleware = "7.0.0"` and rebuilt clean. `ais_vm` was also found still pinned to the pre-migration `^5.7.1` (missed in Phase 3) and bumped to `7.0.0`.
- **Duplicate ID-generator audit**: confirmed clean — no remaining calls to the removed `generate_git_project_id` free function anywhere in the platform; `GitAuth::generate_id()` now delegates to the single canonical `identity::generate_project_id` (§4.1).
- **`ais_proto` consolidation — not started.** The plan (§8.1, Phase 2) called for a new repo holding `.proto` source files only (`accounts.proto`, `watchdog.proto`, `secret.proto`, `session_manager.proto`) to kill the vendor-and-drift problem — `secret.proto` currently has 5 independent copies (Manager, ais_secretserver, watchdog, `ais_secrets_old`, `runners/generic_runner`) and `session_manager.proto` has 4 (RunpodManager, RunpodManager/whisper-applet, project_Kaelum, portal), with no mechanism keeping them byte-identical. This never actually happened — no `ais_proto` directory or repo exists yet. Standing it up requires a decision this document can't make on its own: where the new repo lives (own git remote vs. a directory in an existing repo), how consuming crates pull it in (git submodule, `include!` via a build-time fetch, or a small published crate of just the `.proto` files), and who owns publishing it. Flagged here as open scope, not silently dropped.
- **Port collisions — explicitly out of scope, flagged as a follow-up.** `ais_auth` and portal's debug service both currently assume `:50051`; Kaelum and the RunpodManager `whisper-applet` both assume the Session Manager lives on that same port too. None of this is a resource-taxonomy naming problem, so it wasn't in scope for this migration, but it's a real deployment hazard worth its own pass.

## 10. Quick Reference

| Resource | ID field | Type | URN |
|---|---|---|---|
| Organization | `organization_id` | UUID string | `urn:artisan:organization:<uuid>` |
| Project | `project_id` | 8-hex string | `urn:artisan:project:<hex8>` |
| Instance | `instance_id` | UUID string | `urn:artisan:instance:<uuid>` |
| Node | `node_id` | u64 (string on wire) | `urn:artisan:node:<u64>` |
| Domain | `domain_id` | u64 (string on wire) | `urn:artisan:domain:<u64>` |
| Secret | composite | — | `urn:artisan:secret:<project_id>/<environment_id>/<key>` |
| Environment | `environment_id` | string | `urn:artisan:environment:<id>` |
| Session | `session_id` | UUID string | `urn:artisan:session:<uuid>` |
| Vm | `vm_id` | u64 | `urn:artisan:vm:<u64>` |
| Invite | `invite_id` | UUID string | `urn:artisan:invite:<uuid>` |
