#[cfg(test)]
mod tests {
    use crate::aggregator::Status;
    use crate::config::WorkloadConfig;
    use crate::identity::{
        AuthorityId, AuthorityIdentity, AuthorityKind, IdentityContext, NodeId, RuntimeId,
        RuntimeIdentity, WorkloadId, WorkloadIdentity,
    };
    use crate::state::{RuntimeState, WorkloadSnapshot};
    use crate::state_persistence::StatePersistence;
    use dusa_collection_utils::core::types::pathtype::PathType;
    use dusa_collection_utils::core::version::SoftwareVersion;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_save_and_load_state() {
        let state = RuntimeState {
            name: "test".into(),
            version: SoftwareVersion::dummy(),
            data: "data".into(),
            status: Status::Running,
            pid: 0,
            last_updated: 0,
            started_at: 0,
            event_counter: 0,
            error_log: vec![],
            system_application: false,
            stdout: vec![],
            stderr: vec![],
        };

        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("state.toml").into();

        StatePersistence::save_state(&state, &path).await.unwrap();

        let loaded = StatePersistence::load_state(&path).await.unwrap();
        assert_eq!(state, loaded);
    }

    #[tokio::test]
    async fn test_load_nonexistent_file() {
        let path: PathType = "/tmp/nonexistent_state.toml".into();
        let result = StatePersistence::load_state(&path).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_workload_config_default_is_v2() {
        let config = WorkloadConfig::dummy();
        assert!(config.as_v2().is_some());
    }

    #[tokio::test]
    async fn test_save_and_load_snapshot() {
        let config = WorkloadConfig::dummy();
        let runtime = RuntimeState {
            name: "test".into(),
            version: SoftwareVersion::dummy(),
            data: "data".into(),
            status: Status::Running,
            pid: 0,
            last_updated: 100,
            started_at: 99,
            event_counter: 2,
            error_log: vec![],
            system_application: false,
            stdout: vec![(1, "hello".to_string())],
            stderr: vec![(2, "world".to_string())],
        };

        let identity = IdentityContext::new(
            NodeId(1),
            WorkloadIdentity::new(NodeId(1), WorkloadId::new("workload"), "source".into()),
            RuntimeIdentity::new(
                NodeId(1),
                WorkloadId::new("workload"),
                RuntimeId(2),
                1,
                99,
                None,
            ),
            AuthorityIdentity::new(
                AuthorityId(3),
                RuntimeId(2),
                AuthorityKind::Manager,
                99,
                None,
            ),
        );
        let snapshot = WorkloadSnapshot::new(identity, config, runtime, None);

        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("snapshot.toml").into();
        StatePersistence::save_snapshot(&snapshot, &path)
            .await
            .unwrap();

        let loaded = StatePersistence::load_snapshot(&path).await.unwrap();
        assert_eq!(snapshot, loaded);
    }
}
