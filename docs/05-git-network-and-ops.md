# Git, Network, and Ops Helpers

## `git_actions` module

### Main structures

- `GitServer`
- `GitAuth`
- `GitCredentials`
- `GitAction`

### Key functions

- `GitCredentials::new(...)`, `new_vec(...)`, `save(...)`, `add_auth(...)`, `delete_item(...)`
- `GitAuth::assemble_remote_url()`, `assemble_remote_ssh()`, `generate_id()` (Linux)
- `GitAction::execute().await`
- `generate_git_project_path(auth)` (Linux)
- `generate_git_project_id(auth)` (Linux)

### Example

```rust,no_run
use dusa_collection_utils::core::types::{pathtype::PathType, stringy::Stringy};
use artisan_middleware::git_actions::{GitAction, GitServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let action = GitAction::Clone {
        repo_name: Stringy::from("artisan_lib"),
        repo_owner: Stringy::from("owner"),
        destination: PathType::Content("/tmp/artisan_lib".into()),
        repo_branch: Stringy::from("main"),
        server: GitServer::GitHub,
    };

    let _ = action.execute().await?;
    Ok(())
}
```

## `network` module (Linux)

- `resolve_url(url, resolver_addr).await`: DNS resolve helper (defaults to Cloudflare `1.1.1.1`).

```rust,no_run
use artisan_middleware::network::resolve_url;

#[tokio::main]
async fn main() {
    match resolve_url("example.com", None).await {
        Ok(Some(ips)) => println!("ips={:?}", ips),
        Ok(None) => println!("no records"),
        Err(e) => eprintln!("resolver setup error: {}", e),
    }
}
```

## `notifications` module

### Main structure

- `Email` with `new`, `is_valid`, `to_json`, `from_json`, `send`.

```rust,no_run
use dusa_collection_utils::core::types::stringy::Stringy;
use artisan_middleware::notifications::Email;

#[tokio::main]
async fn main() {
    let email = Email::new(
        Stringy::from("ops@example.com"),
        Stringy::from("Subject"),
        Stringy::from("Body"),
    );

    let _result = email.send(None).await; // default mail address
}
```

## `systemd` module (Linux)

### Main structures

- `SystemdService`
- `ServiceStatus`

### Key functions

- `SystemdService::new(service_name)`
- `start`, `stop`, `kill`, `restart`, `is_active`

## `users` module (Linux)

### Key functions

- `get_id(user)` returns `(uid, gid)`
- `set_file_ownership(path, uid, gid)`
- `set_file_permission(path, permission)`
