use std::{error::Error, net::IpAddr};

use dusa_collection_utils::log;
use dusa_collection_utils::log::LogLevel;
use trust_dns_resolver::{config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts}, AsyncResolver};

pub async fn resolve_url(url: &str) -> Result<Option<Vec<IpAddr>>, Box<dyn Error>> {
    // Configure the resolver to use Cloudflare's DNS
    let resolver_config = ResolverConfig::from_parts(
        None, // Use the system domain
        vec![], // No search list
        vec![NameServerConfig {
            socket_addr: "1.1.1.1:53".parse()?, // Cloudflare DNS
            protocol: Protocol::Udp,
            tls_dns_name: None,
            trust_nx_responses: true,
            bind_addr: None,
        }],
    );
    let resolver_opts = ResolverOpts::default();

    // Create the resolver
    let resolver = AsyncResolver::tokio(resolver_config, resolver_opts)?;

    
    match resolver.lookup_ip(url).await {
        Ok(response) => {
            let ips: Vec<_> = response.iter().collect();
            return Ok(Some(ips))
        },
        Err(err) => {
            log!(LogLevel::Error, "Failed to resolve {}: {}", url, err);
            return Ok(None)
        },
    }
}