#[cfg(test)]
mod tests {
    use crate::config::{AppConfig, GitConfig};
    use crate::git_actions::{GitServer, ARTISANCF};
    use crate::logger::LogLevel;
    use crate::state_persistence::{AppState, StatePersistence};
    use crate::version::{SoftwareVersion, Version};
    use dusa_collection_utils::types::PathType;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir; // Add `tempfile` to your Cargo.toml for a temporary directory.

    #[test]
    #[ignore = "Git runner pathing issues"]
    fn test_save_and_load_state() {
        // Create a mock state
        let app_config: AppConfig = AppConfig {
            app_name: dusa_collection_utils::stringy::Stringy::Mutable("Test app".to_owned()),
            version: serde_json::to_string(&SoftwareVersion::new(env!("CARGO_PKG_VERSION"))).unwrap(),
            max_cpu_usage: 0,
            max_ram_usage: 0,
            environment: "developer".to_owned(),
            debug_mode: false,
            git: Some(GitConfig {
                default_server: GitServer::GitHub,
                credentials_file: PathType::Str(ARTISANCF.into()).to_string(),
            }),
            database: None,
            aggregator: None,
            log_level: LogLevel::Debug,
        };

        let state = AppState {
            name: env!("CARGO_PKG_NAME").to_string(),
            data: String::from("data"),
            version: SoftwareVersion::dummy(),
            last_updated: 1234567,
            event_counter: 0,
            is_active: true,
            error_log: vec![],
            config: app_config,
        };

        // Use a temporary directory to store the state file
        let dir = tempdir().unwrap();
        let path: PathType = dir.path().join("test_state.json").into();
        println!("Temporary state path: {:?}", &path);

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
        let path: PathType = PathBuf::from("non_existent_file.json").into();
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
        let path: PathType = dir.path().join("invalid_state.json").into();

        // Write invalid data to the file
        fs::write(&path, "this is not valid encrypted or JSON data").unwrap();

        // Try loading the state
        let load_result = StatePersistence::load_state(&path);
        assert!(load_result.is_err(), "Loading invalid data should fail");
    }
}
