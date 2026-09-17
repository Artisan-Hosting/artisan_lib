#[cfg(test)]
mod tests {
    use crate::identity::{
        ais_name, generate_project_id, strip_ais_prefix, DomainId, EnvironmentId, NodeId,
        OrganizationId, ProjectId, SecretRef,
    };

    #[test]
    fn project_id_matches_the_known_test_vectors() {
        // Same vectors documented in RESOURCE_TAXONOMY.md §4.1 -- genuinely
        // computed sha256("{branch}-{repo}-{user}")[0..8] values, verified
        // independently (not hand-derived), so this pins the canonical
        // generate_project_id/ProjectId::from_parts algorithm against a
        // known-correct answer rather than against itself.
        let cases = [
            ("user1", "website", "main", "63eff31e"),
            ("acme-org", "api", "develop", "920b821c"),
            ("acme-org", "backend", "release-v2", "3e2223db"),
            ("artisans", "portal", "main", "1990e9a5"),
        ];

        for (user, repo, branch, expected) in cases {
            let id = generate_project_id(user, repo, branch);
            assert_eq!(id.as_str(), expected, "for {branch}-{repo}-{user}");
        }
    }

    #[test]
    fn project_id_generation_is_deterministic() {
        let a = generate_project_id("user", "repo", "main");
        let b = generate_project_id("user", "repo", "main");
        assert_eq!(a, b);
    }

    #[test]
    fn project_id_parse_accepts_only_8_lowercase_hex_chars() {
        assert!(ProjectId::parse("63c35f4b").is_ok());
        assert!(ProjectId::parse("63C35F4B").is_err(), "uppercase must be rejected");
        assert!(ProjectId::parse("63c35f4").is_err(), "too short must be rejected");
        assert!(ProjectId::parse("63c35f4bb").is_err(), "too long must be rejected");
        assert!(ProjectId::parse("not-hex!").is_err());
    }

    #[test]
    fn ais_name_and_strip_ais_prefix_are_inverses() {
        // This is the exact property the old, non-prefix-anchored
        // `.replace("ais_", "")` call sites violated.
        let cases = ["63c35f4b", "manager", "gitmon", "contains_ais_inside"];
        for component in cases {
            let named = ais_name(component);
            assert_eq!(strip_ais_prefix(&named), Some(component));
        }
    }

    #[test]
    fn strip_ais_prefix_only_matches_a_leading_prefix() {
        // A name that merely *contains* "ais_" elsewhere must not be mangled --
        // unlike `.replace("ais_", "")`, which is not prefix-anchored.
        assert_eq!(strip_ais_prefix("not_ais_prefixed"), None);
        assert_eq!(strip_ais_prefix("ais_ais_double"), Some("ais_double"));
    }

    #[test]
    fn project_id_ais_name_round_trips() {
        let id = generate_project_id("user", "repo", "main");
        let process_name = id.ais_name();
        assert_eq!(strip_ais_prefix(&process_name), Some(id.as_str()));
    }

    #[test]
    fn organization_id_round_trips_as_uuid_string() {
        let org = OrganizationId::new_v4();
        let s = org.to_string();
        let parsed = OrganizationId::parse(&s).expect("must parse what we generated");
        assert_eq!(org, parsed);
        assert!(OrganizationId::parse("not-a-uuid").is_err());
    }

    #[test]
    fn node_id_wire_encoding_is_a_decimal_string() {
        let id = NodeId(42);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"42\"", "NodeId must serialize as a decimal string, not a bare number");

        let round_tripped: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, id);
    }

    #[test]
    fn node_id_deserialize_also_accepts_a_bare_number() {
        // Lenient on decode (accepts either a JSON string or number) while
        // strict on encode -- a permissive-in/strict-out wire contract.
        let from_number: NodeId = serde_json::from_str("42").unwrap();
        let from_string: NodeId = serde_json::from_str("\"42\"").unwrap();
        assert_eq!(from_number, from_string);
        assert_eq!(from_number, NodeId(42));
    }

    #[test]
    fn domain_id_wire_encoding_matches_node_id() {
        let id = DomainId(17);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"17\"");
    }

    #[test]
    fn secret_ref_display_matches_urn_id_segment() {
        let secret = SecretRef {
            project_id: ProjectId::parse("63c35f4b").unwrap(),
            environment_id: EnvironmentId::new("prod"),
            key: "API_KEY".to_owned(),
        };
        assert_eq!(secret.to_string(), "63c35f4b/prod/API_KEY");
    }
}
