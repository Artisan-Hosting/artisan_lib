use serde::{Deserialize, Serialize};

/// Response for token operations, including both access and refresh tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: usize,
    pub refresh_expires_in: usize,
}

#[derive(Debug, Deserialize)]
pub struct SimpleLoginRequest {
    pub email: String,
    pub password: String,
}
