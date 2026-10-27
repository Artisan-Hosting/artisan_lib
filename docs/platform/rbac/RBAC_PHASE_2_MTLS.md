# RBAC Phase 2 — mTLS Across Internal gRPC/REST Services

Read `RBAC_IMPLEMENTATION_OVERVIEW.md` first. This phase is independent of Phase 1's schema work — it's a transport-layer hardening pass, not part of the policy engine — but touches several of the same services Phases 3-6 also edit, so land it before or alongside those to avoid double-touching the same files.

**Goal:** every internal gRPC service (and one internal REST service) requires and verifies a client certificate before accepting a connection, on top of whatever Claims/service-credential check already exists or is being added. This is defense-in-depth: mTLS proves "this connection is from a legitimate fleet member," it does not replace the RBAC decision about what that caller is allowed to do.

**Repos touched:** `Libraries/artisan_lib` (new shared helper), `ais_auth`, `domain_management`, `ais_secretserver`, `RunpodManager`, `keystore`, `project_Kaelum`, and a new standalone crate `mtls_ca_tool`.

## Status (as of this revision)

**Done, verified compiling (and, for the CA tool, functionally verified end-to-end with real `openssl x509`/`openssl verify` output):**
- **`mtls_ca_tool`** (new crate, `Platform/mtls_ca_tool/`) — a CLI with `init-ca <out-dir>` and `issue <service-name> <ca-cert-path> <ca-key-path> <out-dir>` subcommands, using `rcgen` 0.14 (needs its `pem` + `x509-parser` features — not on by default, easy to miss). Generates a 10-year self-signed CA and 90-day leaf certs, each with a real distinguished name (`CN=Artisan Internal CA` / `CN=<service_name>`) — **do not omit the CN**: without it every cert defaults to rcgen's placeholder `CN=rcgen self signed cert`, which makes a leaf textually indistinguishable from self-signed to standard chain verification (`openssl verify` reports `error 18: self-signed certificate` even though the leaf is genuinely signed by the CA's key) and would very likely break real TLS handshakes. Also caught: `not_before`/`not_after` must actually be *assigned* onto `CertificateParams` before signing — computing them into local variables and forgetting the assignment silently produces a cert valid from 1975 to 4096 (rcgen's raw defaults), not the intended 90 days. Both were verified by actually building, running, and decoding the tool's output, not by reading the code.
- **`Libraries/artisan_lib/src/mtls.rs`** — simpler than originally sketched in §3 below: it's dependency-light on purpose (see the design note in §3), just `MtlsConfig`/`MtlsMaterial`/`load_mtls_material` loading raw PEM bytes from disk. No `tonic` type appears in this file.
- **Server-side mTLS wired and `cargo build`-verified for all five gRPC services**: `ais_auth`, `domain_management`, `ais_secretserver`, `RunpodManager`, `keystore`. Each loads `MtlsMaterial` from `MTLS_CERT_PATH`/`MTLS_KEY_PATH`/`MTLS_CA_PATH` (env vars, with `/etc/artisan/tls/<service>.{crt,key}` / `/etc/artisan/tls/ca.crt` defaults) and applies `ServerTlsConfig::new().identity(..).client_ca_root(..)` before `.add_service(...)`. Two real bugs were found and fixed only by actually compiling (not by review): a stray `?` after `Identity::from_pem(...)`/`Certificate::from_pem(...)` (neither returns a `Result`), and a hallucinated method name `.client_certificate_authority(...)` where tonic's real method is `.client_ca_root(...)`. Each service's `tonic` dependency needed its TLS feature turned on (`"tls"` for tonic 0.11, `"tls-ring"` for tonic 0.14's `domain_management`) — not on by default. `ais_secretserver`, `RunpodManager`, and `keystore` didn't depend on `artisan_middleware` at all before this (they used `dusa_collection_utils` directly) — added as a new dependency, currently pointed at the local path (see the standing revert-before-commit rule).

**Not done, and why:**
- **`project_Kaelum`'s control router mTLS.** §1/§3 below describe it as `warp`-based — **that's wrong**, verified against the real code: `project_Kaelum/src/main.rs` uses `axum::serve(listener, app)`, not `warp`. `axum`'s own `serve` has no built-in TLS; mTLS on axum normally goes through the separate `axum-server` crate (not currently a dependency here) with a hand-built `rustls::ServerConfig` that adds client-certificate verification — `axum-server`'s convenience `RustlsConfig` wrapper may only cover server-side auth by default, and this needs to be confirmed against the real crate API (the same way tonic's and rcgen's APIs were verified above) before writing real code against it. Left undone rather than guessed at without that verification. Whoever picks this up should start by resolving `axum-server` vs. hand-rolled `tokio-rustls` + `rustls::server::WebPkiClientVerifier` and confirming the exact builder API against the pinned version, the same way this doc's other sections were grounded against real source before delegating.
- **The cert expiry-alert check.** Not started. Still a good candidate for a follow-up pass — see §2's description of what it needs to do (read each cert's `NotAfter`, alert 14-30 days out).
- **Client-side mTLS** (a service verifying the *other* service's cert when calling out, not just requiring one on the way in). Only checked where a service constructs a raw `tonic::Channel` directly in code already read this session — none of the five services' `main.rs`/bootstrap files construct an outbound channel to another in-scope service directly. `ais_auth`'s keystore client goes through a separate shared library crate (`ais_keystore_lib`, part of the same `artisan_keystore` package that also builds the `keystore` binary) rather than raw `tonic` in `ais_auth` itself — adding client-side TLS there means changing that shared client helper once (benefiting every consumer), not each service individually. Not attempted this pass; flagging the exact shared crate so it isn't mistaken for "already covered."

## 1. Scope — which services get this, and which explicitly don't

**Get mTLS** (all are internal-only gRPC servers reachable over the internal network/Tailscale, never by an end-user browser):
- `ais_auth`
- `domain_management`
- `ais_secretserver`
- `RunpodManager`
- `keystore`
- `project_Kaelum`'s control router (REST/`warp`, not gRPC, but internal-only and currently has zero auth — see its own control-router middleware gap, also referenced in `RBAC_PHASE_4_SESSIONS_KAELUM.md`)

**Do NOT touch, and why** (don't "fix" these as a drive-by — they're intentionally excluded):
- **Manager↔portal's fleet tunnel.** Already has its own mutual-authentication handshake using the Noise protocol (`Manager/src/system/noise.rs`, `NoiseIdentity`; verified on portal's side in `portal/src/api/helpers.rs`'s `resolve_identity`/`node.identity.verify()`). This is equivalent protection to mTLS for this specific connection; wrapping it in TLS too is redundant work, not "cheap."
- **`watchdog`.** Reached only via a root-only Unix domain socket (`watchdog/src/grpc.rs`, `UnixListener::bind` + `root_only_connection` filter) — there is no network path here for mTLS to protect.
- **Portal's public HTTP API.** Browser/dashboard-facing, authenticated by JWT bearer tokens. mTLS doesn't fit human/browser clients; don't add it here.

## 2. Certificate authority

Generate **one internal root CA**, once, using `rcgen` (already a platform dependency — `ais_vm` already uses it for Proxmox's self-signed cert handling, so the crate is a known quantity here). Write a small standalone CLI tool (doesn't need to be a long-running service — a one-shot binary or even a script is fine) that:

1. Generates the root CA keypair + self-signed cert (long-lived, e.g. 10 years — this is the root of trust, rotating it is a whole-fleet event, not part of this phase's scope).
2. Given a service name (e.g. `ais_secretserver`, or `kaelum-{organization_id}` for a per-org Kaelum deployment), issues a leaf certificate signed by that CA, valid for **~90 days** (not longer — there's no CRL/OCSP revocation mechanism in this design, so a short lifetime is what bounds the blast radius of a leaked/compromised cert; this mirrors the industry-standard short-lived-cert practice popularized by Let's Encrypt).
3. Outputs the leaf cert + key to wherever that service's deployment config expects to find them (coordinate the exact path convention with the person doing deployment — a reasonable default is alongside each service's existing secrets/config, e.g. `/etc/artisan/tls/{service}.{crt,key}`).

**The root CA private key stays offline** — generated once, used to sign leaf certs via the CLI tool run by a human, never loaded by any running service. (This also sidesteps a bootstrapping problem: `keystore` is one of the services being protected by mTLS, so the CA obviously can't depend on `keystore` to hold its own root key.)

Also ship a **trivial expiry-alert check** in this same phase — not deferred. A cron job (or a scheduled task in whatever the platform already uses for scheduled jobs — check if one exists before building a new one) that reads each issued cert's `NotAfter` and alerts (log line is the minimum bar; wire it into whatever the platform's existing alerting is if one exists) 14-30 days before expiry. This is the cheap insurance that makes "no rotation automation" an acceptable trade-off instead of a landmine — don't skip it because it feels like a stretch item; it's the difference between a manual-renewal process and a future outage nobody saw coming.

## 3. Shared helper in `artisan_middleware`

Add `Libraries/artisan_lib/src/mtls.rs`:

```rust
pub struct MtlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub ca_cert_path: PathBuf,
}

pub fn server_tls_config(cfg: &MtlsConfig) -> Result<tonic::transport::ServerTlsConfig, ErrorArrayItem> {
    // Load cert_path/key_path as this service's Identity, ca_cert_path as the
    // trusted client root -- tonic::transport::ServerTlsConfig::new()
    //   .identity(identity)
    //   .client_ca_root(ca_cert)
}

pub fn client_tls_config(cfg: &MtlsConfig) -> Result<tonic::transport::ClientTlsConfig, ErrorArrayItem> {
    // Same identity (this service authenticating itself as a client to another
    // service) plus ca_certificate(ca_cert) so it also verifies the server's cert.
}
```

Every service in scope loads its own `MtlsConfig` (cert/key/CA paths from config or environment, following whatever convention that service already uses for its other secrets) and passes the result into its `tonic::transport::Server::builder().tls_config(...)` (server side) and `tonic::transport::Channel::from_shared(...).tls_config(...)` (client side, for every *other* in-scope service it calls). Don't hand-roll five slightly different TLS setups — every service should call this exact helper.

For `project_Kaelum`'s `warp`-based control router, the equivalent is `warp`'s `.tls().cert_path(...).key_path(...).client_auth_required_path(...)` (via the `rustls` backend `warp` already uses) — same CA, same cert convention, different API surface since it's REST not gRPC.

## 4. Per-service wiring

For each of `ais_auth`, `domain_management`, `ais_secretserver`, `RunpodManager`, `keystore`:
1. Find where its gRPC server is bootstrapped (`main.rs` or equivalent — e.g. `ais_auth`'s is wherever `tonic::transport::Server::builder()` is called in its `src/grpc.rs`/`src/main.rs`).
2. Load `MtlsConfig` and apply `server_tls_config` via `.tls_config(...)`.
3. Find every place this service constructs a gRPC *client* to call one of the other in-scope services (e.g. `domain_management`'s `AuthClient` in `src/auth.rs` calling `ais_auth`) and apply `client_tls_config` there too.

For `project_Kaelum`, apply the `warp` TLS config to `create_control_router()`'s server bootstrap in `src/main.rs` (the same place `RBAC_PHASE_4_SESSIONS_KAELUM.md` adds the missing auth middleware — do both in the same pass since you're already touching this file).

## 5. Done when

- Every service listed in scope fails a connection attempt from a client presenting no certificate or the wrong CA's certificate — verify with a quick manual test (e.g. `grpcurl` without `-cert`/`-key` flags against a service that previously accepted plaintext) or a test using `tonic`'s test utilities if that's faster to wire up.
- A connection with a valid, CA-signed client cert still succeeds end-to-end (don't just test the rejection path).
- The expiry-alert check actually fires when pointed at a deliberately-near-expiry test certificate — don't just write the check and assume it works.
- No service's `Cargo.toml` has a leftover local-path override for any dependency touched in this phase.
- `cargo build` and existing test suites stay clean for every touched crate.
