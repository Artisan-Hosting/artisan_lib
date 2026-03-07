# Utility Modules

## `timestamp` module

### Key functions

- `current_timestamp()`
- `timesince_unix_timestamp(timestamp)`
- `format_unix_timestamp(timestamp)`
- `time_to_unix_timestamp(datetime_str)`
- `days_in_current_month()`

### Example

```rust
use artisan_middleware::timestamp::{current_timestamp, format_unix_timestamp};

fn main() {
    let now = current_timestamp();
    println!("{}", format_unix_timestamp(now));
}
```

## `version` module

### Key functions

- `aml_version()` builds a `Version` from `CARGO_PKG_VERSION` + `RELEASEINFO`.
- `str_to_version(version_str, release_code)` parses string form into a `Version`.

## `cli` module

### Key functions

- `get_user_input(prompt)`
- `get_encrypted_user_input(prompt).await`
- `get_user_selection(options)`
- `get_yes_no(prompt)`
- `clean_screen()`

## `historics` module

### Main structures

- `HistoricalUsage`
- `UsageLedger`

### Key functions

- `UsageLedger::new()`
- `update_workload_usage(workload_id, metrics)`
- `persist_to_disk(path)` / `load_from_disk(path)`

### Example

```rust,no_run
use dusa_collection_utils::core::types::stringy::Stringy;
use artisan_middleware::aggregator::Metrics;
use artisan_middleware::historics::UsageLedger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = UsageLedger::new();
    ledger.update_workload_usage(
        Stringy::from("workload-1"),
        Metrics { cpu_usage: 10.0, memory_usage: 256.0, other: None },
    );
    ledger.persist_to_disk("/tmp/usage_ledger.json")?;
    Ok(())
}
```
