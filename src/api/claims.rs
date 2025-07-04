use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::roles::Role;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TokenType {
    Auth,
    Admin, // Not implemented
    Refresh,
    Password,
    None
}

impl TokenType {
    pub fn to_str(&self) -> &str {
        match self {
            TokenType::Auth => "auth",
            TokenType::Admin => "admin",
            TokenType::Refresh => "refresh",
            TokenType::Password => "password",
            TokenType::None => "",
        }
    }

    pub fn to_string(&self) -> String {
        self.to_str().to_owned()
    }

    pub fn from_str(data: &str) -> Self {
        match data {
            "auth" => Self::Auth,
            "admin" => Self::Admin,
            "refresh" => Self::Refresh,
            "password" => Self::Password,
            _ => Self::None
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PasswdClaims {
    pub sub: String,    // User ID
    pub exp: u64,       // Expiration timestamp
    pub kind: TokenType
}

/// JWT Claims structure.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,    // User ID
    pub role: Role,     // User role
    pub org_id: String, // Organization id
    pub exp: u64,       // Expiration timestamp
    pub kind: TokenType
}


impl Claims {
    /// Convert claims to a map of stringified key-values for gRPC response
    pub fn to_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("sub".into(), self.sub.clone());
        map.insert("org_id".into(), self.org_id.clone());
        map.insert("role".into(), self.role.to_str().to_owned());
        map.insert("exp".into(), self.exp.to_string());
        map.insert("type".into(), self.kind.to_string());
        map
    }
    /// Attempt to reconstruct `Claims` from a `HashMap<String, String>`
    pub fn from_map(mut map: HashMap<String, String>) -> Result<Self, String> {
        // Extract and remove entries from the map
        let sub = map.remove("sub").ok_or_else(|| "Missing `sub` in claims map".to_string())?;
        let org_id = map.remove("org_id").ok_or_else(|| "Missing `org_id` in claims map".to_string())?;
        let role_str = map.remove("role").ok_or_else(|| "Missing `role` in claims map".to_string())?;
        let exp_str = map.remove("exp").ok_or_else(|| "Missing `exp` in claims map".to_string())?;
        let kind_str = map.remove("type").ok_or_else(|| "Missing `kind` in claims map".to_string())?;

        // Parse exp as usize
        let exp = exp_str.parse::<u64>()
            .map_err(|e| format!("Failed to parse `exp`: {}", e))?;

        // Parse role from string (assuming Role implements FromStr)
        let role = Role::from_str(&role_str);

        // parse token type
        let kind = TokenType::from_str(&kind_str);

        Ok(Claims { sub, org_id, role, exp, kind })
    }
  
}