# SBX-007b: cgroup-based metrics and OOM diagnostics for sandboxed apps (part b of 2: refresh-loop wiring)

watchdog · 0.75 h · Rust

## Scope of this part

Part b of 2 of SBX-007. The helpers are `src/functions/cgroup_metrics.rs` (`SBX-007a`); this task wires them into the refresh loop in `src/functions/monitoring.rs` and `src/functions/monitoring/state_refresh.rs`. Nothing is derived; `verify` checks the wiring.

## Task
For sandboxed apps: report CPU and memory from the app's cgroup instead of from the supervised pid, and when `memory.events` shows a new `oom_kill`, add the `[artisan]` diagnostic to the app's stderr output.

## Why
The launcher's pid uses almost nothing, so pid-based usage would read about zero for sandboxed apps (U13). An OOM-killed app needs a reason the customer can read.

## Existing implementation
- `src/functions/monitoring.rs`: `collect_runtime_snapshot_for_process(name, process, check_stdx)` fills a `RuntimeSnapshot { cpu_usage: Option<f32>, memory_usage: Option<f64>, .. }` (memory in MB); `record_runtime_snapshots_in_ledger` puts those snapshot values into the usage ledger and `apply_runtime_snapshot` puts them into the status stores. `once_cell::sync::Lazy` is already imported there.
- `src/functions/monitoring/state_refresh.rs` is a child module of `monitoring`; `refresh_client_statuses_once` calls `collect_process_observations`, whose `ProcessObservations.stderr` is an `Option<Vec<(u64, String)>>` that `merge_state_and_observations` turns into the status's stderr buffer.
- `AppCgroup::stats() -> CgStats { memory_current, cpu_usage_usec, oom_kill_count, .. }` (SBX-003), `read_policy` (SBX-001), `cgroup_metrics::{cpu_percent, oom_line, uses_cgroup_metrics, CpuSample}` (SBX-007a), `definitions::sandbox_cgroup_root()` (SBX-013a).

## Required changes
1. In `src/functions/monitoring.rs`, add two process-wide maps (`Lazy<std::sync::Mutex<HashMap<String, ..>>>`): the previous `CpuSample` per app, and the previous `oom_kill_count` per app.
2. In `collect_runtime_snapshot_for_process`, when `uses_cgroup_metrics(read_policy(name).ok().flatten().as_ref())` is true, read `AppCgroup::create(&sandbox_cgroup_root(), name)` `.stats()`; set `snapshot.memory_usage = Some(stats.memory_current as f64 / 1_048_576.0)` and `snapshot.cpu_usage = Some(cpu_percent(previous, now))`, with `now` built from `stats.cpu_usage_usec` and the wall clock in microseconds; store `now` as the new previous sample. On a stats error keep the pid-based values. Because the ledger and the status stores read the snapshot, both receive the cgroup numbers and the ledger code is not changed.
3. In the same place, compute `oom_line(previous_count, stats.oom_kill_count, policy.memory_max_mb)`, store the new count, and when it is `Some(line)` save the line in a third process-wide map keyed by app (the pending diagnostic), and add `pub(super) fn take_pending_oom_lines(app: &str) -> Vec<String>` that removes and returns the pending lines of one app.
4. In `src/functions/monitoring/state_refresh.rs`, in `refresh_client_statuses_once`, after `collect_process_observations`, append every line returned by `super::take_pending_oom_lines(&name)` to `observations.stderr` as `(current_timestamp_wrapper(), line)`, so the line reaches the stderr buffer that `GetApplication` returns.

## Interface/contracts
C8 (the `[artisan] ` prefix), C17 (stats files).

## Boundaries
- Legacy apps (no policy or `sandbox = false`) are unchanged. No change to Portal, the proto or the ledger.

## Acceptance criteria
None (this task has no derived tests).

## Not tested (checked in review)
- The staging check (`mem-hog` shows the OOM line, `cpu-hog` shows CPU near the plan cap) is STG-SBX step 6.
- A line written to `observations.stderr` is rebuilt each refresh from the process's own buffer, so the diagnostic may scroll out of it; this is noted for the plan owner, not changed here.

## Validation
- `cargo build`
- `grep -q 'uses_cgroup_metrics' src/functions/monitoring.rs`
- `grep -q 'oom_line' src/functions/monitoring.rs`
- `grep -q 'take_pending_oom_lines' src/functions/monitoring/state_refresh.rs`
It must fail before the work and pass after it; the acceptance criteria above are what it has to prove.

## Dependencies
SBX-007a, SBX-006d, SBX-003c, CD-WD-4.

## Outputs
`watchdog/src/functions/monitoring.rs`, `watchdog/src/functions/monitoring/state_refresh.rs`.
