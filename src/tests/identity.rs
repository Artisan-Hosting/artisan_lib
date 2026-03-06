#[cfg(test)]
mod tests {
    use crate::git_actions::{generate_git_project_id, GitAuth, GitServer};
    use crate::identity::{
        AuthorityId, Identifier, NodeId, RuntimeId, WorkloadId, WorkloadIdentity,
        IDENTITY_RENAME_MAP,
    };
    use dusa_collection_utils::core::types::stringy::Stringy;

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

    #[tokio::test]
    async fn test_runtime_and_authority_id_generation() {
        let runtime_id = RuntimeId::generate().await.unwrap();
        let authority_id = AuthorityId::generate().await.unwrap();

        assert!(runtime_id.0 > 0);
        assert!(authority_id.0 > 0);
    }
}
