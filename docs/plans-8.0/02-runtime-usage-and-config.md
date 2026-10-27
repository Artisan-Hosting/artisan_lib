# 8.0 plan 02: Runtime, usage accounting and config/state

Part of the [artisan_middleware 8.0.0 plan set](README.md). Draft v2, 2026-10-26.

| | |
|---|---|
| **Workstreams** | W3 (usage and billing accounting), W4 (process supervision and monitoring), W5 (AppConfig / Environment / AppState rework) |
| **When** | W3.4 (egress sum) ships first as a patch. W4 with its tests first, then W3 on top of the W4.4 counters. W5 follows plan 01 W2. |
| **Depends on** | [01](01-identity-urn-access.md) for `InstanceId`/`ProjectId` types (W2.5, W2.6); cgroup facts from the watchdog sandbox plan ([`CONTRACTS.md`](../platform/plans/per-app-2026-10/watchdog/CONTRACTS.md), C2 and C17). |
| **Blocks** | [03](03-security-and-cleanup.md) W9 dependency removals (`config`, `lz4`, `base64`); [04](04-refactor-and-docs.md). |
| **Decisions owned** | D5, D6, D7, D8, D13, D22, D23, D24, D25 (D9 cross-cutting) |

Conventions: `[B]` = breaking API/wire/disk change. `file:line` locations are at commit `7c44282` (before the restructure in [04](04-refactor-and-docs.md) moves files). W-numbers are stable across the plan set; see the [index](README.md).

---

## W3. Usage and billing accounting (rebuild)

Data path: `ResourceMonitor` (/proc, 250 ms) -> watchdog `AppStatus.metrics` -> portal polls `AllStatus` ~1/s
(`portal/system/periodic.rs:6`) -> `LiveMetrics` -> `update_metrics` -> 5 min `drain()` flush -> `summarize_usage` ->
Billing `overage.rs` (monthly average vCPU/GB, total egress).

- [ ] **W3.1 [B] Report cumulative counters, not rates.** `aggregator.rs:243-250` `Metrics` and `:300-307` `LiveMetrics`:
  add `cpu_usec_total: u64`, `mem_byte_seconds_total` (or sample + sample time), keep `rx_bytes`/`tx_bytes`, add
  `counter_epoch: u64` (bumped when the counter source restarts). Remove `cpu_usage` percent from billing inputs (keep for
  UI). Sources: W4.4.
- [ ] **W3.2 [B] `update_metrics` takes deltas.** `aggregator.rs:572-634`: delta per `(project_id, instance_id)` with reset
  handling on `counter_epoch` change or decrease (same pattern as the rx/tx code at :605-629). Drop the elapsed-time
  weighting from `7c44282`.
- [ ] **W3.3 Stop losing state at flush.** `aggregator.rs:643-668` and `:672-710` `map.drain()` also clears the baseline and
  `last_sample_epoch_ms`, so each flush drops one interval of CPU and bytes. Keep baselines; reset only the window totals.
- [ ] **W3.4 Egress bug.** `summarize_usage` (`aggregator.rs:457-539`, rx/tx at :495-498, :521-522) uses `max - min` over
  records, but each record is already a per-flush delta. Must sum. **Affects invoices now; do first.**
- [ ] **W3.5 Group correctly.** Same function labels the summary with `records[0]` (:475-477). Group by
  `(project_id, instance_id)` and return a `Vec` or a map; Billing sums per subscription.
- [ ] **W3.6 [B] Time-correct memory.** `avg_memory = sum(samples)/sample_count` (:515-519) is sample-count weighted. Report
  byte-seconds and `seconds_observed` so Billing can compute true average GB and see coverage gaps (node outages).
- [ ] **W3.7 [B] `UsageRecord` schema v3** (`aggregator.rs:309-351`): counters + `seconds_observed`; v1 stays excluded, v2
  (core-seconds) readable. Reader `load_usage_records_from_dir` (:433-455) silently drops unparseable lines
  (`if let Ok`): log and count them (billing data must not vanish quietly).
- [ ] **W3.8 Dead billing code.** `BillingCosts` (:353-382, hard-coded `instances * 5`) and `portal.rs:586` `BillingParams`:
  Billing replaced `calculate_costs` with `overage::calculate_pool_overage` ([`domain_billing_rollout.md:54`](../platform/notes/domain_billing_rollout.md)). Grep all
  repos; delete if unused.
- [ ] **W3.9 Tests** (`src/tests/aggregator.rs`; today rx/tx are never asserted in summarize): multi-window sum, multi-instance
  grouping, counter reset, flush-boundary continuity, v1/v2/v3 mix, gap in samples.
- Consumer ripple: portal `periodic.rs:168-173` mapping, `v1.rs:816,911` and `usage_cache.rs` (summaries + routes
  `/v1/usage/single/{instance_id}`, `/v1/usage/group/{runner_id}`: rename to `project_id`), watchdog `ledger.rs:477`
  (stores percent samples), Billing `overage.rs` inputs (`PoolUsage`).

---

## W4. Process supervision and monitoring (rebuild)

Files: `src/process_manager.rs` (1076 lines), `src/resource_monitor.rs` (616), tests `src/tests/process_manager*.rs`.
Consumers: watchdog (`main.rs`, `functions/monitoring.rs`, `functions/monitoring/state_refresh.rs`, `pid_persistence.rs`),
`runners/generic_runner` (`child.rs`, `global_child.rs`, `main.rs`), other runner copies.

Findings to fix:

| # | Where | Problem |
|---|---|---|
| a | `process_manager.rs:846-880, 922-971` | "kill pgid" = one `ppid` walk of /proc, SIGTERM, sleep, SIGKILL. Forks during the window and orphans re-parented to init escape. `SupervisedChild::new` uses `independent_process_group=false` (:327) so there is no pgid anyway |
| b | `:946` | `thread::sleep(400ms)` inside async call chain (`ChildLock::kill` :607) blocks a Tokio worker; fixed grace |
| c | `:927-953` | PID reuse between snapshot and kill; no `(pid, starttime)` identity |
| d | `:168-305, 230-241` | `SupervisedProcess` (re-attach by pid) cannot reap; zombie reports as running |
| e | `resource_monitor.rs:323-337, 230-250` | after exit, monitor zeroes values and returns `Ok`; watchdog sees "healthy" forever |
| f | `process_manager.rs:667-674` | `running()`: lock timeout = alive, other error = dead |
| g | `:775` | `kill_on_drop(true)` when not independent: dropping the handle kills the app (watchdog crash = outage?) vs re-attach design |
| h | `resource_monitor.rs:343-360, 519-538` + `watchdog/functions/monitoring.rs:563-640` | only root pid measured; tree fallback averages children, uses lifetime CPU, excludes root, only runs when root CPU is 0 |
| i | `process_manager.rs:421-492, 1024-1076` | stdio monitor takes the child write lock and retries forever; partial lines/`\r` |
| j | `resource_monitor.rs:577-616` | `get_system_stats` "LEGACY", wrong units (`/1024000`, `trim_end_matches('0')`) |
| k | `aggregator.rs:782-784` | `pub unsafe fn to_string` with `unwrap_unchecked` on `AppStatus` |

- [ ] **W4.1 [B] One handle type.** `Supervised` with `Owned(Child)` / `Attached{pid, starttime}`; replaces `SupervisedChild`,
  `SupervisedProcess`, `spawn_simple_process` (:695, writes `AppState`) and `spawn_complex_process` (:756). Identity is
  `(pid, starttime)` everywhere (c, d).
- [ ] **W4.2 [B] Async stop.** `async fn stop(&self, grace: Duration) -> StopOutcome`: SIGTERM -> wait via `pidfd`/`try_wait`
  with timeout -> SIGKILL; no sleeps (a, b). Grace comes from config (default 5 s).
- [ ] **W4.3 [B] Containment.** Primary: new session + `killpg` (spawn with `setsid`), then cgroup `cgroup.kill` when the app
  has a cgroup (watchdog sandbox plan C17: [`CONTRACTS.md`](../platform/plans/per-app-2026-10/watchdog/CONTRACTS.md)). `/proc` ppid walk stays as cleanup only.
- [ ] **W4.4 [B] Sampler.** One per-node sampler reading all supervised apps, producing the W3.1 counters:
  cgroup v2 `cpu.stat usage_usec`, `memory.current`/`memory.peak` when a cgroup exists; else sum `utime+stime+cutime+cstime`
  over the subtree and RSS. Publishes a `ResourceSample` per app on a `watch` channel. Keeps `MonitorWatchdog`
  (`resource_monitor.rs:23-148`) health semantics. Removes `aggregate_tree_usage`/`backfill_tree_usage_metrics` hack (h).
  Note: watchdog plan task [`SBX-007b`](../platform/plans/per-app-2026-10/watchdog/tasks/SBX-007b.md) currently converts the cgroup counter back into a percent; change it to export the counter.
- [ ] **W4.5 [B] Liveness enum.** `ProcessState {Running, Exited(status), Zombie, Gone, Unknown}` from one function; monitor
  stops and records `exited_at` rather than reporting zeros as success (e, f).
- [ ] **W4.6** Decide supervisor-death semantics (D5, item g) and make `kill_on_drop` explicit config.
- [ ] **W4.7 `StreamCapture`** for stdout/stderr with its own tests (i); read pipes without holding the child lock.
- [ ] **W4.8** Remove `unsafe fn to_string` (k). `get_system_stats` (j) is **kept** (the Welcome app calls it,
  `Platform/Welcome/src/main.rs:2,11`) and handled in W10.3: moved out of `resource_monitor.rs`, behavior unchanged, documented.
- [ ] **W4.9 Tests first** (existing `tests/process_manager.rs` is 378 lines with no fork/reparent case): `sh -c 'sleep & sleep & wait'`
  grandchildren, double-fork daemon, stalled stdout reader during stop, simulated pid reuse, exit detection, re-attach
  after supervisor restart, zombie vs gone.
- Migration order for consumers: new types alongside old (re-export old names as thin wrappers) -> migrate watchdog and
  generic_runner -> delete wrappers. Dead runner copies (`runners/18da5793_runner`, `old-63-runner`, `fe518f53_runner`,
  `d8392822_runner`, `artisan_runner`) should be archived rather than ported.

---

## W5. Rework AppConfig, the Environment system and AppState (keep `runtime.acai` + TOML)

This replaces the old identity/state/config reorg plan (`docs/implementation-plan-identity-reorg.md`,
`docs/migration-plan-identity-state-config.md`, partly built on branch `new_identity`) with one design in taxonomy vocabulary
(`project_id`/`instance_id`, not `workload_id`/`source_id`; D4). Its useful part is the split into spec / state / snapshot; this
section does that split on top of what the bundle work (`Enviornment_V2`, `runtime_bundle`) already half-did.

### W5.0 How it works today (verified in code)

On a node an app can have, at once: `Config.toml` (`[app_specific]`, read by the runner's own `AppSpecificConfig`,
`generic_runner/src/config.rs:~215`), `Overrides.toml` (the library `AppConfig`, loaded by `AppConfig::new()` through the `config`
crate with `Settings.<RUN_MODE>.toml` and `APP__*` env vars, `config.rs:95-136`), a `.env`, an encrypted `Enviornment_V1` file, a state file
`/opt/artisan/tmp/.<app_name>.state` (`AppState`, which embeds an `AppConfig`), and the bundle `runtime.acai` holding `runtime.toml`
(`Enviornment_V2`), `custom.json` (`CustomConfig`) and `.env` (`runtime_bundle.rs:29-34`). Watchdog exposes five editable "kinds" over its
proto (`ConfigFileKind` 1-5: Config, Overrides, Runtime, Custom, BundleEnv, `watchdog/src/functions/config_files.rs:46-86`) and migrates
legacy apps into bundles at boot (`runtime_bundle_lifecycle.rs`, `migrate_app_to_bundle`, merge logic in
`build_fixed_config_from_legacy_files` `:604`). Portal still *generates the legacy pair* for new deploys
(`portal/src/api/handler/repos.rs:284` `render_config_toml`, `:319` `render_overrides_toml`).

| # | Problem | Evidence |
|---|---|---|
| P1 | `Enviornment_V2` is one flat 25-field struct mixing six concerns: identity, limits, logging, legacy service sub-configs, the runner's rebuild-on-change loop, and build/run | `enviornment/definitions.rs:341-370` |
| P2 | The same config exists three times: `AppConfig`, `AppState.config`, `ApplicationConfig.config`, kept in sync by hand | `config_bundle.rs:11-16, 72-75`; runner `generate_application_state` re-copies fields into the loaded state; Manager `system/config.rs:65-67,120`; GitMonitor `application/config.rs:95-98` |
| P3 | Every consumer rebuilds `AppConfig` field by field from V2 | Manager `config.rs:25-34`, GitMonitor `config.rs:50-59`, generic_runner `config.rs` `load_runtime_v2` |
| P4 | Dead or misplaced fields: `database` is set to `None` by every consumer (generic_runner `get_config`, Manager `:42`, GitMonitor `:72`) but defaults to `postgres://user:password@localhost/dbname` (`config.rs:108-109`); `aggregator` is the ancient socket; `git` is real but only for the two system apps Manager and GitMonitor, yet portal's deploy template writes it for every customer app (`repos.rs:325-328`) | as listed |
| P5 | "Environment" means three things: the deployment scope (`AppConfig.environment: String`, taxonomy `EnvironmentId`), env vars (`.env`), and the `Enviornment*` build/run struct | `config.rs:34`, `runtime_bundle.rs:33`, `definitions.rs:56` |
| P6 | Limits: `max_ram_usage: usize` (MB) and `max_cpu_usage: usize` (no unit) in three structs, `validate()` is wrong (`config.rs:151-155`), and watchdog's sandbox policy has its own limits (`memory_max_mb`, `cpu_max_millicores`, [`CONTRACTS.md`](../platform/plans/per-app-2026-10/watchdog/CONTRACTS.md) C2): two sources of truth | |
| P7 | Secrets in config: V1 JSON carries `secret_id`/`secret_passwd`/`env_key_0` (self-keyed encryption, W10); `custom.json` carries platform keys `secret_server_addr`/`env_file_location` (`generic_runner/config.rs:~50-62`) that V2's own doc calls obsolete | `definitions.rs:149-161` |
| P8 | Wire weight: `AppStatus.app_data: ApplicationConfig` (`aggregator.rs:745-764`) ships full config + full state, including two 500-line stdout/stderr buffers, on every poll (~1/s per app); portal then reverses and truncates them (`portal/system/periodic.rs:156-160`) | |
| P9 | `AppState` mixes concerns: `config`, `system_application` (a property of the app's kind), free-form `data: String`, and the log buffers, all inside the encrypted file rewritten on every `update_state` | `state_persistence.rs:25-60, 308+` |
| P10 | Layering got lost: legacy had base (`Config.toml`) < operator (`Overrides.toml`) < env vars; the bundle has one file and no sanctioned local override, so an operator edit to `runtime.toml` is overwritten by the next portal deploy | `runtime_bundle.rs:142-150` |
| P11 | Taxonomy violations in consumers: `config.app_name.to_string().replace("ais_", "")` in generic_runner `config.rs` (twice in `generate_application_state`); `generic_runner/src/runner_environment.rs:77-78` **prints** `simple_encrypt(&bytes)` of the environment file to stdout (a leftover debug line, variable named `stupidity`) | |

### W5.1 Target model: split by who writes it and how often it changes

| Artifact | Written by | Changes | Lives in | Type |
|---|---|---|---|---|
| **Spec** (desired config) | portal / watchdog / operator | per deploy | bundle entry `runtime.toml` | `RuntimeSpec` (new, replaces `AppConfig`, `Enviornment*`, runner `AppSpecificConfig`) |
| **Custom** (app-owned freeform) | the app / operator | rarely | bundle entry `custom.json` | `CustomConfig` (unchanged) |
| **Secrets** | secret server / watchdog | rarely | bundle entry `.env` | unchanged content; add a small `EnvFile` parser so nobody hand-splits `KEY=value` |
| **State** (observed) | the supervisor | every second | state file | `RuntimeState` (new, replaces `AppState`) |
| **Service config** (per process, not per app) | operator | rarely | the service's own repo | not in this library: DB URL, aggregator socket, git credentials file |

Kept as is: the `.acai` container, the entry names, `build_bundle`/`read_*`/`commit_*`/`unpack_control_plane_config`/`cleanup_unpacked_config`
(plaintext only while the process runs, `runtime_bundle.rs:165-201`). Changed: the *schema* of `runtime.toml` (versioned) and the Rust
types behind it.

### W5.2 `RuntimeSpec` (nested tables, versioned, `runtime.toml`)

```toml
schema = 3

[identity]
project_id     = "63c35f4b"          # ProjectId
environment_id = "prod"              # EnvironmentId (was `environment: String`)
display_name   = "my-app"            # optional label only; project_id is the key (app_name is gone)
kind           = "customer"          # customer | system  (was AppState.system_application)

[source]                             # how code gets built
project_path      = "/var/www/ais/63c35f4b"
install_command   = "npm ci"
pre_build_command = ""
build_command     = "npm run build"
path_modifier     = ""

[run]
command = "npm start"
runtime = { kind = "node", version = "20" }   # replaces ApplicationType; carries the "runtime detection" [PORTAL_API](../platform/api/PORTAL_API.md) §6 #13 defers to v8.0.0
uid  = 1001
gid  = 1001
port = 20017                         # or "auto" (watchdog port allocator, [CD-WD-1](../platform/plans/per-app-2026-10/watchdog/tasks/CD-WD-1.md))

[limits]
memory_mb            = 512
cpu_millicores       = 1000          # optional; set from the plan. `max_cpu_usage` (no unit, never enforced) is gone
source               = "plan"        # plan | override
overage_acknowledged = false         # the other v8.0.0 deferral: plan-derived limits + acknowledgement

[watch]                              # the runner's rebuild-on-change loop (was Config.toml [app_specific])
monitor_path     = "/var/www/ais/63c35f4b"
interval_seconds = 30
changes_needed   = 1
ignored_subdirs  = [".git", "node_modules"]

[logging]
level = "Info"                       # Debug/Trace here replaces debug_mode
```

- [ ] **W5.2a [B]** New `spec` module: `RuntimeSpec { schema, identity, source, run, limits, watch, logging }`, each table `#[serde(default)]`
  except `identity` and `run.command`; unknown keys ignored (forward compatible) but reported by `validate()` as warnings.
- [ ] **W5.2b `validate() -> Vec<SpecIssue>`** returning *all* problems (replaces `AppConfig::validate`, which returns the first and has two dead checks):
  absolute paths, `uid != 0` for customer apps, port range, limits > 0 and within the plan, `run.command` non-empty.
- [ ] **W5.2c** `run.runtime` is an open `RuntimeKind` (`node|python|go|rust|ruby|static|custom` plus a `version` string); replaces
  `ApplicationType` (`Simple|Next|Angular|Python|Custom`), which names frameworks, not runtimes.
- [ ] **W5.2d Overrides partition inside the bundle (your call; replaces `Overrides.toml` and `Settings.<mode>.toml`).**
  Rule: *base* = `runtime.toml`, written by portal/watchdog on deploy; *overrides* = a separate, sparse partition written only by operator
  tooling. A redeploy rewrites the base and **never touches the overrides**. Effective spec = defaults < base < overrides
  (< `APP__SECTION__KEY` env vars, debugging only, optional). Library API (new `spec::overrides`):
  - `read_overrides(bundle, pass) -> SpecOverrides` (a sparse `toml::Table`), `set_override(bundle, pass, "limits.memory_mb", value, meta)`,
    `clear_override(bundle, pass, key)`, `effective_spec(bundle, pass) -> EffectiveSpec` where every field carries its **provenance**
    (`Default | Base | Override{by, at, reason}`) so the dashboard can show "overridden by operator";
  - an allowlist of overridable keys (`logging.*`, `limits.*`, `watch.*`, `run.port`; never `identity.*`, `source.*`, `run.command`) and validation of the *merged* result, so an override cannot produce an invalid spec;
  - optional `expires_at` per override (a debugging `level = "Trace"` should not live forever), and an audit record (who/when/why).
  **Where the partition lives (D22; the `read_state` precedent you pointed at is `acai_core::reader::read_state_tlv`, used by `apostle_client::MailBundle::load` for the identity secret):**
  | Option | How | Cost |
  |---|---|---|
  | **A. Reserved path prefix (recommended)** | entry `overrides/runtime.toml` (+ `overrides/meta.json`), written with the existing `commit_to_container` upsert (`runtime_bundle.rs::commit_entry`) | Works with `acai_core` as is. Encrypted and AEAD-authenticated like every other entry. Each edit appends a chunk (container growth, index region is 1 MiB, `DEFAULT_INDEX_REGION_SIZE`), fine for operator-rate edits |
  | B. Private State Section TLV | a type in the app range `0xC000-0xFFFF` (spec "Reserved ranges"), read with `read_state_tlv`, like `LEDGER_SECRET_TLV_TYPE` | Needs a new `acai_core` API: a `Changeset` field to set/clear a private TLV (today commits only add/remove files and rewrite `CONTAINER_PROMISES`; extra TLVs are preserved, `commit.rs:638-678`). **TLV values are not encrypted** (`read_state_tlv` takes no passphrase), so overrides become plaintext and are covered only by the state hash, not AEAD. Acceptable only if overrides are never secret; the allowlist above already excludes secrets |
  Recommendation: build A now (no `acai_core` change), keep the `overrides` API storage-agnostic so B can replace it later if you add TLV writes to `acai_core`.
  - Wire: reuse watchdog's `ConfigFileKind::Overrides` (=2, `config_files.rs:47-48,82-86`) for this partition, so the proto and dashboard keep working; `Config` (=1) goes away after the migration (W5.5).
  - Env-var override (`with_env_overrides()`) stays for local debugging, and it removes the `config` crate dependency.
- [ ] **W5.2e Compatibility reader.** `RuntimeSpec::from_toml` accepts schema 3 and today's flat `Enviornment_V2` keys (a `from_v2_flat` path), so every
  existing bundle still loads; watchdog rewrites to schema 3 on the next `commit_fixed_config`. No flag day.
- [ ] **W5.2f Move the legacy merge into the library.** `RuntimeSpec::from_legacy(config_toml, overrides_toml)` takes watchdog's
  `build_fixed_config_from_legacy_files` / `sanitize_fixed_table` / `default_fixed_config_table` logic and its tests, so there is one tested migration instead of one in watchdog;
  `from_v1(bytes)` imports `Enviornment_V1`, routing `secret_id`/`secret_passwd`/`env_key_0` into the returned `.env` text, never the spec.

### W5.3 Field disposition

`AppConfig` (`config.rs:15-52`):

| Field | Goes to |
|---|---|
| `app_name` | **dropped** (your call). `identity.project_id` is the key; optional `identity.display_name` label for UIs |
| `max_ram_usage` | `limits.memory_mb` |
| `max_cpu_usage` | **dropped** (your call; it has no unit and only feeds a broken `validate()`). Replaced by the *new* optional `limits.cpu_millicores`, filled from the plan/sandbox policy, nothing migrates from the old value |
| `environment: String` | `identity.environment_id` |
| `debug_mode` | **dropped** (your call); `logging.level = "Debug"` does what `debug_log_set` did |
| `log_level` | `logging.level` |
| `git: Option<GitConfig>` | **out of the per-app spec.** Used only by Manager and GitMonitor: move to their `custom.json` (`custom.get::<GitConfig>("git")`); keep the struct in the `git` module |
| `database: Option<DatabaseConfig>` | **drop** (every consumer sets it to `None`) |
| `aggregator: Option<Aggregator>` | **drop** (the socket mechanism is gone) |

`Enviornment_V2` (`definitions.rs:341-370`): `app_name/environment/debug_mode/log_level/max_*` as in the table above; `interval_seconds/monitor_path/changes_needed/ignored_subdirs` -> `[watch]`;
`project_path/install_command/build_command/pre_build_command/path_modifier` -> `[source]`; `run_command/application_type/execution_uid/execution_gid/primary_listening_port` -> `[run]`;
`git/database/aggregator` as above.

`Enviornment_V1`: import-only (W5.2f); then delete the type, the `Enviornment` V1/V2 enum, `VERSION_TAG_V1/V2/V3` and the `parse/encrypt/parse_to/parse_from` methods [B].

Runner `AppSpecificConfig` (generic_runner `config.rs`): delete; the runner reads `RuntimeSpec`. `secret_server_addr`/`env_file_location` are dropped (the bundle supplies `.env`, V2's own comment says the same).

### W5.4 State and wire: `RuntimeState`, `AppStatus` slim, `ApplicationConfig` removed

- [ ] **W5.4a [B] `RuntimeState`** replaces `AppState`: `{schema, project_id, instance_id (W2.6), spec_digest, version: SoftwareVersion, status, pid, started_at, last_updated, event_counter, error_log (capped, e.g. 50), note: Option<String> (was data)}`.
  No `config`, no `system_application` (now `spec.identity.kind`), no stdout/stderr.
- [ ] **W5.4b Logs leave the state file.** stdout/stderr live in the supervisor's `StreamCapture` (W4.7) and are fetched on demand (the `GetLogs`/historical-logs path already exists in watchdog's ledger). The state file stops being rewritten with 1000 log lines every update.
- [ ] **W5.4c Old state files still load.** `StatePersistence::load_state` ignores the removed fields (serde default), so a 7.x node upgrades in place; first save writes the new shape. Fix `get_state_path` (`state_persistence.rs:267`) to key off `project_id`/`instance_id` instead of `app_name` (D3).
- [ ] **W5.4d [B] Slim `AppStatus`** (`aggregator.rs:745-764`): `{project_id, instance_id, status, expected_status, uptime, metrics (W3 counters), spec_digest, recent_errors (<= 10), timestamp}`.
  Delete `ApplicationConfig` (`config_bundle.rs`) and its getters. The dashboard fetches the spec via the existing `GetConfigFile(Runtime)` call, cached by digest. Remove portal's reverse/truncate code (`periodic.rs:156-160`).
- [ ] **W5.4e** `update_state`, `log_error`, `wind_down_state` (`state_persistence.rs:308-368`) keep their behavior, retargeted at `RuntimeState`. `debug_log_set` (`:361-368`) is deleted with `debug_mode`; callers set the level from `spec.logging.level`.

### W5.5 Migration order (no flag day)

1. Add `spec` + `RuntimeState` next to the old types, with `From<Enviornment_V2>` and golden tests built from **real bundles and real legacy `Config.toml`/`Overrides.toml` files** copied off a node.
2. Move the legacy merge into the library (W5.2f); watchdog switches to calling it and deletes its copy; its existing tests move too.
3. `runtime_bundle` read/commit/build take `RuntimeSpec` [B]; flat V2 still readable.
4. Consumers: generic_runner (delete `AppSpecificConfig`, `load_runtime_v2`'s hand copy, `runner_environment.rs` print), Manager, GitMonitor, `Security`, `aggregator`, `project_Kaelum` (their `get_config()` becomes `RuntimeSpec::load()`); watchdog `monitoring.rs`/`state_refresh.rs` read `RuntimeState`.
5. Portal stops rendering `Config.toml`/`Overrides.toml` for new deploys: it builds a `RuntimeSpec` and asks watchdog to create the bundle directly; `ConfigFileKind::Config` stays read-only for one release then is removed from the proto (needs `ais_proto`); `ConfigFileKind::Overrides` is kept and repointed at the bundle overrides partition (W5.2d).
6. Delete: `AppConfig`, `Aggregator`, `DatabaseConfig`, `Enviornment*`, `config.rs` loader, `config` crate dependency.

### W5.6 Tests

Schema round trip; flat-V2 -> schema 3 equivalence; every legacy `Config.toml`/`Overrides.toml` combination watchdog's tests already cover; `validate()` table tests; override partition: a redeploy rewrites base and leaves overrides intact, provenance reports Default/Base/Override, an override outside the allowlist or producing an invalid merged spec is rejected, expiry removes it, effective spec matches the legacy Config+Overrides merge for the watchdog test fixtures; env override precedence; old state file (with embedded `config` and logs) loads into `RuntimeState`; `AppStatus` JSON size before/after for a chatty app (expect two orders of magnitude smaller); a bundle written by 8.0 is rejected cleanly (not mis-parsed) by a 7.x reader.

### Decisions specific to W5

- D22. (Decided: an overrides partition inside the bundle.) Remaining choice: storage backing A (reserved path prefix, recommended, no `acai_core` change) vs B (private State Section TLV, needs a new `acai_core` commit API and is plaintext). Also: which keys are overridable.
- D23. (Reduced: `max_cpu_usage` is dropped.) The new optional `limits.cpu_millicores` is plan-derived; confirm the plan catalog stores CPU in a unit that converts cleanly (Billing bills vCPU average, `overage.rs`).
- D24. (Decided: yes.) System apps (Manager, GitMonitor, aggregator, Security, Kaelum) move onto the same bundle + `RuntimeSpec`; their service-specific settings (e.g. Manager/GitMonitor git credentials file) go in `custom.json`. Watchdog today only *consumes* bundles for `generic_runner` apps, so unpacking before spawn must extend to system apps.
- D25. Persist app logs at all across restarts, or only in the watchdog ledger?

---

## Decisions

- D5. Supervisor death: should apps survive a watchdog crash (re-attach design, recommended) or die with it
  (`kill_on_drop`)? Changes W4.6.
- D6. cgroup v2 guaranteed on every node? If yes drop the /proc subtree fallback in 8.1.
- D7. Memory billing basis: cgroup `memory.current` (recommended) vs summed RSS (double-counts shared pages).
- D8. Grandfathered v1/v2 usage records: exclude (current) vs re-rate.
- D13. (Resolved into W5: `AppConfig` is replaced by `RuntimeSpec`; remaining questions are D22-D25.) The blocker list is the consumer migration in W5.5.

(D22-D25 are listed inside W5 above.)

## Verification

- Process: run the W4.9 scenarios under `strace -f`/`ps` to confirm no survivors; soak test: spawn and kill 1000 trees, no zombies, no leaked tasks.
- Billing: replay a captured day of `usage-*.jsonl` through old and new `summarize_usage`, diff egress/CPU; a synthetic constant-load process (e.g. `stress-ng -c1`) over 10 minutes reads 600 core-seconds within about 1%.
- Config/state: see W5.6; also load every real bundle and legacy `Config.toml`/`Overrides.toml` pair copied off a node, and compare `AppStatus` size before and after.
