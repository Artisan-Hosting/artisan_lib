# Aggregation and Messaging

## `aggregator` module

### Main enums and structs

- `CommandType`, `Status`, `Command`
- `NetworkUsage`, `Metrics`, `LiveMetrics`
- `UsageAccumulator`, `UsageRecord`, `BilledUsageSummary`, `BillingCosts`
- `AppStatus`, `RegisterApp`, `DeregisterApp`, `UpdateApp`, `CommandResponse`
- `AppMessage` (message envelope)
- `AppContext` (shared tx channels + usage map)

### Key functions

- `update_metrics(live, &usage_map).await`
- `spawn_flush_task(usage_map, output_dir).await`
- `flush_metrics_to_disk(&usage_map, &output_dir).await`
- `load_usage_records_from_dir(dir)`
- `summarize_usage(records)`
- `save_registered_apps(apps).await` / `load_registered_apps().await`
- `initialize_app_context(output_dir).await`

### Example: feed live metrics

```rust,no_run
use dusa_collection_utils::core::types::pathtype::PathType;
use dusa_collection_utils::core::types::stringy::Stringy;
use artisan_middleware::aggregator::{initialize_app_context, LiveMetrics};

#[tokio::main]
async fn main() {
    let output_dir = PathType::Content("/tmp/usage".into());
    let (ctx, _project_rx) = initialize_app_context(output_dir).await;

    let _ = ctx.metrics_tx.send(LiveMetrics {
        runner_id: Stringy::from("runner-a"),
        instance_id: Stringy::from("instance-1"),
        cpu_usage: 15.0,
        memory_mb: 220.0,
        rx_bytes: 12_000,
        tx_bytes: 3_000,
    });
}
```

### Example: summarize JSONL usage records

```rust,no_run
use dusa_collection_utils::core::types::pathtype::PathType;
use artisan_middleware::aggregator::{load_usage_records_from_dir, summarize_usage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathType::Content("/tmp/usage".into());
    let records = load_usage_records_from_dir(&dir)?;
    if let Some(summary) = summarize_usage(&records) {
        println!("samples={} peak_cpu={}", summary.total_samples, summary.peak_cpu);
    }
    Ok(())
}
```

## `portal` module

Primary transport/data models for manager-node-runner API payloads.

### Key structures

- Envelope: `ApiResponse<T>`, `ErrorInfo`, `ErrorCode`
- Node models: `NodeInfo`, `NodeDetails`, `ManagerData`, `NodeReloadResult`
- Runner models: `RunnerSummary`, `RunnerDetails`, `RunnerHealth`, `RunnerLogs`
- Command/log models: `CommandRequest`, `CommandResponse`, `CommandStatusResponse`, `LogEntry`, `NodeLogs`, `RunnerLogResponse`, `InstanceLogResponse`
- Discovery/registration message: `PortalMessage`, `ProjectInfo`

### Example envelope

```rust
use artisan_middleware::portal::{ApiResponse, RunnerSummary};

let response: ApiResponse<Vec<RunnerSummary>> = ApiResponse {
    status: "success".into(),
    data: Some(vec![]),
    errors: vec![],
};
```
