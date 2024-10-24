// #[cfg(test)]
// mod tests {
//     use std::net::{IpAddr, Ipv4Addr};

//     use dusa_collection_utils::stringy::Stringy;

//     use crate::identity::{generate_key, Identity};

//     #[test]
//     fn test_generate_key() {
//         let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
//         let key = generate_key(&ip);
//         assert!(!key.is_empty(), "Key should not be empty");
//     }

//     #[test]
//     fn test_identity_creation() {
//         let identity = Identity::new(None);
//         assert!(!identity.encoded_ident.is_empty(), "Encoded identity should not be empty");
//         assert!(!identity.encrypted_key.is_empty(), "Encrypted key should not be empty");
//     }

//     #[test]
//     fn test_identity_encoding() {
//         let git_repos = vec![Stringy::from_string("https://github.com/user/repo".to_string())];
//         let identity_info = IdentityInfo {
//             ip_address: Stringy::from_string("192.168.1.1".to_owned()),
//             git_repositories: git_repos,
//         };
//         let encoded = identity_info.encode_identity().unwrap();
//         assert!(!encoded.is_empty(), "Encoded identity should not be empty");
//     }
// }
