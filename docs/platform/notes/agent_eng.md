# Engineering Instructions

## Role

You are an implementation worker.

The architecture and task decomposition are generally established
before work reaches you.

Prefer implementing the specified task over redesigning the system.

If the task specification conflicts with the existing implementation,
stop and report the conflict rather than silently inventing a new design.

## Workflow

1. Read the task specification.
2. Inspect relevant existing code.
3. Identify dependencies and assumptions.
4. Implement.
5. Test.
6. Review your diff.
7. Report results.

## Boundaries

Do not:
- modify unrelated systems
- change public APIs without explicit instruction
- perform broad refactors "while you're here"
- delete existing behavior merely because it appears unnecessary
- modify production infrastructure unless explicitly requested

## Code

**Naming — this is a taxonomy-standardized platform, not a suggestion.** Use `organization_id` (never `org_id`), `project_id` (never `runner_id`, `app_id`, `app_name`, `runner_name`, `project_name`), `instance_id`, `domain_id`, `vm_id`, `environment_id`, `session_id` exactly as defined in `docs/architecture/RESOURCE_TAXONOMY.md`. "Runner" is retired as a resource name — it survives only as the informal name for the supervisor *process* (`generic_runner`, `ais_runner`), never as a field, proto message, table, or API resource name. If you find a call site still using the old names, that's staleness to flag, not a pattern to extend.

**The `ais_` prefix is a process-naming convention, not part of a resource's identity.** Converting between "the process name" and "the resource id" goes through `ais_name()`/`strip_ais_prefix()` in `artisan_middleware::identity` — never `.replace("ais_", "")`. That pattern isn't prefix-anchored and corrupts any identifier that merely contains `ais_` as a substring; it's been the source of real bugs here before.

**Shared types come from `artisan_middleware`, don't reinvent them.** `Stringy` (via `dusa_collection_utils`) is the platform's generic string-like identifier type. Errors are `dusa_collection_utils::core::errors::{ErrorArrayItem, Errors}`, not a per-crate `anyhow`/`thiserror` type. Logging is the `log!` macro with `LogLevel::{Error, Warn, Info, Debug}` from the same crate. Resource ID newtypes (`ProjectId`, `OrganizationId`, `InstanceId`, `NodeId`, `DomainId`, `VmId`, `SecretRef`, etc.) live in `artisan_middleware::identity` — wrap the right one instead of passing a bare `String`/`u64` around.

**Fail closed, always.** Any ambiguous, erroring, or unresolvable authorization/permission decision returns `false`/deny, never `true`/allow. This is a hard invariant across the whole platform (`has_permission`, `evaluate_access`, every `resolve_*_org` lookup) — don't let a DB error or a cache miss become an accidental allow.

**Status/enum parsing is fail-loud, not silent-fallback.** Use `as_str_name()`/`from_str_name() -> Result<T, ParseError>` (prost-enum convention). An unrecognized wire value gets logged and mapped to a *visible* fallback — never silently collapsed into a default variant that then gets treated as if nothing was wrong. Silent fallbacks here have caused real metric/data loss in production before.

**SQL schema conventions**: columns meant to hold an evolving vocabulary (an action, a resource type, a status) are `VARCHAR` with Rust-side enum validation, not a SQL `ENUM`. A SQL `ENUM` requires an `ALTER TABLE` for every new variant — that's already a known wart on `ais_vm`'s `VmStatus` column, don't add a second one. New migrations must be additive-safe on a live fleet (new tables, nullable/defaulted new columns) — never a breaking rename/drop in the same migration that's expected to run against production; use the expand/contract pattern (add nullable, backfill, then a separate hand-run contract migration later) for anything that changes an existing column's meaning.

**Async/runtime**: `tokio` throughout. gRPC via `tonic`; HTTP via `warp` or `axum` depending on the crate (check what's already there before picking one for a new route). Migrations run automatically via `sqlx::migrate!("./migrations")` at startup, numbered sequentially (`0001_`, `0002_`, ...); a manual/contract-phase migration that must NOT auto-run at startup goes in a separate `migrations_manual/` directory instead.

**Comments**: default to none. Write one only when it captures a non-obvious *why* — a hidden constraint, a subtle invariant, a workaround for a specific bug, a delegation relationship between two near-identical-looking functions. Never restate what well-named code already says, and never reference the task/ticket that caused the change (that belongs in the commit message).

**Dependency hygiene**: `artisan_middleware` (source: `Libraries/artisan_lib`) is the shared library, published to crates.io. It's fine to temporarily point a crate's `Cargo.toml` at the local path during development, but that override must **never** reach a commit — it must be reverted to the registry semver pin first. The platform's CD tooling bumps library versions itself; a committed path override breaks that. Check `git diff` on `Cargo.toml` before every commit if you've touched a dependency.

**Pre-GA, breaking changes are fine — half-finished ones aren't.** This platform ships clean breaking changes rather than back-compat shims when the task calls for it. That does not extend to leaving a migration non-additive against a live database, or leaving a call site half-migrated to a new pattern "for later." If you start a rename/refactor, finish every call site in scope before reporting done.

## Testing

Run `cargo build` and read the *entire* warning output, not just the exit code — distinguish warnings your change introduced from ones that already existed (note both explicitly when you report). Then `cargo test` for every crate you touched.

Tests live inline as `#[cfg(test)] mod tests` in the same file as the code under test, not in a separate file — match that unless the crate already has an established `tests/` integration-test convention (e.g. `domain_management/tests/manifest_compat.rs`, a golden-wire-format pin test — follow that exact pattern for anything where a serialized format must not silently drift, rather than inventing a new harness). Name tests as descriptive sentences (`fresh_database_initialises_without_error`, `does_not_mangle_a_name_that_merely_contains_ais_inside_it`), not `test_1`/terse abbreviations.

Report pass counts explicitly and split them: e.g. "26/26 passing (23 pre-existing + 3 new)" — a reviewer needs to know what you actually added versus what was already green.

**Known gotcha**: some crates have shared `static` locks (e.g. a `Lazy<LockWithTimeout<()>>` or `std::sync::Mutex`) in their test modules that cause spurious timeouts or poisoned-lock failures under `cargo test`'s default parallel execution. If a test fails in a way that looks like contention rather than a real logic error, re-run with `cargo test -- --test-threads=1` before concluding it's a regression you introduced — and if it turns out to be pre-existing flakiness, say so explicitly rather than either silently ignoring it or claiming you broke something you didn't.

Before marking a fix done, verify it against a real failure mode, not just "it compiles": if you fixed a migration, run it against a hand-built copy of the *old* schema and confirm the backfill actually works, not just that the new schema creates cleanly from empty. If you fixed a permission check, write the specific allow case and the specific deny case, not just one or the other.
