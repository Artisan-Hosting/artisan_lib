#[cfg(test)]
mod tests {
    use crate::config::{AppConfig, GitConfig};
    use crate::git_actions::{GitServer, ARTISANCF};
    use crate::state_persistence::{AppState, StatePersistence};
    use dusa_collection_utils::types::PathType;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir; // Add `tempfile` to your Cargo.toml for a temporary directory.

    #[test]
    fn test_save_and_load_state() {
        // Create a mock state
        let app_config: AppConfig = AppConfig {
            app_name: "Test app".to_owned(),
            version: "1.0.2".to_owned(),
            max_connections: 22,
            environment: "PORT=3306".to_owned(),
            debug_mode: false,
            git: Some(GitConfig {
                default_server: GitServer::GitHub,
                credentials_file: PathType::Str(ARTISANCF.into()).to_string(),
            }),
            database: None,
            aggregator: None,
        };

        let state = AppState {
            data: String::from("data"),
            last_updated: 1234567,
            event_counter: 0,
            is_active: true,
            error_log: vec![],
            config: app_config,
        };

        // Use a temporary directory to store the state file
        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("test_state.toml").into();

        // Save the state
        let save_result = StatePersistence::save_state(&state, &path);
        assert!(
            save_result.is_ok(),
            "Failed to save state: {:?}",
            save_result
        );

        // Load the state
        let load_result = StatePersistence::load_state(&path);
        assert!(
            load_result.is_ok(),
            "Failed to load state: {:?}",
            load_result
        );

        // Verify the loaded state matches the original state
        let loaded_state = load_result.unwrap();
        assert_eq!(
            state, loaded_state,
            "Loaded state does not match the original state"
        );
    }

    #[test]
    fn test_load_non_existent_file() {
        // Try loading from a non-existent file
        let path: PathType = PathBuf::from("non_existent_file.toml").into();
        let load_result = StatePersistence::load_state(&path);
        assert!(
            load_result.is_err(),
            "Loading non-existent file should fail"
        );
    }

    #[test]
    fn test_invalid_data_format() {
        // Use a temporary directory to create an invalid state file
        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("invalid_state.toml").into();

        // Write invalid data to the file
        fs::write(&path, "this is not valid encrypted or TOML data").unwrap();

        // Try loading the state
        let load_result = StatePersistence::load_state(&path);
        assert!(load_result.is_err(), "Loading invalid data should fail");
    }
}
