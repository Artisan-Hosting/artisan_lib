#[cfg(test)]
mod tests {
    use crate::git_actions::{generate_git_project_id, GitAuth, GitServer};
    use crate::identity::{
        AuthorityId, AuthorityIdentity, AuthorityKind, Identifier, NodeId, RuntimeId,
        RuntimeIdentity, WorkloadId, WorkloadIdentity, IDENTITY_RENAME_MAP,
    };
    use dusa_collection_utils::core::types::stringy::Stringy;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_identifier_new_and_verify() {
        let ident = Identifier::new().await.expect("create identifier");
        assert!(ident.verify().await, "identifier verification failed");
    }

    #[tokio::test]
    async fn test_identifier_json_roundtrip() {
        let ident = Identifier::new().await.unwrap();
        let json = ident.to_json().unwrap();
        let decoded: Identifier = serde_json::from_str(&json).unwrap();
        assert_eq!(ident, decoded);
    }

    #[test]
    fn test_identity_rename_map_contains_required_vocab() {
        assert!(IDENTITY_RENAME_MAP.contains(&("app_id", "workload_id")));
        assert!(IDENTITY_RENAME_MAP.contains(&("git_id", "source_id")));
        assert!(IDENTITY_RENAME_MAP.contains(&("NodeIdentity.id", "node_id")));
    }

    #[test]
    fn test_workload_id_derivation_reuses_git_hashing() {
        let auth = GitAuth {
            user: Stringy::from("user"),
            repo: Stringy::from("repo"),
            branch: Stringy::from("main"),
            server: GitServer::GitHub,
            token: None,
        };

        let expected = generate_git_project_id(&auth);
        let workload_id = WorkloadId::from_git_auth(&auth);
        let workload_identity = WorkloadIdentity::from_git_auth(NodeId(42), &auth);

        assert_eq!(workload_id.0, expected);
        assert_eq!(workload_identity.workload_id.0, expected);
        assert_eq!(workload_identity.source_id, expected);
        assert_eq!(workload_identity.node_id, NodeId(42));
    }

    #[test]
    fn test_workload_id_stability_for_same_source() {
        let auth = GitAuth {
            user: Stringy::from("stable-user"),
            repo: Stringy::from("stable-repo"),
            branch: Stringy::from("main"),
            server: GitServer::GitHub,
            token: None,
        };

        let first = WorkloadId::from_git_auth(&auth);
        let second = WorkloadId::from_git_auth(&auth);
        let third_identity = WorkloadIdentity::from_git_auth(NodeId(7), &auth);

        assert_eq!(first, second);
        assert_eq!(first, third_identity.workload_id);
        assert_eq!(first.0, third_identity.source_id);
    }

    #[tokio::test]
    async fn test_runtime_id_rotates_per_generation() {
        let workload_id = WorkloadId::new("workload-stable");
        let first = RuntimeIdentity::generate(NodeId(1), workload_id.clone(), 1)
            .await
            .unwrap();
        sleep(Duration::from_millis(2)).await;
        let second = RuntimeIdentity::generate(NodeId(1), workload_id.clone(), 2)
            .await
            .unwrap();

        assert_eq!(first.workload_id, workload_id);
        assert_eq!(second.workload_id, workload_id);
        assert_ne!(first.runtime_id, second.runtime_id);
        assert_ne!(first.generation, second.generation);
        assert!(first.ended_at.is_none());
        assert!(second.ended_at.is_none());
    }

    #[tokio::test]
    async fn test_runtime_and_authority_id_generation() {
        let runtime_id = RuntimeId::generate().await.unwrap();
        let authority_id = AuthorityId::generate().await.unwrap();

        assert!(runtime_id.0 > 0);
        assert!(authority_id.0 > 0);
    }

    #[test]
    fn test_authority_identity_validity_window() {
        let authority = AuthorityIdentity::new(
            AuthorityId(10),
            RuntimeId(20),
            AuthorityKind::Manager,
            1_000,
            Some(2_000),
        );
        assert!(!authority.is_valid_at(999));
        assert!(authority.is_valid_at(1_000));
        assert!(authority.is_valid_at(1_500));
        assert!(authority.is_valid_at(2_000));
        assert!(!authority.is_valid_at(2_001));
    }
}
