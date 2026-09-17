#[cfg(test)]
mod tests {
    use crate::urn::{ResourceType, Urn, UrnParseError};

    #[test]
    fn simple_urns_round_trip() {
        let cases = [
            (ResourceType::Organization, "550e8400-e29b-41d4-a716-446655440000"),
            (ResourceType::Project, "63c35f4b"),
            (ResourceType::Node, "42"),
            (ResourceType::Domain, "17"),
            (ResourceType::Environment, "prod"),
            (ResourceType::Vm, "100"),
        ];

        for (resource_type, id) in cases {
            let urn = Urn::new(resource_type, id);
            let formatted = urn.to_string();
            let parsed = Urn::parse(&formatted).expect("must parse what we formatted");
            assert_eq!(parsed, urn);
            assert_eq!(parsed.resource_type(), resource_type);
            assert_eq!(parsed.id(), id);
        }
    }

    #[test]
    fn composite_secret_urn_keeps_its_slashes() {
        // A "/"-joined composite id inside the id segment must not be mistaken
        // for another ":"-delimited grammar segment -- Urn::parse splits on the
        // first three ':' only.
        let urn = Urn::new(ResourceType::Secret, "63c35f4b/prod/API_KEY");
        let formatted = urn.to_string();
        assert_eq!(formatted, "urn:artisan:secret:63c35f4b/prod/API_KEY");

        let parsed = Urn::parse(&formatted).unwrap();
        assert_eq!(parsed.id(), "63c35f4b/prod/API_KEY");
    }

    #[test]
    fn parse_rejects_missing_prefix() {
        let err = Urn::parse("not-a-urn:artisan:project:abc").unwrap_err();
        assert!(matches!(err, UrnParseError::Malformed(_)));
    }

    #[test]
    fn parse_rejects_unknown_resource_type() {
        let err = Urn::parse("urn:artisan:runner:abc").unwrap_err();
        assert!(matches!(err, UrnParseError::UnknownResourceType(_)));
    }

    #[test]
    fn parse_rejects_empty_id() {
        let err = Urn::parse("urn:artisan:project:").unwrap_err();
        assert!(matches!(err, UrnParseError::Malformed(_)));
    }
}
