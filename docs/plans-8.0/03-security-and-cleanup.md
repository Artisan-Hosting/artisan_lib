# 8.0 plan 03: Security and cleanup

Part of the [artisan_middleware 8.0.0 plan set](README.md). Draft v2, 2026-10-26.

| | |
|---|---|
| **Workstreams** | W7 (hygiene and release mechanics), W8 (retire `notifications`, re-export `apostle_client`), W9 (FIXME/TODO markers and dependencies), W10 (encryption rework, RECS, `get_system_stats`) |
| **When** | W10 and W8 are independent of plans 01-02 and can run in parallel; do W10.2 (stop `encrypt_text` returning an empty string) early. W9 dependency removals land after the code that used them is gone. W7 closes the release. |
| **Depends on** | W8 needs your `apostle_client` commit published first. W9 removals depend on plan 01 W2.6 (`lz4`, `base64`) and plan 02 W5 (`config` crate). |
| **Blocks** | [04](04-refactor-and-docs.md). |
| **Decisions owned** | D10, D11, D15, D16, D17, D18 |

Conventions: `[B]` = breaking API/wire/disk change. `file:line` locations are at commit `7c44282` (before the restructure in [04](04-refactor-and-docs.md) moves files). W-numbers are stable across the plan set; see the [index](README.md).

---

## W10. Encryption module rework; keep RECS (private, experimental); keep and move `get_system_stats`

### What is there today (`src/encryption.rs`, 469 lines)

- Lines 33-384, "Legacy": the RECS API. It is **already a non-functional shell**: `use recs::...` is commented out (`:8`),
  there is no `recs` dependency in `Cargo.toml`, `encrypt_data` returns `UnifiedResult::new(Ok(Vec::new()))` (`:160`-ish) and
  `decrypt_data`'s body is commented out. So today `encrypt_text` (public, `:72`) **returns an empty string without error**.
  Left over: four lowercase statics under `lazy_static!` (`initialized`, `cleaning_loop_initialized`, `cleaning_call`,
  `cleaning_lock`, `:19-31`), `_call_clean`, `_clean_loop`, `_initialize_locker`, `_execution_locked`, `clean_override_op`
  (`pub async unsafe fn`, :314).
- Lines 386-469, "Modern": `generate_key`, `simple_encrypt`, `simple_decrypt`. `simple_encrypt` makes a **fresh random key per
  call and prepends it to the ciphertext**: output = `hex(key[32] || nonce[12] || ciphertext+tag)`. Anyone holding the blob can
  decrypt it, so this gives **neither confidentiality nor authenticity** (an attacker can re-encrypt altered data with a new key);
  GCM only catches accidental corruption. Real protection is the file's permissions. `simple_encrypt` also has an
  `.expect("nonce size mismatch")` (`:419`).
- Callers (all in this crate; no direct use found in `Platform/`, `Software/`, `Tools/`): `cli.rs:25`; `aggregator.rs:1010,1031`
  (registered-apps cache); `git_actions.rs:118,133,183` (**git credentials incl. tokens**); `state_persistence.rs:280,295` (state files);
  `identity.rs:355` (node `Identifier`); `enviornment/definitions.rs:85,172,198,211,381,404,416` (Environment V1/V2 `.env` data,
  which can carry secrets). Test: `src/tests/encryption.rs` (2 tests, no tamper or known-answer test).

### TODO

- [ ] **W10.1 [B] Generic AES-256-GCM API in `encryption`** (caller supplies the key, so it actually provides confidentiality):
  - `generate_key() -> Key256` (moved out of the legacy region, resolves the FIXME at `:395`; fill from the OS CSPRNG, zeroize on drop
    via the `zeroize` crate that `aes-gcm` already pulls in; `Key256` has no `Debug`/`Display`);
  - `encrypt(key: &Key256, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>>` and `decrypt(key, blob, aad)`; blob =
    `version(1) || nonce(12) || ciphertext+tag`, random 96-bit nonce per call (use `Aes256Gcm::generate_nonce`, removing the `expect`);
  - AAD parameter so a blob can be bound to its purpose/owner (for example `"urn:artisan:project:<id>/state"`), preventing a blob
    from one place being replayed into another;
  - hex/base64 text helpers for callers that store strings; typed errors instead of `Errors::InvalidBlockData` for everything;
  - **Key source is deliberately not decided here** (D16). The API takes a key; where it comes from is a separate item.
- [ ] **W10.2 RECS: keep, un-deprecate, make private, label experimental.**
  - Move lines 18-384 into `src/encryption/recs_experimental.rs` (private `mod`, `#![allow(dead_code)]`), delete the
    `#[deprecated(since = "4.3.0")]` attributes and the "Marked as deprecated" doc text, drop `pub` on everything (including
    `clean_override_op`, so the public `unsafe` surface disappears [B]).
  - Replace `lazy_static!` with plain `static` atomics (`AtomicBool::new` and `Notify::const_new` are const) and rename the
    statics to `SCREAMING_CASE`; drops the `lazy_static` dependency.
  - Module doc: **EXPERIMENTAL, NOT FUNCTIONAL.** State exactly what is stubbed (the two functions above, the missing `recs` crate),
    that nothing in the crate may call it, and what repurposing would take.
  - Write-up: `docs/experimental-recs.md` (in this repo) covering what RECS was meant to be, why it was shelved at 4.3.0 ("currently
    unstable" is the only reason in the code), and the interfaces the scaffold implies (cleaning loop, lock/notify, `[data]-[key]-[count]`
    wire format from the commented body of `decrypt_data`). I could not recover the original design intent from the code, so
    **this document needs your input** (D18).
  - Test that the module is not reachable: a `compile_fail` doctest or simply `cargo doc` showing no RECS items.
- [ ] **W10.3 `get_system_stats`: keep, move, document, do not change behavior.** Move `resource_monitor.rs:572-616` to a new
  module (suggest `src/system_stats.rs`, still `#[cfg(target_os = "linux")]`); keep the re-export at its old path for one release or
  update `Platform/Welcome/src/main.rs:2,11` in the same release (D17). Keep `sysinfo` and `gethostname`. Document properly:
  returns a `HashMap<Stringy, Stringy>` of display strings (`CPU Usage`, `Total RAM`, `Used RAM`, `Total Swap`, `Used Swap`,
  `Hostname`) intended only for a login/welcome banner; creates a full `System::new_all()` and refreshes everything on every call
  (expensive; do not call in a loop); `Hostname` is `Debug`-formatted (`"host"` with quotes); and the **known quirks, preserved on
  purpose**: units are wrong (`sysinfo` returns bytes, the code divides by 1024 for "MB" and by 1024000 for "Used"), and
  `trim_end_matches('0')` is applied after the " MB" suffix so it never trims anything. Add a test pinning the key set so a future
  fix is a conscious change. (Fixing the units is an optional follow-up: one line, but it changes what Welcome prints.)
- [ ] **W10.4 [B] Rename and document `simple_encrypt`/`simple_decrypt`.** New home: `src/encryption/self_keyed.rs`, re-exported
  from `encryption`. New names (D15; suggested `self_keyed_encrypt` / `self_keyed_decrypt`; alternatives `obfuscate`/`deobfuscate`).
  Keep the on-disk byte format **unchanged** (`hex(key||nonce||ct)`) so every existing state file, `Identifier`, git-credentials file and
  `.env` keeps loading with no migration. The doc comment must state: the key is stored inside the output; protects only against
  accidental corruption and casual reading; provides no confidentiality and no authenticity; never use for secrets; list the
  current callers. Remove the `expect`. Do not keep `simple_*` as deprecated aliases (8.0 is a clean break), but add a changelog line.
- [ ] **W10.5 Move secret-bearing callers to the keyed API** (needs D16, then per call site):
  - `git_actions.rs:118,133,183` (git credentials / tokens) — highest priority;
  - `enviornment/definitions.rs` (`.env` data) — note secrets are also delivered through `runtime_bundle` / `acai_core`, which is
    already keyed per app; decide whether `Enviornment` encryption should simply defer to the bundle;
  - leave `state_persistence`, `aggregator.rs` cache, `identity.rs`, `cli.rs` on `self_keyed` (non-secret operational data), but
    document that choice at each site;
  - each migration reads the old format and writes the new one (version byte distinguishes them), so a node upgrades in place.
- [ ] **W10.6 Tests** (replace the two in `src/tests/encryption.rs`): AES-GCM known-answer vector; wrong key fails; flipped
  ciphertext bit fails; wrong AAD fails; nonce differs across calls; round-trip for 0-byte, 1-byte and 1 MiB inputs;
  a golden file written by the 7.x `simple_encrypt` decodes with `self_keyed_decrypt`; `Key256` zeroized (compile-time: no `Debug`).
- [ ] **W10.7 Docs.** `docs/04-security-identity-and-auth.md` and any `docs/` reference to `simple_encrypt`/`encrypt_text`; changelog
  (W7.5). (The full docs rewrite is W11.)

---

## W8. Retire `notifications`; re-export `apostle_client`

`src/notifications.rs` (214 lines) has said "FIXME:(depricated) ... USE apostle_client instead / TODO Drop this whole file
in 6.0.0" (line 1-2) since 6.0. It is now two majors overdue. The user will commit the `apostle_client` change before this
plan starts; this item is the library side.

What the old module is: `Email { destination, subject, body }` plus `Email::send(Option<&str>)`, which dials a **hard-coded
mail server** (`MAIL_ADDRESS`, two public IPs, `notifications.rs:21`) with a **pinned server public key** (`MAIL_SERVER_PUB`,
`:24-25`) over `simple_comms`. `apostle_client` (`Libraries/apostle_client`, v1.1.2, standalone, no dependency on
`artisan_middleware`) has the same `Email` struct and `new/is_valid/to_json/from_json`, but `Email::send(&MailBundle)` takes
its server list and key from a bundle (`bundle.rs`, `MailServerConfig`, `.acai`), tries each address in order, and has
`validate_destination`. So the move is also what removes embedded infrastructure IPs and a key from this crate.

- [ ] **W8.1 [B] Delete `src/notifications.rs`** and `pub mod notifications;` (`lib.rs:23`), plus `src/tests/notification.rs`
  and its `#[path]` include (`mod notification_test` in `lib.rs`), and `src/tests_broke/notification_test.rs`.
- [ ] **W8.2 [B] Re-export instead:** add `apostle_client = "<published version>"` to `Cargo.toml` and `pub use apostle_client;`
  in `lib.rs` next to the existing `pub use dusa_collection_utils;`/`pub use simple_comms;` re-exports (`lib.rs:2-5`). Do **not** enable
  its `server-components` feature (pulls sqlx/rand). Version must be the crates.io release, never a path.
- [ ] **W8.3 Decide the module name (D10).** Either only `artisan_middleware::apostle_client::...` (clean break, recommended) or
  also keep `artisan_middleware::notifications` as `pub use apostle_client as notifications;` for one release. The `send()`
  signature differs either way (`Option<&str>` -> `&MailBundle`), so a re-export alias would still not compile-compat callers.
- [ ] **W8.4 Drop what only notifications used.** `simple_comms` is imported only by `lib.rs` (re-export) and
  `notifications.rs`; `apostle_client` already re-exports it (`apostle_client/src/lib.rs:5`). Decide (D11) whether to keep the
  crate-root `pub use simple_comms` (consumers may use it) or point it at `apostle_client::simple_comms`; either way remove it
  as a direct dependency if nothing else in the library needs it. Check `colored` imports in `Cargo.toml` still needed.
- [ ] **W8.5 Fix callers in this repo:** `examples/test_email.rs` uses the old `send(None)` form; rewrite against
  `MailBundle` or delete the example.
- [ ] **W8.6 Docs.** README/docs references to `notifications`/`Email::send` (grep `docs/` and `README.md`); migration-guide
  entry (W7.5): old `Email::send(None)` -> load a `MailBundle`, `email.send(&bundle)`.
- Consumer ripple: `Platform/ServerTest/src/email.rs` (`use artisan_middleware::notifications::Email`) must move to the new
  path/signature; `Software/website_monitor/src/main.rs:4,62-67` imports `notifications::{Email, EmailSecure}` and
  `EmailSecure` no longer exists in the current module, so it is already broken against 7.x/8.x. Any deployed service that
  reaches the mail server through the old hard-coded IPs now needs a `MailBundle` provisioned (`.acai` + mail server pub
  key; see `apostle_client` README), i.e. an ops/config step on each node.

---

## W9. Remaining FIXME/TODO markers and dependency cleanup

### Markers still in the code (all, as of `7c44282`)

| Where | Marker | Action |
|---|---|---|
| `notifications.rs:1-2, 194` | FIXME deprecated, "drop in 6.0.0", randomise server | W8 |
| `config.rs:1-2` | TODO fold `AppConfig` into Environment V2, "clean up in 6.x" | **W5**: replaced by `RuntimeSpec`. Consumers to migrate first: `Platform/aggregator`, `project_Kaelum` (`config.rs:736`), `Security`, Manager, GitMonitor, generic_runner |
| `config.rs:45, 49` | FIXME deprecate `DatabaseConfig`; FIXME `Aggregator` socket config is ancient | W5.3: both dropped (every consumer sets `database = None`) [B] |
| `encryption.rs:33-384` | Legacy RECS block, 5 fns `#[deprecated(since = "4.3.0")]` (`encrypt_text`, `decrypt_text`, raw variants, `clean_override_op` which is `pub async unsafe fn`) | **Keep, but un-deprecate, make private and mark experimental** (W10.2). Not deleted |
| `encryption.rs:395` | FIXME `generate_key` is critical code stranded in the deprecated area | W10.1: becomes part of the new generic AES-GCM API |
| `git_actions.rs:746` | "Token (Deprecated stop using)" display; `GitAuth.token` (`:41`) still used | Remove `token` [B] once GitMonitor's credential resolver ([`NODE-GIT-001`](../platform/plans/per-app-2026-10/gitmonitor/tasks/NODE-GIT-001.md)) is the only path. `assemble_remote_url` (`:295-312`) builds `https://{token}@{base_url}/...` where `base_url` already contains `https://`, i.e. `https://TOKEN@https://github.com/...` — malformed, and a token-in-URL ends up in `.git/config` |
| `portal.rs:41` | `#[allow(deprecated)]` for `base64::encode` | Goes away with W2.6 |
| `portal.rs:411, 415` | TODO per-instance log endpoint; TODO document | Keep as a roadmap note or implement with W4.7 `StreamCapture`; "document" is stale, remove |
| `identity.rs:20-21` | TODO add registration date to the identity hash; rename hash to "shadow" as a node-authenticity guarantee | Design item, tie to mTLS/RBAC ([Phase 2](../platform/rbac/RBAC_PHASE_2_MTLS.md)). Not 8.0 unless the node `Identifier` format changes anyway; at minimum record the decision |
| `identity.rs:215-216` | `Identifier` snowflake uses `rand::rng().random_range(1..=5)` for datacenter and machine ids | Only 25 combinations, so two nodes can share a generator id; low risk (time+sequence also feed it) but review with the `NodeId` work (W2.7) |
| `api/roles.rs:75-76` | TODO "add a super user ... notification and logs when SU executes a command" | `Role::Super` exists and the [RBAC engine](../platform/rbac/RBAC_IMPLEMENTATION_OVERVIEW.md) gives it blanket allow, so the TODO is half-obsolete. What remains is **auditing Super actions**: [RBAC Phase 1 §7](../platform/rbac/RBAC_PHASE_1_CORE_ENGINE.md) logs only `Purchase/Grant/Delete`; decide whether Super usage is logged too (and with W8, could notify through Apostle) |
| `resource_monitor.rs:572` | `get_system_stats` "LEGACY (welcome screen)" | Keep (Welcome uses it); W10.3 moves and documents it. `sysinfo`/`gethostname` stay |

### Dependencies (`Cargo.toml`)

Unused: no `use`/path reference anywhere in `src/` or `examples/`.
- [ ] **Remove** `reqwest 0.13.5`, `ed25519-dalek 3.0.0`, `tokio-uds 0.2` (a tokio-0.1 era crate), `colorized 1.0.0`, dev `mockall 0.15.0`.
  Dev-dependencies `serde`/`serde_json` duplicate the normal ones. The empty feature `cli = []` is referenced by no `cfg(feature)`.

Used only by code this plan deletes:
- [ ] `simple_comms` (only `notifications.rs` + the root re-export) -> W8 / D11
- [ ] `lz4`, `base64` (only `ProjectInfo::get_id`) -> W2.6
- [ ] `lazy_static` (one macro, `encryption.rs:19`, wrapping four statics for the RECS scaffold): replace with plain
  `static X: AtomicBool = AtomicBool::new(false)` and `Notify::const_new()` (no `Arc`/`lazy_static` needed) as part of W10.2
- (`sysinfo`, `gethostname` **stay**: `get_system_stats` is kept, see W10.3)
- [ ] `config` crate (only `config.rs` `AppConfig::new`) -> if W5.3 removes `AppConfig`
- [ ] `systemctl 0.3.1` (`systemd.rs`, shells out to `systemctl`) — keep, but note `systemd.rs` should use the supervisor from W4 or be dropped if the watchdog manages units itself (check consumers)

Should be upgraded/replaced:
- [ ] `trust-dns-resolver 0.23` (only `network.rs`): the project was renamed `hickory-resolver`; trust-dns is unmaintained. Move or drop `network.rs` if no consumer needs it.
- [ ] `users 0.11` (only `users.rs`, with `walkdir`): unmaintained; fork is `uzers`. Verify with `cargo audit`.
- [ ] `tempfile` is a normal dependency but is used in `runtime_bundle.rs` non-test code (`:62, :120`), so it is correctly a dependency; leave.
- [ ] Run `cargo audit`, `cargo outdated` and `cargo machete` (not run in this review: plan mode is read-only) and attach results. Re-check `acai_core`, `dusa_collection_utils` pins against `apostle_client` (`^1.0.1`, `^4.0.2`) so both resolve to one version.
- [ ] Edition 2021 vs 2024 (`apostle_client` is 2024): optional, but a major release is the time to move.

Net after W4/W5/W8/W9: about 7-9 fewer direct dependencies (`sysinfo`/`gethostname` stay for the Welcome app).

---

## W7. Hygiene and release mechanics

- [ ] **W7.1** `src/tests_broke/` (8 files, not compiled by `lib.rs`): fix and wire up, or delete.
- [ ] **W7.2** `lib.rs` `RELEASEINFO = ReleaseCandidate`: decide for 8.0.
- [ ] **W7.3** Dead directories under `Libraries/`: `artisan_middleware_lib` (abandoned duplicate, memory note says safe to
  delete), `artisan_lib-7.3.0` (worktree for branch `release-7.3.0`, which also carries `Subscription` as 7.3.0, `fd4b059`).
  Reconcile: is 7.3.0 ever released, or does its content roll into 8.0.0?
- [ ] **W7.4** Unwraps in non-test code: `runtime_bundle.rs` (19), `identity.rs` (13), `custom_config.rs`, `control.rs`; triage
  which are in production paths.
- [ ] **W7.5** CHANGELOG / migration guide for consumers: old -> new for every `[B]` item above.
- [ ] **W7.6** CI: `cargo test -- --test-threads=1` (known lock contention in some suites, [`agent_eng.md`](../platform/notes/agent_eng.md)), `cargo clippy`,
  and a check that no consumer `Cargo.toml` has a committed path override.

---

## Decisions

- D10. Module name for the mail client: only `artisan_middleware::apostle_client` (recommended, clean break) vs also an alias `notifications`.
- D11. Keep the crate-root `pub use simple_comms` or reach it only through `apostle_client`.
- D15. Names for the renamed `simple_*`: `self_keyed_encrypt/decrypt` (recommended, describes the property) vs `obfuscate/deobfuscate` (blunter).
- D16. Where does the key for the keyed AES-GCM API come from: a root-only node key file (`/opt/artisan/...`, 0600), derived from the
  node `Identifier`, systemd credentials/TPM, or per-app `acai` bundle key? Decides W10.5. Which callers count as "secret"?
- D17. `get_system_stats`: keep a re-export at `resource_monitor::get_system_stats` for one release, or update Welcome in the same release.
- D18. RECS write-up: what was it for, and what should "repurposing" target? (Needed for `docs/experimental-recs.md`.)

## Verification

- Encryption: new unit tests (W10.6); load every existing on-disk artifact type written by 7.x (state file, `Identifier`, git credentials, `.env`, registered-apps cache) with the 8.0 build; `cargo doc` shows no RECS items; Welcome builds and prints against the moved `get_system_stats`.
- Mail: `ServerTest` and `website_monitor` send through `apostle_client` with a provisioned `MailBundle`.
- Dependencies: `cargo audit`, `cargo outdated`, `cargo machete` output attached; `cargo tree -d` shows one version each of `acai_core` and `dusa_collection_utils`.
