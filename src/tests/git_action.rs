#[cfg(test)]
mod tests {
    use crate::git_actions::{GitAction, GitAuth, GitCredentials, GitServer};
    use dusa_collection_utils::core::types::pathtype::PathType;
    use dusa_collection_utils::core::types::stringy::Stringy;

    #[test]
    fn test_git_auth_url_generation() {
        let auth = GitAuth {
            user: Stringy::from("user"),
            repo: Stringy::from("repo"),
            branch: Stringy::from("main"),
            server: GitServer::GitHub,
            token: None,
        };
        let url = auth.assemble_remote_url();
        assert!(url.contains("github.com/user/repo.git"));
    }

    #[tokio::test]
    async fn test_bootstrap_git_credentials() {
        let creds = GitCredentials::bootstrap_git_credentials()
            .await
            .expect("bootstrap");
        assert!(creds.auth_items.is_empty());
    }

    #[test]
    fn test_git_credentials_add() {
        let mut creds = GitCredentials { auth_items: vec![] };
        let auth = GitAuth {
            user: Stringy::from("user"),
            repo: Stringy::from("repo"),
            branch: Stringy::from("main"),
            server: GitServer::GitHub,
            token: None,
        };
        creds.add_auth(auth.clone());
        assert_eq!(creds.auth_items.len(), 1);
        assert_eq!(creds.auth_items[0], auth);
    }

    #[tokio::test]
    #[ignore]
    async fn test_git_action_clone_mock() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let action = GitAction::Clone {
            repo_name: Stringy::from("does_not_exist"),
            repo_owner: Stringy::from("none"),
            destination: PathType::PathBuf(dir.path().to_path_buf()),
            repo_branch: Stringy::from("main"),
            server: GitServer::GitHub,
        };
        // This should fail but return an error type
        let result = action.execute().await;
        assert!(result.is_err());
    }
}
