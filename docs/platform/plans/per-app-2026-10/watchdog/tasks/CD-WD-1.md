# CD-WD-1: Watchdog port allocator for customer apps (pure functions)

watchdog · 0.75 h · Rust

## Task
Add `src/functions/ports.rs`: the node picks a stable, free, unshared listening port in 20000-29999 for an app that opts in, remembers it in `<conf_dir>/<app>/port`, and reports it. Add `pub mod ports;` to `src/functions/mod.rs`.

## Why
Nothing assigns a port to a customer app today, so a vhost backend `{node, port}` has to be typed in by hand. The node is the authority on what is free on it. This re-implements work that exists only on the old `customer-deploy` branch.

## Existing implementation
- `src/functions/mod.rs` lists the public modules (`config_files`, `inventory`, `monitoring`, `runtime_bundle_lifecycle`, `verification`); add `ports` in alphabetical order after `monitoring`.
- `crate::definitions::ARTISAN_CONF_DIR` is `/opt/artisan/etc`. The crate already depends on `toml` and `std`; no new dependency.
- Nothing in `ports` exists yet.

## Required changes
1. `pub const PORT_RANGE_START: u16 = 20000;` and `pub const PORT_RANGE_END: u16 = 29999;`.
2. `pub fn port_file(conf_dir: &Path, app: &str) -> PathBuf` is `conf_dir/app/port`.
3. `pub fn read_port(conf_dir: &Path, app: &str) -> Option<u16>`: the trimmed file content parsed as a `u16` inside the range; anything else (missing file, text, out of range, empty) is `None`.
4. `pub fn ports_in_use(conf_dir: &Path, except: &str) -> HashSet<u16>`: every valid port stored by an app directory other than `except`.
5. `pub fn pick_port(used: &HashSet<u16>, is_bound: impl Fn(u16) -> bool) -> Option<u16>`: the lowest port in the range that is neither in `used` nor `is_bound`.
6. `pub fn wants_auto_port(config_toml: &str) -> bool`: true only when the TOML has `auto_port = true` under `[app_specific]`; false for `false`, a missing key, or invalid TOML.
7. `pub fn port_or_zero(conf_dir: &Path, app: &str) -> u32`: the stored port, or 0.
8. `pub(crate) fn ensure_port_with(conf_dir: &Path, app: &str, bound: impl Fn(u16) -> bool) -> io::Result<u16>`: returns the stored port if `read_port` has one, else picks one with `pick_port(&ports_in_use(..), bound)`, writes `"<port>\n"` to a `.tmp` file in the same directory and renames it over `port` (so a crash never leaves half a number). The `<conf_dir>/<app>/` directory must already exist (the function does not create it). A process-wide `static Mutex` makes allocations one at a time. Returns `io::Error::other("no free port left in the customer app range")` when nothing is free.
9. `pub fn ensure_port(conf_dir: &Path, app: &str) -> io::Result<u16>` calls `ensure_port_with` with a real check: a port is bound when binding a `TcpListener` to `0.0.0.0:port` or `127.0.0.1:port` fails.
10. `pub fn port_for_start(app: &str) -> io::Result<Option<u16>>`: reads the app's `Config.toml` in its directory under `ARTISAN_CONF_DIR`; `Ok(None)` when the file is missing or `wants_auto_port` is false; otherwise `ensure_port(ARTISAN_CONF_DIR, app).map(Some)`.
11. `pub mod ports;` in `src/functions/mod.rs`.

## Interface/contracts
None.

## Boundaries
- Do not call `ports` from `monitoring.rs` or `grpc.rs` (CD-WD-4 and CD-WD-6 do that).
- Apps that do not set `auto_port = true` must never be given a port: an older app that reads `PORT` would be moved off the port its vhost points at.

## Acceptance criteria
1. `pick_port` returns the lowest free port: with `used = {20000, 20001}` and `is_bound = |p| p == 20002` it returns `Some(20003)`; with nothing used and nothing bound it returns `Some(20000)`; with `is_bound = |_| true` it returns `None`.
2. With the directory `<tmp>/ais_aaaa1111` created, `ensure_port_with(&tmp, "ais_aaaa1111", |_| false)` returns `20000` and the file `<tmp>/ais_aaaa1111/port` holds `20000\n`; a second call with `|_| true` still returns `20000` (the stored port is reused even though it now looks bound).
3. With the directories `ais_aaaa1111`, `ais_bbbb2222`, `ais_cccc3333` created, three `ensure_port_with(.., |_| false)` calls return three different ports.
4. `read_port` returns `None` when the port file holds `not a number`, `80`, `70000` or is empty, and `Some(20007)` when it holds `20007\n`; `port_or_zero` is `0` for an app with no port file and `20000` after `ensure_port_with(.., |_| false)`.
5. `wants_auto_port` is true for `[app_specific]\nauto_port = true\nrun_command = "x"\n`, and false for `[app_specific]\nauto_port = false\n`, for `[app_specific]\nrun_command = "x"\n` and for `not toml {{{`.

## Not tested (checked in review)
- `ensure_port` and `port_for_start` use the real filesystem and sockets and are checked in review.
- `pub mod ports;` is in `src/functions/mod.rs`. Nothing else in the crate changed.

## Validation
- `cargo test functions::ports`
- `cargo build`
It must fail before the work and pass after it; the acceptance criteria above are what it has to prove.

## Dependencies
None inside this project.

## Outputs
`watchdog/src/functions/ports.rs`, `watchdog/src/functions/mod.rs`.
