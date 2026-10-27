# Artisan Hosting Platform Overview

## Executive Summary

The Artisan Hosting Platform is a distributed infrastructure platform built in Rust that manages nodes, runners, and containerized applications across multiple cloud providers. The system is designed for horizontal scaling, high availability, andmulti-tenancy through a centralized portal controlling distributed nodes.

---

## Architecture Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        ARTISAN HOSTING ARCHITECTURE                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ╔═════════════════════════════════════════════════════════════════════════╗ │
│   ║                        PORTAL (Central Control)                          ║ │
│   ║   • Fleet Tunnel Accept Loop                                             ║ │
│   ║   • Command Dispatch                                                     ║ │
│   ║   • Status Aggregation                                                   ║ │
│   ║   • REST API Endpoints                                                   ║ │
│   ╚═════════════════════════════════════════════════════════════════════════╝ │
│                              │                                              │
│                              │ TLS 1.3 / Noise_NK Encrypted                 │
│                              │ (Port 9800)                                  │
│                              ▼                                              │
│   ┌─────────────────────────┬─────────────────────────┬────────────────────┐ │
│   │                         │                         │                    │ │
│   ▼                         ▼                         ▼                    │ │
│ ┌─────────────────┐   ┌─────────────────┐   ┌────────────────────────┐    │ │
│ │   Manager Node 1│   │   Manager Node 2│   │     Manager Node N     │    │ │
│ │   ┌───────────┐ │   │   ┌───────────┐ │   │    (distributed)       │    │ │
│ │   │ Watchdog  │ │   │   │ Watchdog  │ │   │                        │    │ │
│ │   │(supervisor)│ │   │   │(supervisor)│ │   │   ┌────────────────┐ │    │ │
│ │   └─────┬─────┘ │   │   └─────┬─────┘ │   │   │   Runners      │ │    │ │
│ │         │       │   │         │       │   │   └────────────────┘ │    │ │
│ │    ┌────┴────┐  │   │    ┌────┴────┐  │   │                        │    │ │
│ │    │ Runners │  │   │    │ Runners │  │   │   ┌────────────────┐ │    │ │
│ │    └─────────┘  │   │    └─────────┘  │   │   │   VMs          │ │    │ │
│ └─────────────────┘   └─────────────────┘   │   └────────────────┘ │    │ │
│                                             └────────────────────────┘    │ │
│                                                                             │ │
│   ▲───────────────────────────────────────────────────────────────────────▲ │
│   │    Node ↔ Manager     │    Manager ↔ Portal                           │ │
│   │    Local Unix Socket  │    Fleet Tunnel (persistent)                  │ │
│   └───────────────────────┘    Port 9800 via NAT/Firewall                │ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## System Components

### 1. Manager (`Manager/`) - `ais_manager`

**Role**: Fleet orchestrator and node coordinator

**Key Responsibilities**:
- Maintains persistent bidirectional tunnel connection to Portal (`:9800`)
- Proxies lifecycle commands to local Watchdog service
- Caches "app inventory + status" snapshots for Portal queries
- Periodically refreshes per-app stdout/stderr from state files
- Manages git repository configurations and deployments

**Architecture**:
```
 Manager
   ├── network::command_processor (shared by fleet + debug)
   ├── applications::watchdog_sync (status cache)
   ├── applications::git_repos (repo management)
   ├── system::portal (fleet tunnel supervisor)
   ├── system::debug_listener (loopback :9825)
   └── system::state (persistence layer)
```

**Key Features**:
- `Noise_NK` encrypted tunnel with Portal
- Periodic git repo audit and cleanup
- Secret-server KV relay to watchdog
- Config file validation and hot-reload

---

### 2. Watchdog (`watchdog/`) - `ais_watchdog`

**Role**: Process supervisor and telemetry collector

**Key Responsibilities**:
- Supervises all critical applications (system + client)
- Collects CPU/memory/network metrics
- Maintains rolling log buffers (500 entries)
- Tracks build and verification status
- Exposes gRPC interface for status queries

**Architecture**:
```
 Watchdog
   ├── ApplicationStatusStore (runtime state)
   ├── BuildStatusStore (build tracking)
   ├── VerificationStatusStore (integrity)
   ├── SystemInformationStore (node metadata)
   ├── ChildProcessArray (supervised processes)
   ├── monitoring loops (5s cadence)
   └── grpc::serve (Unix socket: /tmp/artisan_watchdog.sock)
```

**Monitoring Pipeline**:
```
┌─────────────────────────────────────────────────────────────────┐
│                    WATCHDOG MONITORING PIPELINE                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌────────────────┐     ┌────────────────┐                     │
│  │  Process State │────▶│  Metrics       │─────┐              │
│  │  Snapshots     │     │  Collection    │     │              │
│  └────────────────┘     └────────────────┘     ▼              │
│                                             ┌────────────────┐ │
│  ┌────────────────┐     ┌────────────────┐ │  Status Store  │ │
│  │  Integrity    │────▶│  Build Status  │◀───│  (gRPC API)  │ │
│  │  Verification │     │  Tracking      │    └────────────────┘ │
│  └────────────────┘     └────────────────┘                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

### 3. Portal (`portal/`) - Artisan Portal

**Role**: Central management interface and API gateway

**Key Responsibilities**:
- Accepts fleet tunnel connections from all Managers
- Maintains live tunnel registry for all nodes
- Provides REST API for web interface and external integrations
- Handles authentication, authorization, and RBAC
- Manages organization, user, and resource relationships

**API Structure**:
```
/api/v1/
├── auth/
│   ├── login              POST  - User authentication
│   ├── whoami             POST  - Current user info
│   └── accept-invite      POST  - Accept organization invite
├── nodes/
│   ├──                    GET   - List all nodes (Admin)
│   ├── {id}               GET   - Node details (Admin)
│   └── {id}/git-config    GET/POST - Git configuration (Admin)
├── runners/
│   ├──                    GET   - List all runners (Auth)
│   ├── {id}               GET   - Runner details (Auth)
│   ├── {id}/git-info      GET   - Repo label resolution (Auth)
│   └── {id}/config        GET/POST - Multi-node config (Auth)
├── control/
│   └── {runner}/{command} GET   - Runner control (Auth)
├── admin/
│   ├── organizations/     GET/POST - Org management (Admin)
│   ├── invitations/       GET/POST - Invite management (Admin)
│   └── users/{id}/org     POST  - User org assignment (Admin)
├── repos/
│   ├──                    GET   - Repo catalog (Admin)
│   └── deploy             POST  - Deploy repository (Admin)
└── secrets/
    ├──                    GET   - List secrets (Auth)
    ├──                    POST  - Create secret (Auth)
    └── update             POST  - Update secret (Auth)
```

---

### 4. Runpod Session Manager (`RunpodManager/`)

**Role**: Cloud session orchestration for remote compute

**Key Responsibilities**:
- Manages container sessions on Runpod infrastructure
- Handles template deployment and lifecycle
- Implements heartbeat-based health monitoring
- Supports orphan session reuse for cost optimization
- Manages multi-GPU allocation with cost-aware scheduling

**Architecture**:
```
Session Manager
   ├── grpc::SessionManagerServer (API endpoint)
   ├── services::SessionService (domain logic)
   ├── services::ProviderSyncService (async reconciliation)
   ├── runpod::RunpodService (cloud provider integration)
   ├── db::GlobalState (SQLite + in-memory cache)
   └── models::Session (session state)
```

**Session Lifecycle**:
```
┌─────────────────────────────────────────────────────────────────┐
│                    SESSION LIFECYCLE                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐    ┌───────────┐    ┌──────────┐    ┌──────────┐ │
│  │  Deploy  │───▶│  Active   │◀───│  Health  │◀───│  Worker  │ │
│  │ Request  │    │  Running  │    │  Check   │    │  Active  │ │
│  └──────────┘    └───────────┘    └──────────┘    └──────────┘ │
│       │              │               │               │         │
│       │              │               │               │         │
│       ▼              ▼               ▼               ▼         │
│  ┌──────────┐    ┌───────────┐    ┌──────────┐    ┌──────────┐ │
│  │  Error   │◀───│  Queued   │◀───│  Misses  │◀───│  Orphan  │ │
│  │  State   │    │  Waiting  │    │  Threshold│    │  Expired │ │
│  └──────────┘    └───────────┘    └──────────┘    └──────────┘ │
│                                                                  │
│  • Active:    Periodic heartbeats, resource monitoring          │
│  • Orphan:    Reusable session, no heartbeat expected           │
│  • Expired:   Cleanup, archival, Pod termination                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

### 5. Domain Management (`domain_management/`) - `ais_domains`

**Role**: Domain registration, SSL certificate management, and DNS orchestration

**Key Responsibilities**:
- Purchases domains via Cloudflare Registrar + Stripe
- Issues/renews SSL certificates via Let's Encrypt
- Manages DNS records via Cloudflare API
- Publishes nginx configurations to edge via R2
- Orchestrates organization assignment for domains

**Architecture**:
```
Domain Manager
   ├── db::Database (MySQL - domain state)
   ├── acme::AcmeClient (Let's Encrypt integration)
   ├── cloudflare::CloudflareClient (DNS/Registrar API)
   ├── publish::Publisher (R2 deployment)
   ├── inventory::Scanner (nginx tree scanning)
   └── grpc::DomainService (gRPC API)
```

**API Endpoints**:
```
DomainService RPCs:
├── Purchasing (Phase 3)
│   ├── SearchDomains
│   ├── QuoteDomain
│   ├── CreateOrder
│   └── HandleStripeWebhook
├── Lifecycle (Phase 2)
│   ├── AddDomain
│   ├── ListDomains
│   ├── VerifyDomainNow
│   └── WatchDomain (streaming)
├── Vhosts
│   ├── AttachDomain
│   └── DetachDomain
├── Members
│   ├── InviteDomainMember
│   ├── ListDomainMembers
│   └── RemoveDomainMember
└── Ops (Admin)
    ├── ListCertificates
    ├── ForceRenew
    └── PublishNow
```

---

### 6. Authentication Service (`ais_auth/`)

**Role**: Identity, access control, and authentication

**Key Responsibilities**:
- JWT-based authentication with access/refresh token rotation
- Organization management and invitational workflows
- Role-based access control (RBAC)
- Permission caching and validation

**Architecture**:
```
Auth Service
   ├── api::Claims (JWT claim structure)
   ├── api::Roles (Super > Admin > Controller > Viewer > Audit > None)
   ├── database::Caching (org/permission caches)
   ├── mail::ApostleClient (email invites)
   └── grpc::AccountInternal (internal gRPC interface)
```

**Permission Hierarchy**:
```
Role::Super    → Bypass all checks (unrestricted)
   ↓
Role::Admin    → Full access within organization
   ↓
Role::Controller → Can modify resources they own
   ↓
Role::Viewer   → Read-only access
   ↓
Role::Audit    → Read-only with additional logging access
   ↓
Role::None     → No access
```

---

### 7. Other Components

| Component | Purpose | Language |
|-----------|---------|----------|
| `GitMonitor` | Automated git polling and sync | Rust |
| `Billing` | Usage cost tracking and calculation | Rust |
| `IdentityManager` | Node identity management | Rust |
| `Runner` (generic) | Standard runner for arbitrary workloads | Rust |
| `project_Kaelum` | Ollama runner with OpenAI-compatible API | Rust |
| `watchdog` | Process supervision layer | Rust |
| `SecretServer` | Encrypted secret storage | Rust |

---

## Resource Hierarchy and Relationships

### organizational Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    RESOURCE HIERARCHY                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│                              ORGANIZATION (org_id)                          │
│                                    │                                        │
│              ┌─────────────────────┼─────────────────────┐                │
│              │                     │                     │                │
│              ▼                     ▼                     ▼                │
│    ┌────────────────┐  ┌────────────────┐  ┌────────────────┐             │
│    │     NODES      │  │    RUNNERS     │  │    DOMAINS     │             │
│    │   (compute     │  │  (workloads)   │  │   (ingress)    │             │
│    │    servers)    │  │   /applets     │  │   /certs       │             │
│    └───────┬────────┘  └───────┬────────┘  └───────┬────────┘             │
│            │                   │                   │                      │
│            ▼                   ▼                   ▼                      │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐              │
│  │     SECRETS      │  │     VMs        │  │    REPOS       │              │
│  │ (configuration)  │  │  (compute)     │  │   (sources)    │              │
│  └────────────────┘  └────────────────┘  └────────────────┘              │
│                                                                             │
│  RELATIONSHIPS:                                                             │
│  • Nodes host Runners                                                       │
│  • Runners belong to Organizations                                          │
│  • Domains attach to Runners                                                │
│  • Secrets scope to Runners + Environments                                  │
│  • Repos belong to Organizations                                            │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Resource Types Reference

### Node Resource

```
Type:       node
URN:        node:{id}
ID Type:    u64
Table:      managers (db)
Status:     Starting|Running|Idle|Stopping|Stopped|Warning|Building

Attributes:
  • identity.id (u64)              - Unique node identifier
  • hostname (string)              - System hostname
  • ip_address (string)            - External IP address
  • status (enum)                  - Current operational status
  • runners (string[])             - List of runner IDs
  • manager_data.*                 - Manager metadata
  • created_at (timestamp)         - Registration time
  • last_updated (timestamp)       - Last heartbeat
```

### Runner Resource

```
Type:       runner
URN:        runner:{id}
ID Type:    string (ais_{git_id})
Table:      runners (db) + Manager state
Status:     Running|Stopped|Building|Unknown

Attributes:
  • name (string)                  - Runner identifier
  • status (enum)                  - Current status
  • version (string)             - Software version
  • org_id (string)                - Organization ID
  • nodes (number[])               - Node IDs hosting this runner
  • artisan_config.*               - Runtime configuration
  • specific_config.*              - Per-instance overrides
  • enviornment (string)           - Environment name
  • health.*                       - Health metrics

Variants:
  • System runner:    ais_manager, ais_watchdog, ais_gitmon
  • Client runner:    ais_{customer_id}, generic_runner
  • External runner:  Runpod/VM-based (via Session Manager)
```

### Domain Resource

```
Type:       domain
URN:        domain:{fqdn} or domain:{id}
ID Type:    string
Table:      domains (db)
Status:     Pending|Registering|Active|Error|Removed

Source:     Purchased|BYO|Imported

Attributes:
  • fqdn (string)                  - Fully qualified domain name
  • org_id (string)                - Owning organization
  • runner_id (string)             - Attached project (nullable)
  • source (enum)                  - Acquisition source
  • status (enum)                  - Lifecycle status
  • cf_zone_id (string)            - Cloudflare zone identifier
  • challenge_target (string)      - ACME challenge CNAME target
  • expires_at (timestamp)         - Registry expiry
  • auto_renew (bool)              - Auto-renewal toggle
  • last_error (string)            - Last error message

Operations:
  • Purchase via Stripe/Cloudflare
  • Import existing domains
  • Attach to runner
  • Issue/renew SSL certificates
```

### Secret Resource

```
Type:       secret
URN:        secret:{runner_id}:{environment}:{id}
ID Type:    composite key
Table:      secrets (db)

Attributes:
  • runner_id (string)             - Associated runner
  • environment (string)           - Scope: prod|stage|dev
  • key (string)                   - Secret identifier
  • value (encrypted)              - Encrypted secret value
  • created_at (timestamp)         - Creation time

Scoping:
  •Runner-scoped: Available to specific runner
  •Environment-scoped: Per-environment variants
  •Inheritance: Runner's org determines access
```

### Repo Resource

```
Type:       repo
URN:        repo:{user}/{repo}/{branch}
ID Type:    string (derived)
Table:      git_config (db)
Status:     Pending|Active|Error

Attributes:
  • user (string)                  - GitHub/GitLab org/user
  • repo (string)                  - Repository name
  • branch (string)                - Target branch
  • nodes (number[])               - Deployed node IDs
  • org_id (string)                - Owning organization
  • server (enum)                  - GitHub|GitLab|Custom
  • token (string, encrypted)      - Access token

Operations:
  • Deploy to nodes
  • Edit configuration
  • Trigger rebuilds
```

---

## Communication Patterns

### Manager ↔ Portal Communication

```
┌─────────────────────────────────────────────────────────────────┐
│                  FLEET TUNNEL PROTOCOL                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Transport:     TCP, persistent connection, full-duplex          │
│  Port:          9800                                             │
│  Encryption:    Noise_NK handshake                               │
│  Protocol:      simple_comms 2.x (binary, bincode payload)       │
│  Direction:     Manager initiates, Portal responds               │
│                                                                  │
│  Connection lifecycle:                                           │
│  1. Manager dials Portal (outbound - NAT traversal)              │
│  2. Noise_NK handshake                                           │
│  3. Manager sends identity + capabilities                        │
│  4. Portal registers tunnel in node_tunnels registry             │
│  5. Bidirectional RPC via AppMessage envelope                    │
│                                                                  │
│  Message types:                                                  │
│  • Command{app_id, command_type, timestamp}                     │
│  • Response{request_id, command_type, message, error}           │
│                                                                  │
│  Commands:                                                       │
│  • StartApp{app_id, config}                                      │
│  • StopApp{app_id}                                               │
│  • RestartApp{app_id}                                            │
│  • Status{app_id}                                                │
│  • AllStatus{}                                                   │
│  • GetGitConfig{}                                                │
│  • SetGitConfig{config}                                          │
│  • WatchdogConfig{kind, content}                                 │
│  • Custom{command: "StreamLogs"}  (special streaming)           │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Portal API Authentication

```
┌─────────────────────────────────────────────────────────────────┐
│                  AUTHENTICATION FLOW                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. User logs in → Portal                                          │
│     POST /v1/login                                               │
│     { email, password }                                          │
│                                                                  │
│  2. Portal validates → ais_auth                                  │
│     JWT access_token (15m expiry)                               │
│     JWT refresh_token (7d expiry)                               │
│                                                                  │
│  3. Portal issues session cookie                                │
│     Set-Cookie: session={token}; HttpOnly; Secure               │
│                                                                  │
│  4. Subsequent requests include cookie                          │
│     Cookie: session={token}                                     │
│                                                                  │
│  5. Portal validates token                                       │
│     • Check signature and expiry                                │
│     • Fetch claims: { sub, org_id, role, exp }                  │
│     • Check role-based permissions                              │
│     • Enforce org scoping                                       │
│                                                                  │
│  6. Admin elevation (password required)                         │
│     POST /v1/admin/elevate                                       │
│     { password }                                                 │
│     → Elevated token (short-lived, ~5 min)                      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Current Implementation Status

### Completed Features

| Feature | Status | Notes |
|---------|--------|-------|
| Fleet tunnel | ✅ | Noise_NK, persistent, multi-node |
| Resource monitoring | ✅ | CPU, memory, logs, network |
| Git management | ✅ | Polling, hooks, multi-node sync |
| Secret management | ✅ | Encrypted, runner-scoped |
| Organization scope | ✅ | Basic org model, limited UI |
| Domain management | ✅ | Cloudflare API, ACME, R2 publish |
| Runpod integration | ✅ | Session manager, heartbeat, reuse |
| RBAC | ✅ | Role hierarchy, permission checking |
| Multi-node config | ✅ | Consistent config across nodes |

### Work in Progress / Incomplete

| Feature | Status | Notes |
|---------|--------|-------|
| Project naming | 🚧 | Spec resolved (single name: "project" — see `RESOURCE_TAXONOMY.md`); rollout in progress, see roadmap below |
| Organization ID type | 🚧 | Spec resolved (UUID everywhere); `ais_auth` still on `BIGINT` pending its migration (Phase 2) |
| Resource grouping | ⚠️  | Org exists, not fully woven through |
| Explicit permission inheritance | ⚠️  | Flat check, no hierarchical policy |
| Resource tagging | ⚠️  | No tagging system |
| Unified resource API | ⚠️  | Resource-specific endpoints; `/projects` collection not yet built (see `RESOURCE_TAXONOMY.md` §6) |
| Audit logging | ⚠️  | Basic, not comprehensive |
| Resource relationships | ⚠️  | Partially connected |
| Status vocabulary | 🚧 | Nine overlapping status enums across services; canonical `Status` gaining an `Error` variant and fail-loud parsing — see `RESOURCE_TAXONOMY.md` §7 |

---

## Recommendation: Resource Taxonomy Alignment

**The resource type definitions, ID formats, URN scheme, naming conventions, and status vocabulary referenced above are now fully specified in `RESOURCE_TAXONOMY.md`** at the repository root — read that document for the normative spec rather than this section, which is retained only as a pointer. Its migration roadmap (§9) sequences the rollout across every affected crate; status:

1. `artisan_middleware` 7.0.0 — done, published to crates.io.
2. `ais_auth` — done for code (organization ID UUID *expand* migration, `accounts.proto` renames); the *contract* migration (dropping legacy columns) is intentionally deferred to a coordinated maintenance window.
3. Independent consumers (domain_management, artisan_cli, ais_secretserver, ais_vm, keystore, RunpodManager, GitMonitor, Billing, Welcome, project_Kaelum, dashboard) — done, all on `artisan_middleware` 7.0.0.
4. Portal + Manager + watchdog — done, moved together as a synchronized wave.
5. Cleanup — in progress. Local-path dependency overrides audited and fixed platform-wide. The shared `ais_proto` crate for `.proto` vendoring (`secret.proto` has 5 diverging copies, `session_manager.proto` has 4) was never actually stood up and needs a decision on where it lives before it can happen — see `RESOURCE_TAXONOMY.md` §11.

Resource-level permission inheritance, tagging, and a unified resource API remain open work items not yet covered by a written spec. A `:50051` port collision between `ais_auth` and portal's debug service (and Kaelum/whisper-applet assuming the Session Manager lives there too) is a known, separate deployment hazard — not a taxonomy problem — also tracked in `RESOURCE_TAXONOMY.md` §11.

---

## Summary

The Artisan Hosting Platform consists of:

| Component | Count | Primary Function |
|-----------|-------|------------------|
| Resources | ~15 types | Nodes, Runners, Domains, Secrets, VMs, Repos |
| Services | 10+ | Manager, Watchdog, Portal, Auth, Session Manager |
| APIs | 20+ | Portal REST, gRPC internal, Cloud integrations |
| Languages | Rust (mostly) | Type-safe, concurrent, performant |

**Architecture Principle**: Distributed control (Manager/Watchdog on nodes) with centralized coordination (Portal) via secure, persistent tunnels.
