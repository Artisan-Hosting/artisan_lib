#[cfg(test)]
mod tests {
    use crate::identity::Identifier;

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
}
