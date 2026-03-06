//! Workload identity domain.
//!
//! Workload identity is the stable logical identity for a managed application.
//! It should remain constant across runtime restarts/rebuilds so orchestration,
//! billing, and historical tracking can refer to the same logical workload.
//!
//! This module also carries `source_id`, which is derived from source metadata
//! (currently git auth/project identity) and paired with `workload_id`.

use dusa_collection_utils::core::types::stringy::Stringy;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
use crate::git_actions::{generate_git_project_id, GitAuth};

use super::node::NodeId;

/// Stable workload identity key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct WorkloadId(pub Stringy);

impl WorkloadId {
    /// Creates a workload ID from a caller-provided stable value.
    ///
    /// Prefer a deterministic value so the same logical workload maps to the
    /// same `WorkloadId` across restarts.
    pub fn new(value: impl Into<Stringy>) -> Self {
        Self(value.into())
    }
}

#[cfg(target_os = "linux")]
impl WorkloadId {
    /// Derives workload identity from existing git-derived ID logic.
    pub fn from_git_auth(auth: &GitAuth) -> Self {
        Self(generate_git_project_id(auth))
    }
}

/// Identity envelope for workload-level naming.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct WorkloadIdentity {
    /// Node where this workload is managed.
    pub node_id: NodeId,
    /// Stable logical workload identifier.
    pub workload_id: WorkloadId,
    /// Source identity for this workload (currently git-derived).
    pub source_id: Stringy,
}

impl WorkloadIdentity {
    /// Creates a workload identity envelope from explicit IDs.
    pub fn new(node_id: NodeId, workload_id: WorkloadId, source_id: Stringy) -> Self {
        Self {
            node_id,
            workload_id,
            source_id,
        }
    }
}

#[cfg(target_os = "linux")]
impl WorkloadIdentity {
    /// Creates a workload identity using the existing git project hashing logic.
    ///
    /// This keeps `workload_id` and `source_id` aligned during migration by
    /// deriving both from the same source hash.
    pub fn from_git_auth(node_id: NodeId, auth: &GitAuth) -> Self {
        let source_id = generate_git_project_id(auth);
        Self {
            node_id,
            workload_id: WorkloadId(source_id.clone()),
            source_id,
        }
    }
}
