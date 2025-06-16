#[cfg(all(test, target_os = "linux"))]
mod tests {
    use crate::network::resolve_url;

    #[tokio::test]
    async fn test_resolve_localhost() {
        let result = resolve_url("localhost", None).await.unwrap();
        assert!(result.unwrap().iter().any(|ip| ip.is_loopback()));
    }

    #[tokio::test]
    async fn test_resolve_invalid() {
        let result = resolve_url("invalid.invalid", None).await.unwrap();
        assert!(result.is_none());
    }
}
