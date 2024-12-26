use std::error::Error;
use std::net::IpAddr;
use dusa_collection_utils::log;
use dusa_collection_utils::log::LogLevel;
use trust_dns_resolver::{config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts}, AsyncResolver};

use crate::version::aml_version;

pub async fn resolve_url(url: &str, resolver_addr: Option<IpAddr>) -> Result<Option<Vec<IpAddr>>, Box<dyn Error>> {
    // Configure the resolver to use Cloudflare's DNS

    let address: IpAddr = match resolver_addr {
        Some(given) => match given {
            IpAddr::V4(ipv4_addr) => IpAddr::from(ipv4_addr),
            IpAddr::V6(ipv6_addr) => IpAddr::from(ipv6_addr),
        },
        None => IpAddr::from([1, 1, 1, 1]),
    };

    // * Don't actually test this with an Ipv6 address
    let resolver = format!("{}:53", address);


    let resolver_config = ResolverConfig::from_parts(
        None, // Use the system domain
        vec![], // No search list
        vec![NameServerConfig {
            socket_addr: resolver.parse()?,
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

pub fn get_header_version() -> u16 {
    let lib_version = aml_version();
    lib_version.encode()
}