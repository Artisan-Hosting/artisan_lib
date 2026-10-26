#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use dusa_collection_utils::core::types::{rwarc::LockWithTimeout, stringy::Stringy};

    use crate::aggregator::{update_metrics, LiveMetrics, UsageMap};

    #[tokio::test]
    async fn test_network_first_sample_sets_baseline() {
        let usage_map: UsageMap = LockWithTimeout::new(HashMap::new());

        update_metrics(
            LiveMetrics {
                project_id: Stringy::from("project-a"),
                instance_id: Stringy::from("instance-1"),
                cpu_usage: 1.0,
                memory_mb: 10.0,
                rx_bytes: 10_000,
                tx_bytes: 5_000,
            },
            &usage_map,
            1_000,
        )
        .await
        .expect("first update should succeed");

        let map = usage_map.try_read().await.expect("map should be readable");
        let key = (Stringy::from("project-a"), Stringy::from("instance-1"));
        let entry = map.get(&key).expect("entry should exist");
        assert_eq!(entry.total_rx, 0, "first sample should not be counted");
        assert_eq!(entry.total_tx, 0, "first sample should not be counted");
        assert_eq!(entry.last_rx, 10_000);
        assert_eq!(entry.last_tx, 5_000);
        assert!(entry.has_network_baseline);
    }

    #[tokio::test]
    async fn test_network_delta_and_reset_behavior() {
        let usage_map: UsageMap = LockWithTimeout::new(HashMap::new());

        let updates = vec![
            (10_000, 5_000), // baseline only
            (10_500, 5_400), // +500, +400
            (200, 100),      // reset/backward, add 0 and re-baseline
            (250, 180),      // +50, +80
        ];

        for (i, (rx, tx)) in updates.into_iter().enumerate() {
            update_metrics(
                LiveMetrics {
                    project_id: Stringy::from("project-a"),
                    instance_id: Stringy::from("instance-1"),
                    cpu_usage: 1.0,
                    memory_mb: 10.0,
                    rx_bytes: rx,
                    tx_bytes: tx,
                },
                &usage_map,
                1_000 + (i as i64) * 1_000,
            )
            .await
            .expect("update should succeed");
        }

        let map = usage_map.try_read().await.expect("map should be readable");
        let key = (Stringy::from("project-a"), Stringy::from("instance-1"));
        let entry = map.get(&key).expect("entry should exist");
        assert_eq!(entry.total_rx, 550);
        assert_eq!(entry.total_tx, 480);
        assert_eq!(entry.last_rx, 250);
        assert_eq!(entry.last_tx, 180);
    }

    #[tokio::test]
    async fn test_cpu_first_sample_seeds_baseline_with_no_accrual() {
        let usage_map: UsageMap = LockWithTimeout::new(HashMap::new());

        update_metrics(
            LiveMetrics {
                project_id: Stringy::from("project-a"),
                instance_id: Stringy::from("instance-1"),
                cpu_usage: 100.0,
                memory_mb: 10.0,
                rx_bytes: 0,
                tx_bytes: 0,
            },
            &usage_map,
            1_000,
        )
        .await
        .expect("first update should succeed");

        let map = usage_map.try_read().await.expect("map should be readable");
        let key = (Stringy::from("project-a"), Stringy::from("instance-1"));
        let entry = map.get(&key).expect("entry should exist");
        assert_eq!(
            entry.total_cpu_core_seconds, 0.0,
            "first sample has no prior timestamp to weight against, so it must not accrue"
        );
        assert_eq!(entry.last_sample_epoch_ms, Some(1_000));
    }

    #[tokio::test]
    async fn test_cpu_accrues_time_weighted_core_seconds() {
        let usage_map: UsageMap = LockWithTimeout::new(HashMap::new());

        // Two samples, 2 seconds apart, at 50% CPU: 0.5 core * 2s = 1.0 core-second.
        // (A naive sum of the raw percent readings, the bug this fixes, would instead
        // treat this as 100.0 "core-hours" — three orders of magnitude off.)
        update_metrics(
            LiveMetrics {
                project_id: Stringy::from("project-a"),
                instance_id: Stringy::from("instance-1"),
                cpu_usage: 50.0,
                memory_mb: 10.0,
                rx_bytes: 0,
                tx_bytes: 0,
            },
            &usage_map,
            1_000,
        )
        .await
        .expect("first update should succeed");

        update_metrics(
            LiveMetrics {
                project_id: Stringy::from("project-a"),
                instance_id: Stringy::from("instance-1"),
                cpu_usage: 50.0,
                memory_mb: 10.0,
                rx_bytes: 0,
                tx_bytes: 0,
            },
            &usage_map,
            3_000,
        )
        .await
        .expect("second update should succeed");

        let map = usage_map.try_read().await.expect("map should be readable");
        let key = (Stringy::from("project-a"), Stringy::from("instance-1"));
        let entry = map.get(&key).expect("entry should exist");
        assert_eq!(entry.total_cpu_core_seconds, 1.0);
        assert_eq!(entry.last_sample_epoch_ms, Some(3_000));
    }

    #[tokio::test]
    async fn test_cpu_accrual_across_multi_sample_sequence_with_varying_load() {
        let usage_map: UsageMap = LockWithTimeout::new(HashMap::new());

        // (cpu_usage_percent, now_epoch_ms); expected accrual is elapsed-since-previous *
        // (cpu_usage / 100). First sample seeds the baseline (0 accrual).
        let samples = [(100.0, 0), (100.0, 1_000), (0.0, 2_000), (100.0, 5_000)];
        // Expected core-seconds: sample 1 -> 0 (baseline seed)
        //                        sample 2 -> 1.0 * 1s = 1.0
        //                        sample 3 -> 0.0 * 1s = 0.0
        //                        sample 4 -> 1.0 * 3s = 3.0
        // total = 4.0 core-seconds
        for (cpu_usage, now_epoch_ms) in samples {
            update_metrics(
                LiveMetrics {
                    project_id: Stringy::from("project-a"),
                    instance_id: Stringy::from("instance-1"),
                    cpu_usage,
                    memory_mb: 10.0,
                    rx_bytes: 0,
                    tx_bytes: 0,
                },
                &usage_map,
                now_epoch_ms,
            )
            .await
            .expect("update should succeed");
        }

        let map = usage_map.try_read().await.expect("map should be readable");
        let key = (Stringy::from("project-a"), Stringy::from("instance-1"));
        let entry = map.get(&key).expect("entry should exist");
        assert_eq!(entry.total_cpu_core_seconds, 4.0);
    }

    #[test]
    fn summarize_usage_excludes_legacy_schema_records_from_cpu_total() {
        use crate::aggregator::{
            summarize_usage, UsageRecord, USAGE_RECORD_SCHEMA_CURRENT, USAGE_RECORD_SCHEMA_V1,
        };

        let records = vec![
            // Legacy record: total_cpu here is a unitless raw percent-sum (the old bug),
            // and must NOT be folded into the corrected core-hours total.
            UsageRecord {
                timestamp_epoch: 1,
                schema_version: USAGE_RECORD_SCHEMA_V1,
                project_id: Stringy::from("project-a"),
                instance_id: Stringy::from("instance-1"),
                total_cpu: 999_999.0,
                peak_cpu: 100.0,
                total_memory: 100.0,
                peak_memory: 200.0,
                total_rx: 0,
                total_tx: 0,
                sample_count: 1,
            },
            // Current-schema record: total_cpu is genuine core-seconds (3600 = 1 core-hour).
            UsageRecord {
                timestamp_epoch: 2,
                schema_version: USAGE_RECORD_SCHEMA_CURRENT,
                project_id: Stringy::from("project-a"),
                instance_id: Stringy::from("instance-1"),
                total_cpu: 3600.0,
                peak_cpu: 100.0,
                total_memory: 100.0,
                peak_memory: 200.0,
                total_rx: 0,
                total_tx: 0,
                sample_count: 1,
            },
        ];

        let summary = summarize_usage(&records).expect("non-empty records must summarize");
        assert_eq!(
            summary.total_cpu, 1.0,
            "only the current-schema record's core-seconds should count, converted to 1 core-hour"
        );
    }

    #[test]
    fn status_as_str_name_round_trips_through_from_str_name() {
        use crate::aggregator::Status;

        let all = [
            Status::Starting,
            Status::Running,
            Status::Idle,
            Status::Stopping,
            Status::Stopped,
            Status::Warning,
            Status::Building,
            Status::Error,
            Status::Unknown,
        ];

        for status in all {
            let wire = status.as_str_name();
            let parsed = Status::from_str_name(wire).expect("known variant must parse");
            assert_eq!(status, parsed, "round trip failed for {wire}");
        }
    }

    #[test]
    fn status_from_str_name_is_case_insensitive() {
        use crate::aggregator::Status;

        assert_eq!(Status::from_str_name("running"), Ok(Status::Running));
        assert_eq!(Status::from_str_name("Running"), Ok(Status::Running));
        assert_eq!(Status::from_str_name("RUNNING"), Ok(Status::Running));
    }

    #[test]
    fn status_from_str_name_rejects_unrecognized_values_instead_of_silently_falling_back() {
        use crate::aggregator::Status;

        // This is the property the old `_ => Status::Unknown` match arms violated:
        // an unrecognized wire value must be an error the caller has to handle,
        // never silently coerced into a variant (Unknown included).
        let err = Status::from_str_name("definitely-not-a-status")
            .expect_err("unrecognized value must be Err, not a silent Unknown");
        assert_eq!(err.0, "definitely-not-a-status");
    }
}
