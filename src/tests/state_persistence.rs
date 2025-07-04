#[cfg(test)]
mod tests {
    use crate::aggregator::Status;
    use crate::config::AppConfig;
    use crate::state_persistence::{AppState, StatePersistence};
    use dusa_collection_utils::core::types::pathtype::PathType;
    use dusa_collection_utils::core::version::SoftwareVersion;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_save_and_load_state() {
        let config = AppConfig::dummy();
        let state = AppState {
            name: "test".into(),
            version: SoftwareVersion::dummy(),
            data: "data".into(),
            status: Status::Running,
            pid: 0,
            last_updated: 0,
            stared_at: 0,
            event_counter: 0,
            error_log: vec![],
            config,
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
}
