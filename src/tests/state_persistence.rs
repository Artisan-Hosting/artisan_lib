#[cfg(test)]
mod tests {
    use crate::aggregator::Status;
    use crate::config::WorkloadConfig;
    use crate::encryption::simple_encrypt;
    use crate::identity::{
        AuthorityId, AuthorityIdentity, AuthorityKind, IdentityContext, NodeId, RuntimeId,
        RuntimeIdentity, WorkloadId, WorkloadIdentity,
    };
    use crate::state::{RuntimeState, WorkloadSnapshot};
    use crate::state_persistence::StatePersistence;
    use dusa_collection_utils::core::errors::ErrorArrayItem;
    use dusa_collection_utils::core::errors::Errors;
    use dusa_collection_utils::core::types::pathtype::PathType;
    use dusa_collection_utils::core::version::SoftwareVersion;
    use std::fs;
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

    #[tokio::test]
    async fn test_load_state_rejects_legacy_extra_fields() {
        let state = RuntimeState {
            name: "test".into(),
            version: SoftwareVersion::dummy(),
            data: "data".into(),
            status: Status::Running,
            pid: 42,
            last_updated: 100,
            started_at: 90,
            event_counter: 7,
            error_log: vec![],
            system_application: false,
            stdout: vec![],
            stderr: vec![],
        };

        let legacy_toml = format!(
            "config = \"legacy-mixed-shape\"\n{}",
            toml::to_string(&state).unwrap()
        );

        let encrypted = simple_encrypt(legacy_toml.as_bytes()).unwrap();
        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("legacy_state.toml").into();
        fs::write(&path, encrypted.as_bytes()).unwrap();

        let result = StatePersistence::load_state(&path).await;
        assert!(
            result.is_err(),
            "legacy mixed runtime+config shape should be rejected"
        );
    }

    #[tokio::test]
    async fn test_load_snapshot_rejects_unknown_top_level_fields() {
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
            stdout: vec![],
            stderr: vec![],
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

        let toml_data = format!(
            "app_id = \"legacy-field\"\n{}",
            toml::to_string(&snapshot).unwrap()
        );

        let encrypted = simple_encrypt(toml_data.as_bytes()).unwrap();
        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("legacy_snapshot.toml").into();
        fs::write(&path, encrypted.as_bytes()).unwrap();

        let result = StatePersistence::load_snapshot(&path).await;
        assert!(
            result.is_err(),
            "snapshot payloads with unknown top-level fields should be rejected"
        );
    }

    #[tokio::test]
    async fn test_save_and_load_state_with_output_streams() {
        let state = RuntimeState {
            name: "output-test".into(),
            version: SoftwareVersion::dummy(),
            data: "test data".into(),
            status: Status::Running,
            pid: 1234,
            last_updated: 1000,
            started_at: 900,
            event_counter: 5,
            error_log: vec![],
            system_application: true,
            stdout: vec![
                (100, "stdout line 1".to_string()),
                (101, "stdout line 2".to_string()),
            ],
            stderr: vec![
                (200, "stderr line 1".to_string()),
                (201, "stderr line 2".to_string()),
            ],
        };

        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("output_state.toml").into();

        StatePersistence::save_state(&state, &path).await.unwrap();

        let loaded = StatePersistence::load_state(&path).await.unwrap();
        assert_eq!(state, loaded);
    }

    #[tokio::test]
    async fn test_save_load_empty_state() {
        let state = RuntimeState {
            name: "empty".into(),
            version: SoftwareVersion::dummy(),
            data: "".into(),
            status: Status::Building,
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
        let path: PathType = dir.path().join("empty_state.toml").into();

        StatePersistence::save_state(&state, &path).await.unwrap();

        let loaded = StatePersistence::load_state(&path).await.unwrap();
        assert_eq!(state, loaded);
    }

    #[tokio::test]
    async fn test_save_load_state_with_errors() {
        let error1 = ErrorArrayItem::new(Errors::GeneralError, "Error 1".to_string());
        let error2 = ErrorArrayItem::new(Errors::GeneralError, "Error 2".to_string());
        
        let state = RuntimeState {
            name: "error-test".into(),
            version: SoftwareVersion::dummy(),
            data: "error test".into(),
            status: Status::Warning,
            pid: 5678,
            last_updated: 2000,
            started_at: 1900,
            event_counter: 10,
            error_log: vec![error1, error2],
            system_application: false,
            stdout: vec![],
            stderr: vec![],
        };

        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("error_state.toml").into();

        StatePersistence::save_state(&state, &path).await.unwrap();

        let loaded = StatePersistence::load_state(&path).await.unwrap();
        assert_eq!(state, loaded);
    }

    #[tokio::test]
    async fn test_load_invalid_encryption() {
        let invalid_content = b"this is not valid encrypted data";
        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("invalid_encryption.toml").into();
        fs::write(&path, invalid_content).unwrap();

        let result = StatePersistence::load_state(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_invalid_toml() {
        let valid_encrypted = simple_encrypt(b"not valid toml content").unwrap();
        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("invalid_toml.toml").into();
        fs::write(&path, valid_encrypted.as_bytes()).unwrap();

        let result = StatePersistence::load_state(&path).await;
        assert!(result.is_err());
    }
}
