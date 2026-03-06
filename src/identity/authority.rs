//! Authority identity domain.
//!
//! Authority identity describes *who* is currently permitted to publish or
//! mutate canonical runtime state for a given runtime generation.
//!
//! This module is used to prevent split-brain writes between intermediates,
//! managers, and recovery paths by making authority explicit in payloads.

use dusa_collection_utils::{core::errors::ErrorArrayItem, core::types::stringy::Stringy};
use serde::{Deserialize, Serialize};

use crate::timestamp::current_timestamp;

use super::{node::Identifier, runtime::RuntimeId};

/// Identity for the actor currently authoritative for runtime state.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct AuthorityId(pub u64);

#[cfg(target_os = "linux")]
impl AuthorityId {
    /// Generates an authority key using the existing snowflake/identifier machinery.
    ///
    /// This keeps authority ID generation behavior aligned with node/runtime IDs.
    pub async fn generate() -> Result<Self, ErrorArrayItem> {
        Ok(Self(Identifier::new().await?.id))
    }
}

/// Known authority actor classes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum AuthorityKind {
    /// Runtime intermediate process currently publishing state.
    Intermediate,
    /// Lifecycle manager/service acting as authority.
    Manager,
    /// Recovery/takeover worker handling stale or failed intermediates.
    Recovery,
    /// Caller-defined authority class.
    Custom(Stringy),
}

/// Identity envelope describing who is allowed to publish canonical runtime state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AuthorityIdentity {
    /// Authority actor identifier.
    pub authority_id: AuthorityId,
    /// Runtime generation this authority is scoped to.
    pub runtime_id: RuntimeId,
    /// Class of authority actor.
    pub kind: AuthorityKind,
    /// Timestamp when authority was granted.
    pub granted_at: u64,
    /// Optional expiry timestamp for bounded authority sessions.
    pub expires_at: Option<u64>,
}

impl AuthorityIdentity {
    /// Creates an authority identity envelope from explicit values.
    pub fn new(
        authority_id: AuthorityId,
        runtime_id: RuntimeId,
        kind: AuthorityKind,
        granted_at: u64,
        expires_at: Option<u64>,
    ) -> Self {
        Self {
            authority_id,
            runtime_id,
            kind,
            granted_at,
            expires_at,
        }
    }
}

#[cfg(target_os = "linux")]
impl AuthorityIdentity {
    /// Generates a fresh authority identity for a runtime.
    ///
    /// `granted_at` is set to current time; caller controls optional `expires_at`.
    pub async fn generate(
        runtime_id: RuntimeId,
        kind: AuthorityKind,
        expires_at: Option<u64>,
    ) -> Result<Self, ErrorArrayItem> {
        Ok(Self {
            authority_id: AuthorityId::generate().await?,
            runtime_id,
            kind,
            granted_at: current_timestamp(),
            expires_at,
        })
    }
}
