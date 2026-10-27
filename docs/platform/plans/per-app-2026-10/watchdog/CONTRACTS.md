# Frozen contracts: one-week self-serve plan (shadow_clone edition)

Every task brief points here. **Do not change a contract inside a task.** If a
contract is wrong, stop and raise it with the plan owner. A changed contract
means changing every task that consumes it.

Ids: a project id is the 8-hex `GitAuth::generate_id()` (for example `ab12cd34`). The
app name is `ais_<id>`.

---

**Reading this file.** Each `## C<n>.` section is pasted whole into the prompt of every task that lists it in
`plan.yaml`, so each is self-contained. C8 and C16–C18 were one section in the original plan; they were split so a
task only carries the part it needs. C19 is filled in by SPIKE-002. C20 pins one proto field.

---

## C1. Node file layout (new)

| Path | Owner / mode | Written by | Read by | Content |
|---|---|---|---|---|
| `/opt/artisan/etc/git_tokens/` | root:root `0700` | Manager | Manager, GitMonitor | directory |
| `/opt/artisan/etc/git_tokens/<id>` | root:root `0600` | Manager (`GitTokenSet`), atomic tmp+rename | Manager, GitMonitor | One line: the token, nothing else. An **empty file means anonymous** (no auth header). |
| `/opt/artisan/etc/policy/` | root:root `0700` | watchdog | watchdog | directory |
| `/opt/artisan/etc/policy/ais_<id>.toml` | root:root `0600` | watchdog `SetAppPolicy` only | watchdog | Policy, see C2 |
| `/opt/artisan/etc/policy/ais_<id>.uid` | root:root `0600` | watchdog (allocated once, reused forever) | watchdog | `2000017\n` (uid = gid) |
| `/var/lib/ais/apps/<id>/work` | `<uid>:<uid>` `0700` | watchdog before every sandboxed start | runner (inside sandbox at `/app`) | the writable workspace |
| `/var/lib/ais/apps/<id>/home` | `<uid>:<uid>` `0700` | watchdog | app (inside at `/home/app`) | HOME, npm/pip/go caches |
| `/opt/artisan/tmp/.ais_<id>.state` | `<uid>:<uid>` `0600`, pre-created by watchdog | runner (`fs::write`, truncates in place) | watchdog | runner state, unchanged format |
| `<cgroup_root>/apps/ais_<id>/` | root | watchdog | watchdog | cgroup v2 dir, see C8 |

The sandbox UID range is **2,000,000 – 2,099,999**, one uid per app, with gid = uid. There is
no `/etc/passwd` entry. Allocation uses the lowest uid not used by another `*.uid` file,
under a process-wide mutex. This is the same pattern as `watchdog/src/functions/ports.rs`.

## C2. Policy file (`/opt/artisan/etc/policy/ais_<id>.toml`)

```toml
version = 1
sandbox = true            # the presence of this file with sandbox=true is what makes an app sandboxed
runtime = "node"          # node | static | python | go | rust | ruby | custom
runtime_version = "22"    # optional. node: "20" | "22" (default 22). Others: informational only
memory_max_mb = 512       # cgroup memory.max
cpu_max_millicores = 250  # cgroup cpu.max = "<millicores*100> 100000"
pids_max = 512            # cgroup pids.max
tmp_max_mb = 256          # size of the sandbox /tmp tmpfs (counts toward memory)
```

- Only Portal's customer-deploy path and Super write it, through `WatchdogSetAppPolicy`.
- It is **never** reachable through the customer-editable config route
  (`/v1/runner/{id}/config`, kinds `config` and `overrides`).
- Watchdog refuses unknown keys, `version != 1`, and values outside these bounds:
  memory 128–16384, cpu 50–8000, pids 64–4096, tmp 16–2048.
- Customer-editable `Config.toml` must never weaken the sandbox. Mounts, uid and limits
  come from the app id and this file only. Nothing in them is taken from `Config.toml`
  (`project_path`, `monitor_path` and the like).

## C5. watchdog.proto additions

These are identical in `watchdog/proto/watchdog.proto` and `Manager/proto/watchdog.proto`.
Append only, and never renumber.

```proto
rpc SetAppPolicy(SetAppPolicyRequest) returns (SetAppPolicyResponse);
rpc RemoveApplication(RemoveApplicationRequest) returns (RemoveApplicationResponse);

message SetAppPolicyRequest { string application = 1; string policy_toml = 2; }
message SetAppPolicyResponse { bool ok = 1; string message = 2; uint32 uid = 3; }
message RemoveApplicationRequest { string application = 1; }
message RemoveApplicationResponse { bool ok = 1; string message = 2; repeated string removed = 3; }
```

## C8. Sandbox launch and runtime (watchdog → runner)

- Launch shape: watchdog spawns `current_exe() __sandbox-init --app ais_<id>`. That launcher is
  watchdog's direct child, the pid watchdog supervises.
  1. The launcher creates and joins the cgroup (C17).
  2. It calls `unshare(CLONE_NEWNS|CLONE_NEWPID|CLONE_NEWUTS|CLONE_NEWIPC|CLONE_NEWCGROUP)`.
  3. It forks **init** (PID 1 in the namespace). Init builds the mount tree (C16, pivot_root), then forks
     the **runner**: setgroups([]), setgid/setuid(uid), clear capabilities, `PR_SET_NO_NEW_PRIVS`,
     seccomp, then exec `/opt/artisan/bin/ais_<id>` with cwd `/opt/artisan/etc/ais_<id>`.
  4. Init reaps zombies and forwards signals. The launcher forwards SIGTERM/SIGINT/SIGHUP/SIGUSR1
     to init, and init forwards them to the runner.
  5. Exit: the launcher moves itself back to the cgroup it started in, writes `1` to the app
     cgroup's `cgroup.kill`, removes the cgroup, and then exits with the runner's exit status
     (128+sig if init was killed by a signal). Sandbox setup failure exits 70 after printing
     `[artisan] platform could not start your app (sandbox: <reason>)` to stderr.

  *(Deviation from "runner is PID 1": a tiny init is PID 1 so zombies are reaped and signals behave.
  The runner is PID 2 and the parent of every app process.)*
- **Environment** (added on top of the existing bundle env): `AIS_SANDBOX=1`, `AIS_WORKSPACE=/app`,
  `HOME=/home/app`, `PORT`, `PATH` (by runtime, C9), `AIS_RUNTIME`, `AIS_RUNTIME_VERSION`, and the
  bundle secrets exactly as today.
- **Runner exit codes**: `75` means "my checkout was replaced, restart me". Watchdog restarts on any exit, as it does today.
- **Diagnostics**: the runner pushes human-readable lines prefixed `[artisan] ` into `state.stderr`.
  Portal shows the newest of these as the failure message.
- **Module interface inside watchdog** (so the sandbox tasks compile against each other):
  `sandbox::init::SandboxCtx { app: String, id: String, uid: u32, policy: AppPolicy }`,
  `sandbox::init::run_init(ctx: &SandboxCtx) -> !` (runs as PID 1: mounts, fork runner, reap),
  `sandbox::init::run_runner(ctx: &SandboxCtx) -> !` (drops privileges, seccomp, exec),
  `sandbox::seccomp::install() -> Result<(), String>`, `sandbox::launch::run() -> !` (the launcher; watchdog's `main` calls it
  before starting tokio when `argv[1] == "__sandbox-init"`).

## C9. Runtime → PATH (watchdog)

| runtime | PATH prefix (before `/usr/local/bin:/usr/bin:/bin`) | extra env |
|---|---|---|
| node, static (default) | `/opt/runtimes/node/<20\|22>/bin` (default 22) | `NODE_ENV=production`, `npm_config_cache=/home/app/.npm` |
| python | `/app/current/.venv/bin` (each release creates its own `.venv`) | `PIP_CACHE_DIR=/home/app/.cache/pip`, `PYTHONUNBUFFERED=1` |
| go | `/opt/runtimes/go/bin` | `GOPATH=/home/app/go`, `GOCACHE=/home/app/.cache/go-build`, `GOTOOLCHAIN=auto`, `CGO_ENABLED=0` |
| rust | `/opt/runtimes/cargo/bin` | `CARGO_HOME=/home/app/.cargo`, `CARGO_TARGET_DIR=/app/target` |
| ruby | `/usr/bin` | `BUNDLE_PATH=/app/vendor/bundle`, `GEM_HOME=/home/app/.gem` |

Toolchains live under `/opt/runtimes/` (world-readable). This is installed by OPS-LANG-004.

## C16. Sandbox mounts

The new root is a tmpfs, so nothing is visible by default. It contains:

- Read-only binds: `/usr`, `/lib`, `/lib64` (if present), `/bin` and `/sbin` (symlinks when the host is merged-usr), `/etc`,
  `/opt/runtimes`, the runner binary `/opt/artisan/bin/ais_<id>` (a single file), the app conf dir
  `/opt/artisan/etc/ais_<id>`, and the checkout `/var/www/ais/<id>` (bound twice: at its own path and at `/src`).
- Empty tmpfs over the read-only `/etc` sub-paths `/etc/artisan` and `/etc/ssh`.
- Read-write binds: `/var/lib/ais/apps/<id>/work` → `/app`, `/var/lib/ais/apps/<id>/home` → `/home/app`,
  and the single file `/opt/artisan/tmp/.ais_<id>.state`.
- Fresh mounts: `/tmp` (tmpfs, `tmp_max_mb`, mode 1777), `/proc` (new procfs), `/dev` (tmpfs plus binds of
  null, zero, full, random, urandom and tty, plus `/dev/shm` tmpfs), and `/sys/fs/cgroup`
  (cgroup2, read-only, own subtree via the cgroup namespace).
- Order: make `/` private recursively, mount the tmpfs root, the read-only binds (each remounted
  `ro,nosuid,nodev`), the masks over `/etc`, the read-write binds, the fresh mounts, then `pivot_root`.

Nothing else from the host. Mounts, uid and limits come from the app id and the policy file only (C2), never from `Config.toml`.

## C17. Sandbox cgroup (v2)

- Per-app directory: `<cgroup_root>/apps/ais_<id>/`. `<cgroup_root>` is a watchdog config value
  (`sandbox_cgroup_root`, env override `AIS_SANDBOX_CGROUP_ROOT`); SPIKE-002 decides its default
  (a delegated subtree of watchdog's own cgroup, or `/sys/fs/cgroup/ais`).
- Values written from the policy (C2): `memory.max = memory_max_mb MiB (bytes)`, `memory.swap.max = 0`
  (ignore ENOENT when swap accounting is absent), `cpu.max = "<cpu_max_millicores*100> 100000"`, `pids.max = pids_max`.
- Stats read: `memory.current`, `memory.max`, `cpu.stat` (`usage_usec`), `memory.events` (`oom_kill`), `pids.current`.
- Kill: write `1` to `cgroup.kill`; if the kernel lacks it, SIGKILL every pid in `cgroup.procs`, repeating until empty (at most 20 passes).
- Setup at the root: write `+memory +cpu +pids` to `cgroup.subtree_control` at each level; create `supervisor/`
  (watchdog's own pid is moved into it) and `apps/`, as SPIKE-002's recipe (C19) requires.

## C19. Sandbox recipe (from SPIKE-002)

**TO BE FILLED BY SPIKE-002.** The spike runs the C8/C16/C17 sandbox by hand on the real node kernel and replaces this
text with the exact working command sequence: the cgroup root decision and delegation steps, the mount order including
merged-usr symlinks and per-bind read-only remounts, and anything in C8/C16/C17 that could not be built as written.
SBX-013, SBX-004 and SBX-014 are blocked until this section is real; a worker that sees this placeholder must stop and say so.

