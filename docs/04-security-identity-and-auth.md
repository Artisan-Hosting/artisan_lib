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

### Main structures/constants

- `SnowflakeIDGenerator`
- `NodeIdentity`
- `IDENTITYPATHSTR`, `HASH_LENGTH`, `CUSTOM_EPOCH`

### Key functions

- `SnowflakeIDGenerator::new(datacenter_id, machine_id)`
- `generate_id().await`
- `NodeIdentity::new().await`
- `verify().await`
- `load().await`, `save_to_file()`, `load_from_file()`
- `to_json()`, `to_encrypted_json().await`

### Example

```rust,no_run
use artisan_middleware::identity::NodeIdentity;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ident = NodeIdentity::new().await?;
    assert!(ident.verify().await);
    ident.save_to_file()?;

    let loaded = NodeIdentity::load_from_file()?;
    println!("id={}", loaded.id);
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
