use std::error::Error;
use std::net::IpAddr;
use dusa_collection_utils::log;
use dusa_collection_utils::log::LogLevel;
use trust_dns_resolver::{
    config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts},
    AsyncResolver,
};

use crate::version::aml_version;

/// Resolves a given URL to its corresponding IP addresses using a DNS resolver. 
/// If no custom resolver address is provided, it defaults to `1.1.1.1` (Cloudflare).
///
/// # Arguments
/// 
/// * `url` - The URL (hostname) to resolve (e.g., "example.com").
/// * `resolver_addr` - An optional custom IP address for the DNS resolver. If `None`, `1.1.1.1` is used.
///
/// # Returns
/// 
/// * `Ok(Some(Vec<IpAddr>))` if the resolution succeeds, returning a vector of IP addresses.
/// * `Ok(None)` if the resolution fails to find any records or encounters an error during lookup.
/// * `Err(Box<dyn Error>)` if there is an error configuring the resolver (e.g., invalid IP address format).
///
/// # Example
/// ```rust,no_run
/// # use tokio::runtime::Runtime;
/// # let rt = Runtime::new().unwrap();
/// # rt.block_on(async {
///     match resolve_url("example.com", None).await {
///         Ok(Some(ips)) => {
///             for ip in ips {
///                 println!("Resolved IP: {}", ip);
///             }
///         }
///         Ok(None) => println!("No IP addresses resolved."),
///         Err(e) => eprintln!("Resolver configuration error: {}", e),
///     }
/// # });
/// ```
pub async fn resolve_url(url: &str, resolver_addr: Option<IpAddr>) -> Result<Option<Vec<IpAddr>>, Box<dyn Error>> {
    // Configure the resolver to use Cloudflare's DNS or the custom IP address
    let address: IpAddr = match resolver_addr {
        Some(given) => match given {
            IpAddr::V4(ipv4_addr) => IpAddr::from(ipv4_addr),
            IpAddr::V6(ipv6_addr) => IpAddr::from(ipv6_addr),
        },
        None => IpAddr::from([1, 1, 1, 1]),
    };

    // Construct "[address]:53" for the DNS server
    let resolver = format!("{}:53", address);

    let resolver_config = ResolverConfig::from_parts(
        None,        // Use the system domain
        vec![],      // No search list
        vec![NameServerConfig {
            socket_addr: resolver.parse()?,
            protocol: Protocol::Udp,
            tls_dns_name: None,
            trust_nx_responses: true,
            bind_addr: None,
        }],
    );
    let resolver_opts = ResolverOpts::default();

    // Create the async DNS resolver
    let resolver = AsyncResolver::tokio(resolver_config, resolver_opts)?;

    // Perform the lookup
    match resolver.lookup_ip(url).await {
        Ok(response) => {
            let ips: Vec<_> = response.iter().collect();
            Ok(Some(ips))
        },
        Err(err) => {
            log!(LogLevel::Error, "Failed to resolve {}: {}", url, err);
            Ok(None)
        },
    }
}

/// Retrieves the current AML (Artisan Manager Library) version and encodes it into a `u16`.
/// 
/// # Details
/// This function calls [`aml_version()`](crate::version::aml_version), which returns 
/// a version object that implements a custom `encode()` method. The method call converts 
/// the version data into a `u16`.
///
/// # Returns
/// A 16-bit unsigned integer representing the encoded version.
///
/// # Example
/// ```rust
/// let version_header = get_header_version();
/// println!("Header version: {}", version_header);
/// ```
pub fn get_header_version() -> u16 {
    let lib_version = aml_version();
    lib_version.encode()
}
