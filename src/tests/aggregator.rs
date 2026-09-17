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

        for (rx, tx) in updates {
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
