# Artisan Library Docs

These notes summarize the library API from the existing Rust doc comments and give practical examples.

## Doc set

- [01-runtime-control.md](./01-runtime-control.md): async pause/resume control, supervised processes, watchdog monitoring.
- [02-aggregation-and-messaging.md](./02-aggregation-and-messaging.md): metrics aggregation, app status/messages, portal-facing models.
- [03-state-config-and-environment.md](./03-state-config-and-environment.md): config loading, encrypted state persistence, environment payloads.
- [04-security-identity-and-auth.md](./04-security-identity-and-auth.md): encryption, node identity, API claims/roles/token types.
- [05-git-network-and-ops.md](./05-git-network-and-ops.md): git workflows, DNS resolution, notifications, systemd/user helpers.
- [06-utilities.md](./06-utilities.md): timestamp/version utilities, CLI helpers, historical usage ledger.
- [07-identity-platform.md](./07-identity-platform.md): identity-domain architecture (`node`, `workload`, `runtime`, `authority`) and migration contract.

## Module map from `lib.rs`

- Core runtime: `control`, `process_manager` (Linux), `resource_monitor` (Linux)
- Data flow: `aggregator`, `portal`, `config_bundle`, `historics`
- State/config: `state_persistence`, `config`, `enviornment`
- Security/auth: `encryption`, `identity`, `api::*`
- Ops: `git_actions`, `network` (Linux), `notifications`, `systemd` (Linux), `users` (Linux)
- Utilities: `timestamp`, `version`, `cli`

## Platform notes

Several modules are Linux-only behind `#[cfg(target_os = "linux")]`, especially:
`process_manager`, `resource_monitor`, `network`, `systemd`, and `users`.

## Naming note

The code intentionally uses `Enviornment` / `Enviornment_V1` / `Enviornment_V2` (spelling preserved from source).
