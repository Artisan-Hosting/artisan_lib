# State, Config, and Environment

## `config` module

### Main structures

- `WorkloadConfig`
- `GitConfig`
- `DatabaseConfig`
- `Aggregator`

### Key functions

- `WorkloadConfig::new(enviornment)`: wrap an `Enviornment` payload.
- `WorkloadConfig::new_v2()`: start a V2 builder flow.
- `WorkloadConfig::dummy()`: useful placeholder config for tests/examples.

## `state_persistence` module

### Main structures

- `RuntimeState`: runtime-only workload state (status, pid, counters, logs, stdio).
- `WorkloadSnapshot`: persisted composition of identity + config + runtime + optional custom data.
- `StatePersistence`: encrypted save/load helpers.

### Key functions

- `StatePersistence::get_state_path(state_name)`
- `save_state(state, path).await` / `load_state(path).await`
- `save_snapshot(snapshot, path).await` / `load_snapshot(path).await`
- `update_state(state, path, metrics).await`
- `wind_down_state(state, path).await`
- `log_error(state, err, path).await`
- `debug_log_set(config)`

### Example

```rust,no_run
use artisan_middleware::config::WorkloadConfig;
use artisan_middleware::state_persistence::StatePersistence;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _config = WorkloadConfig::dummy();
    let state_path = StatePersistence::get_state_path("runner-name");

    // Load existing state if available
    let _loaded = StatePersistence::load_state(&state_path).await.ok();
    Ok(())
}
```

## `enviornment::definitions` module

### Main structures

- `ApplicationType`
- `Enviornment` (`V1` and `V2`)
- `Enviornment_V1` (documented and operational)
- `Enviornment_V2` (builder + validation + parsing operational)

### Key constants

- `VERSION_TAG_V1`, `VERSION_TAG_V2`, `VERSION_TAG_V3`

### Key functions (`V1`)

- `encrypt().await`
- `to_json()`
- `parse_to().await` (adds version header + encrypts)
- `parse_from(bytes).await` (decrypts + validates version header)
- `Enviornment::parse(bytes).await` (version-dispatch parser)

### Key functions (`V2`)

- `Enviornment::new_v2()` (builder entrypoint)
- `set_*` / `with_*` / `add_*` helpers
- `validate()`
- `finalize()` (returns `Enviornment::V2`)

### Example

```rust,no_run
use artisan_middleware::enviornment::definitions::{ApplicationType, Enviornment_V1};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = Enviornment_V1 {
        application_type: Some(ApplicationType::Python),
        execution_uid: Some(1000),
        execution_gid: Some(1000),
        primary_listening_port: Some(8080),
        secret_id: None,
        secret_passwd: None,
        path_modifier: None,
        pre_build_command: Some("pip install -r requirements.txt".into()),
        build_command: None,
        run_command: Some("python app.py".into()),
        env_key_0: Some(("ENV".into(), "prod".into())),
    };

    let bytes = env.parse_to().await?;
    let _parsed = Enviornment_V1::parse_from(&bytes).await?;
    Ok(())
}
```

## `config_bundle` module

`WorkloadSnapshot` is the convenience composition around:

- runtime `RuntimeState`
- static `WorkloadConfig`
- identity context
- optional service-specific JSON config

Useful methods: `get_name`, `get_status`, `set_status`, `get_pid`, `set_pid`, `update_runtime`, `update_error_log`, `update_timestamp`.
