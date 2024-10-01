#[cfg(test)]
mod tests {
    use crate::encryption::decrypt_text;
    use crate::git_actions::{GitAction, GitAuth, GitCredentials, GitServer};
    use dusa_collection_utils::stringy::Stringy;
    use dusa_collection_utils::types::PathType;
    use tempfile::{NamedTempFile, TempDir};
    use tokio::runtime::Runtime;

    /// Test for creating a new instance of GitCredentials.
    #[test]
    fn test_git_credentials_creation() {
        let git_auth = GitAuth {
            user: Stringy::new("test_user"),
            repo: Stringy::new("test_repo"),
            branch: Stringy::new("main"),
            server: GitServer::GitHub,
            token: Some(Stringy::new("test_token")),
        };

        let credentials = GitCredentials {
            auth_items: vec![git_auth.clone()],
        };

        // Assert that the credentials contain the expected data
        assert_eq!(credentials.auth_items.len(), 1);
        assert_eq!(credentials.auth_items[0], git_auth);
    }

    /// Test for adding a new GitAuth to GitCredentials.
    #[test]
    fn test_git_credentials_add_auth() {
        let mut credentials = GitCredentials { auth_items: vec![] };

        let git_auth = GitAuth {
            user: Stringy::new("test_user"),
            repo: Stringy::new("test_repo"),
            branch: Stringy::new("main"),
            server: GitServer::GitHub,
            token: Some(Stringy::new("test_token")),
        };

        // Add the new GitAuth
        credentials.add_auth(git_auth.clone());

        // Assert that the auth item was added correctly
        assert_eq!(credentials.auth_items.len(), 1);
        assert_eq!(credentials.auth_items[0], git_auth);
    }

    /// Test for GitAction - Mocked Git Clone action.
    #[test]
    fn test_git_action_clone() {
        let runtime = Runtime::new().unwrap();
        let git_dir = TempDir::new().expect("Couldn't create temp dir");

        let git_action = GitAction::Clone {
            repo_name: Stringy::new("doge"),
            repo_owner: Stringy::new("Dj-Codeman"),
            destination: PathType::Path(git_dir.path().to_path_buf().into()),
            repo_branch: Stringy::new("master"),
            server: GitServer::GitHub,
        };

        // Mock the execution of the Git action (this will not actually clone)
        let result = runtime.block_on(git_action.execute());

        // Check that the Git command would execute
        assert!(result.is_ok(), "Expected Git clone action to succeed");
    }

    /// Test for GitCredentials bootstrap_git_credentials.
    #[test]
    fn test_bootstrap_git_credentials() {
        // Bootstrap credentials
        let bootstrap_result = GitCredentials::bootstrap_git_credentials();

        assert!(bootstrap_result.is_ok(), "Expected bootstrap to succeed");

        // Load the bootstrapped credentials
        let credentials = bootstrap_result.unwrap();

        // Check that the credentials are empty since it was newly created
        assert!(
            credentials.auth_items.is_empty(),
            "Expected credentials to be empty"
        );
    }

    /// Test for saving, encrypting, and reading a file for GitCredentials.
    #[test]
    fn test_read_and_save_git_credentials_file() {
        // Create a sample GitCredentials
        let git_auth = GitAuth {
            user: Stringy::new("test_user"),
            repo: Stringy::new("test_repo"),
            branch: Stringy::new("main"),
            server: GitServer::GitHub,
            token: Some(Stringy::new("test_token")),
        };

        let credentials = GitCredentials {
            auth_items: vec![git_auth],
        };

        // Create a temporary file for testing
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = PathType::PathBuf(temp_file.path().to_path_buf());

        // Save the credentials
        let save_result = credentials.save(&file_path);
        assert!(save_result.is_ok(), "Expected save to succeed");

        // Read the file
        let read_result = GitCredentials::read_file(&file_path);
        assert!(read_result.is_ok(), "Expected read file to succeed");

        // Decrypt the read file content
        let decrypted_content = decrypt_text(read_result.unwrap()).unwrap();
        let parsed_credentials: GitCredentials = serde_json::from_str(&decrypted_content).unwrap();

        // Assert that the saved and read credentials are identical
        assert_eq!(credentials, parsed_credentials);

        // Clean up
        drop(temp_file);
    }
}
