use std::net::IpAddr;

use dusa_collection_utils::{functions::{create_hash, truncate}, stringy::Stringy};
use serde::{Deserialize, Serialize};

use crate::{aggregator::AppStatus, identity::Identifier};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PortalMessage {
    Discover,
    IdRequest,
    IdResponse(Option<Identifier>),
    RegisterRequest(Identifier, IpAddr),
    RegisterResponse(bool),
    Error(String)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientInfo {
    pub identity: Identifier,
    pub address: IpAddr,
    pub last_updated: u64,
}

impl ClientInfo {
    pub fn get_stringy(&self) -> Stringy {
        let data = format!("{}_-_{}", self.address, self.identity.id);
        let hash = create_hash(data);
        let result = truncate(&hash, 20).to_owned();
        return Stringy::from(result);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectInfo {
    pub project_id: Stringy,
    pub identity: Identifier,
    pub project_data: AppStatus,
}

impl ProjectInfo {
    pub fn get_stringy(&self) -> Stringy {
        let data = format!("{}-{}-{}", self.identity.id, self.project_id, self.project_data.timestamp);
        let hash = create_hash(data);
        let result = truncate(&hash, 20).to_owned();
        return Stringy::from(result);
    }
}