# NODE-GIT-001: GitMonitor credential resolver (pure)

GitMonitor · 1 h · Rust

## Task
Add `GitMonitor/src/application/credentials.rs` with the pure C3 resolver: given a repo's `GitAuth`, decide which credential applies. Nothing else changes yet; NODE-GIT-011 uses it.

## Why
Today every clone and fetch uses the node's single operator GitHub token (`auth.rs` `github_auth_header()`), passed unscoped as `http.extraheader`. A customer's own repo token is ignored, a customer can deploy any private repo the operator can read, and a custom server receives the operator's token (security finding S1, gaps M1–M2). The resolver is the rule that fixes this.

## Existing implementation
- `GitMonitor/src/application/auth.rs`: the global token (`init_gh_token`, `github_auth_header`).
- `git.rs:464` `expected_remote_url(auth)` gives the remote URL, which tells you the host.
- Modules are declared in `GitMonitor/src/application/main.rs`.

## Required changes
1. Add `credentials.rs` with a pure function (C3), with exactly this signature:
   `pub fn resolve_repo_credential(token: Option<&str>, id: &str, remote_url: &str, token_dir: &Path, global_token: Option<&str>) -> RepoCredential`
   where `token` is the repository's configured token value (`None` when unset), and `remote_url` is the repository's remote URL as a plain `&str`. Add `pub enum RepoCredential { Header { url_prefix: String, header: String }, Anonymous, Pending(String) }` deriving `Debug, Clone, PartialEq, Eq`. No other type from this crate may appear in the file (in particular not `GitAuth`); callers unpack `GitAuth` themselves.
   - `token_dir` defaults to `/opt/artisan/etc/git_tokens` and can be overridden with the env var `AIS_GIT_TOKEN_DIR` (provide a small `fn token_dir() -> PathBuf` for that).
   - token `None` → `Header` with the global token, but only when the remote host is `github.com`; otherwise `Anonymous`.
   - `"anonymous"` → `Anonymous`.
   - `"github-app:<n>"` reads `<token_dir>/<id>`: a non-empty file → `Header`; an empty file → `Anonymous`; a missing file → `Pending("waiting for repository credentials")`.
   - any other value → `Header` with that literal, scoped to the repo host.
   - `url_prefix` is `"<scheme>://<host>/"`, taken from `expected_remote_url`.
2. The file must be **self-contained** with no crate-internal dependencies (std only, no `crate::` paths), so NODE-GIT-002 can copy it verbatim into Manager. Derive the host and scheme from the `remote_url` parameter yourself; do not call `expected_remote_url`.
3. Add `mod credentials;` to `main.rs`. Do not use it yet.
4. Never log the header or token.

## Interface/contracts
C1 (token files), C3 (the resolver table).

## Boundaries
- Don't change `git.rs`, `pull.rs`, `auth.rs` or any behaviour. NODE-GIT-011 does that.
- Don't touch `artisan_middleware`.

## Acceptance criteria
Inputs below use `token_dir` = a fresh temp dir, `remote` = the repository's remote URL, `gt` = the global token. In
every Header row `header` is `Authorization: Basic <base64("x-access-token:<token>")>` (standard base64 with padding)
and `url_prefix` is `<scheme>://<host>/` of `remote`.

1. `resolve_repo_credential(None, "ab12cd34", "https://github.com/o/r.git", dir, Some("gt"))` returns
   `Header { url_prefix: "https://github.com/", header: "Authorization: Basic eC1hY2Nlc3MtdG9rZW46Z3Q=" }`.
2. `resolve_repo_credential(None, "ab12cd34", "https://git.example.com/o/r.git", dir, Some("gt"))` returns `Anonymous`
   (the operator token never goes to another host).
3. `resolve_repo_credential(None, "ab12cd34", "https://github.com/o/r.git", dir, None)` returns `Anonymous`.
4. `resolve_repo_credential(Some("anonymous"), "ab12cd34", <any remote>, dir, Some("gt"))` returns `Anonymous`.
5. With `dir/ab12cd34` containing `ghs_abc` followed by a newline, `resolve_repo_credential(Some("github-app:42"), "ab12cd34",
   "https://github.com/o/r.git", dir, Some("gt"))` returns
   `Header { url_prefix: "https://github.com/", header: "Authorization: Basic eC1hY2Nlc3MtdG9rZW46Z2hzX2FiYw==" }`
   (the file is read once, trimmed, and the global token is not used).
6. With `dir/ab12cd34` present and empty, the same call returns `Anonymous`.
7. With no file at `dir/ab12cd34`, the same call returns `Pending("waiting for repository credentials".to_string())`.
8. `resolve_repo_credential(Some("pat1"), "ab12cd34", "http://gitea.local/o/r.git", dir, Some("gt"))` returns
   `Header { url_prefix: "http://gitea.local/", header: "Authorization: Basic eC1hY2Nlc3MtdG9rZW46cGF0MQ==" }`.
9. `token_dir()` returns `/opt/artisan/etc/git_tokens` when `AIS_GIT_TOKEN_DIR` is unset, and the value of
   `AIS_GIT_TOKEN_DIR` when it is set (one test sets and clears the variable; no other test reads it).
10. `RepoCredential` implements `Debug`, `Clone`, `PartialEq` and `Eq`: cloning a `Header` gives an equal value.

## Not tested (checked in review)
- `credentials.rs` uses only `std`: no `crate::` path, no external crate, and no `GitAuth`. Base64 is therefore a few
  lines of code in the file itself, not the `base64` crate.
- `mod credentials;` is declared once in `GitMonitor/src/application/main.rs`, and nothing calls the resolver yet.
- `git.rs`, `pull.rs`, `auth.rs` and `artisan_middleware` are unchanged.
- Neither the header nor the token is logged or printed anywhere in the file.

## Validation
- `cargo test credentials`
It must fail before the work and pass after it; the acceptance criteria above are what it has to prove.

## Dependencies
ENV-001.

## Outputs
`GitMonitor/src/application/credentials.rs`, `GitMonitor/src/application/main.rs` (the `mod` line).
