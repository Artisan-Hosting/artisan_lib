# Per-application plans (2026-10)

The one-week v3 plan (`../one-week-2026-10-v3`, 161 tasks, 10 repos, one DB) cut into one shadow_clone project per application, re-written with the lessons of the `sc_trial`, `sc_cambium` and `cambium_test` runs, with the customer-deploy work that lived only on `customer-deploy` branches added as tasks. v3 and v2 are untouched.

**Nothing here has been run against a worker.** The plans lint clean and `tools/check_briefs.py` reports no violations, but that only checks the shape of the tasks (size, touches, named identifiers exist in the base, guards fail on the base), not that a worker can do them.

## Projects

Each folder holds `plan.yaml`, `CONTRACTS.md` (only the contracts its tasks use) and `tasks/<ID>.md`. In the TUI wizard pick the folder's `plan.yaml`.

| Project | Repo | Base | Target | Tasks | Wave |
|---|---|---|---|---|---|
| `ais_proto` | ais_proto | `main` | `release` | 3 | 1 |
| `billing` | Billing | `main` | `release` | 8 | 1 |
| `gitmonitor` | GitMonitor | `release` | `release` | 6 | 1 |
| `runner` | runners/generic_runner (clone dir `generic_runner`) | `release` | `release` | 14 | 1 |
| `ais_auth` | ais_auth | `main` | `release` | 5 | 2 |
| `domain_management` | domain_management | `main` | `release` | 12 | 2 |
| `watchdog` | watchdog | `release` | `release` | 41 | 2 |
| `manager` | Manager | `release` | `release` | 13 (2 human) | 3 |
| `portal_connect` | portal | `main` | `release` | 26 | 3 |
| `portal_deploy` | portal | `main` | `release` | 28 | 4 |
| `dashboard` | dashboard | `main` | `release` | 35 | 5 (can start in wave 3) |

191 tasks: 189 agent, 2 human. Where a repo has no `release` branch the base is `main` and `release` is created from it (Phase 0). Where `release` exists it is both base and target. On 2026-10-14 `origin/release` and `origin/main` have identical trees in GitMonitor and runner; watchdog and Manager `release` are ahead of `main`.

Start gates (a project starts when these are merged to the upstream `release`) are in `gates.yaml`: every dependency on another project or on a human task that was removed from a plan. Hard gates: ais_proto to ais_auth, domain_management and portal; ais_auth `GH-012` to portal_connect; GitMonitor `NODE-GIT-001` and watchdog `SBX-011b`, `CD-WD-6` to manager; portal_connect to portal_deploy. The rest are soft (staging only).

## Customer-deploy work, re-implemented as tasks (`CD-*`)

| Tasks | What |
|---|---|
| `watchdog/CD-WD-1..6` | port allocator for customer apps (`auto_port`, 20000-29999, `PORT`), `/var/www/ais/<id>` checkout path, honest start results, runner build lock, `port` in app status |
| `runner/CD-RUN-1..3` | install/build/run commands run in the app directory, failed starts exit 100 |
| `manager/CD-MGR-1..2` | `port` in the watchdog proto, `WatchdogListBuilds` and `WatchdogGetApplication` verbs |
| `billing/CD-BILL-1a..c` | `GetOrganizationEntitlements` (proto, pure rule in `entitlement.rs`, handler) |
| `dashboard/CD-FE-1` | `forApp` on buy-domain, "Add a project" link |

Not re-implemented: the runner's secret-server client and env wiring (`533f5b5`, and the secrets part of `45d3e7f`). `release` already takes the app environment from the watchdog's runtime bundle (`runner_environment.rs`), so that code is obsolete there. portal, ais_auth and the rest of the dashboard's customer-deploy work are already on `main`.

## Phase 0, before wave 1 (human)

Per repo, after `git fetch --all --prune`:
1. `git format-patch` the `customer-deploy` commits (watchdog `fc984ce` `fce2508`, runner `45d3e7f` `533f5b5`, Manager `7b2a264`, Billing `8d5bdc4`, dashboard's two edits) into `reference/`, kept for review and not given to workers. Tag branches worth keeping as `archive/<name>`, then delete `w1-*`, `backup/*`, `sc-real/*`, `agent/*`, `shadow-clone-trial*` and the Syncthing `*.sync-conflict-*` files and branches. Reset stale local `release` branches to `origin/release` (runner's is from 09-12).
2. Create `release` where missing: `git push origin main:release` for ais_proto, ais_auth, Billing, portal, domain_management, dashboard. For ais_proto first land `session-manager-prefix-sessions` (`ListPrefixSessions`/`pod_prefix`, `ProvisionServiceHost`/`ParkZone` in `domains.proto`) into the base, since DOMAIN-002 edits the same file.
3. `gh repo edit --default-branch release` on all ten (Manager already is), and require a PR on `release`. PRs are drafts; every merge to `release` is prod-bound.
4. Each base must build clean. Fix pre-existing errors first (cambium: one `F821` in an untouched file parked every task).
5. Give each project a unique `pod_prefix` (the session manager reaps unknown prefixes).
6. Sibling build paths: `portal`, `ais_auth` and `Manager` compile `../ais_proto/*.proto`, and four repos use `/opt/artisan/lib/artisan_lib`. A TUI worktree is `projects/<n>/w/sc_<repo>_<task>`, so put a clone of ais_proto (at its `release`) at `$SC_HOME/projects/<n>/w/ais_proto` before the first start, and check `/opt/artisan/lib/artisan_lib` exists. shadow_clone has no sibling-repo support; the wizard could do this later.

Human tasks (`HUMAN.md`): SPIKE-002 (fills C19, before watchdog SBX-004/013/014), OPS-GH-000 (before portal_connect), FE-000 (fills C21, before the dashboard).

## Checking the plans

`python3 -I tools/check_briefs.py` (needs the repos' `origin/*` refs fetched): criteria (at most 6) and touches (at most 2), unobservable rows, `derive` on files over 1,000 lines, identifiers and paths named in "Existing implementation" present in the base, grep guards failing on the base. Six tasks that merged in the old trial keep their larger definitions (`NODE-GIT-001`, `SBX-001`, `RUN-001`, `GH-013`, `DEPLOY-002`, `GH-003`); split them if they fail twice. `AUTHORING.md` has the rules and why. `tools/split.py` and `tools/cd_tasks.py` produced the first cut from v3; the folders were edited by hand after that, so do not rerun them.

## Decisions the revision left open

- **runner RUN-011b**: rows 8-11 refer to `start_flow` and the flow functions without signatures; it also needs `cwd` variants of the child builders without changing `create_child`'s signature (`tests/basic.rs` calls it).
- **runner RUN-002b**: `Enviornment_V2` has no `shell_commands`, so the flag is read from `Config.toml` `[app_specific]`; assumes that file stays next to `runtime.toml`.
- **portal_connect DEPLOY-001a/b** still point at a fake `GhHttp` private to GH-013, which is not in the base.
- **manager** NODE-GIT-002a and SBX-002a are scripted copies that assume GitMonitor's `release` carries `credentials.rs` and watchdog's carries `CD-WD-6` and `SBX-011b`.
- **watchdog**: 014a/b step types, 015 test filters and 016d's wait-for-stop are loosely specified. SBX-000 enables the `nix` `mount` and `sched` features the base lacks.
- **portal_deploy**: new signatures, dry-run line formats and `E2E_DEPLOY_KEY` were the reviser's choices; `CUSTOMER_NODE_IDS` are numeric strings.
- **dashboard**: nothing returns the org slug at deploy time (the auto address falls back to the manual picker); the login page ignores a return path; a plan chosen at signup is lost if the verify link opens in another tab.
- Parent rows dropped or changed in the revision (LANG-001a row 15, the Node half of row 18, LANG-002 token-minting rows now review items, GH-002 cascade row): check them before the first run.

## Before judging the first run

Commit the uncommitted shadow_clone tree (63 files): it holds the fixes for base-compile scoping, the `origin/<ref>` fallback, the Runpod request body, the `run_tests` tool, the "stub the symbols the brief says the task creates" hint, and the Go and Python test languages. Still open in the tool: the red gate accepts an environmental failure as red, a worker can oscillate between two accepted edits, `definition` lookup loops cap the author at 12,288 tokens, and `max-pods: 1` with `concurrency: 2` queues for one pod.
