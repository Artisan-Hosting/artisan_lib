# Security, Identity, and Auth Models

## `encryption` module

### Recommended functions

- `simple_encrypt(data: &[u8]) -> Result<Stringy, ErrorArrayItem>`
- `simple_decrypt(cipher: &[u8]) -> Result<Vec<u8>, ErrorArrayItem>`
- `generate_key(buffer: &mut [u8])`

These are the modern AES-256-GCM helpers and are the preferred path in your doc comments.

### Deprecated legacy functions (Linux)

- `encrypt_text`, `decrypt_text`
- `encrypt_data`, `decrypt_data`
- `clean_override_op`

### Example

```rust
use artisan_middleware::encryption::{simple_decrypt, simple_encrypt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let plaintext = b"sensitive information";
    let encrypted = simple_encrypt(plaintext)?;
    let decrypted = simple_decrypt(encrypted.as_bytes())?;
    assert_eq!(plaintext.to_vec(), decrypted);
    Ok(())
}
```

## `identity` module (Linux)

The identity model is now split into explicit domains.
For full architecture details, see [07-identity-platform.md](./07-identity-platform.md).

### Main structures/constants

- Node domain: `Identifier`, `NodeId`, `SnowflakeIDGenerator`
- Workload domain: `WorkloadId`, `WorkloadIdentity`
- Runtime domain: `RuntimeId`, `RuntimeIdentity`
- Authority domain: `AuthorityId`, `AuthorityKind`, `AuthorityIdentity`
- Shared context: `IdentityContext`, `IDENTITY_RENAME_MAP`
- Node constants: `IDENTITYPATHSTR`, `HASH_LENGTH`, `CUSTOM_EPOCH`

### Key functions

- `NodeId::generate().await`, `NodeId::load().await`, `NodeId::save_to_file()`
- `Identifier::new().await`, `verify().await`, `load().await`, `save_to_file()`
- `WorkloadId::new(...)`, `WorkloadId::from_git_auth(...)`
- `RuntimeId::generate().await`, `RuntimeIdentity::generate(...).await`
- `AuthorityId::generate().await`, `AuthorityIdentity::generate(...).await`
- `IdentityContext::new(...)`

### Example

```rust,no_run
use artisan_middleware::identity::{
    AuthorityIdentity, AuthorityKind, IdentityContext, NodeId, RuntimeIdentity, WorkloadId,
    WorkloadIdentity,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node_id = match NodeId::load().await? {
        Some(id) => id,
        None => {
            let id = NodeId::generate().await?;
            id.save_to_file()?;
            id
        }
    };

    let workload_id = WorkloadId::new("payments-api");
    let workload = WorkloadIdentity::new(node_id, workload_id.clone(), "source-123".into());
    let runtime = RuntimeIdentity::generate(node_id, workload_id, 1).await?;
    let authority = AuthorityIdentity::generate(runtime.runtime_id, AuthorityKind::Manager, None).await?;

    let context = IdentityContext::new(node_id, workload, runtime, authority);
    println!("{:?}", context.runtime.runtime_id);
    Ok(())
}
```

## `api` module (`claims`, `roles`, `token`)

### `roles`

- Enum: `Role` (`Super`, `Admin`, `Controller`, `Viewer`, `Audit`, `None`)
- Helpers: `Role::from_str`, `Role::to_str`
- Permission helper: `has_org_permission(current_role, required_role)`

### `claims`

- `TokenType`
- `PasswdClaims`
- `Claims`
- helpers: `Claims::to_map()`, `Claims::from_map(...)`

### `token`

- `TokenResponse`
- `SimpleLoginRequest`

### Example

```rust
use artisan_middleware::api::claims::{Claims, TokenType};
use artisan_middleware::api::roles::{has_org_permission, Role};

fn main() {
    let can_view = has_org_permission(Role::Controller, Role::Viewer);
    assert!(can_view);

    let claims = Claims {
        sub: "user-1".into(),
        role: Role::Admin,
        org_id: "org-1".into(),
        exp: 1_800_000_000,
        kind: TokenType::Auth,
    };

    let map = claims.to_map();
    let rebuilt = Claims::from_map(map).unwrap();
    assert_eq!(rebuilt.org_id, "org-1");
}
```
