//! Identity domain root for AIS.
//!
//! This module defines the identity contract used across runtime orchestration,
//! state snapshots, and wire payloads. Identity is intentionally split into
//! four domains:
//! - [`node`]: durable machine/host identity
//! - [`workload`]: stable logical application identity
//! - [`runtime`]: one execution generation of a workload
//! - [`authority`]: actor currently allowed to publish canonical runtime state
//!
//! Keep this module as a type/contract layer. Process and persistence behavior
//! should be implemented by runtime/state modules that consume these types.

use serde::{Deserialize, Serialize};

pub mod authority;
pub mod node;
pub mod runtime;
pub mod workload;

pub use authority::{AuthorityId, AuthorityIdentity, AuthorityKind};
pub use node::{
    Identifier, NodeId, SnowflakeIDGenerator, CUSTOM_EPOCH, HASH_LENGTH, IDENTITYPATHSTR,
};
pub use runtime::{RuntimeId, RuntimeIdentity};
pub use workload::{WorkloadId, WorkloadIdentity};

/// Frozen rename contract for the identity reorganization work.
///
/// This is intentionally kept in-code so the vocabulary mapping is explicit
/// while migration slices are implemented module-by-module.
pub const IDENTITY_RENAME_MAP: [(&str, &str); 5] = [
    ("app_id", "workload_id"),
    ("git_id", "source_id"),
    ("NodeIdentity.id", "node_id"),
    ("runtime_id", "runtime_id"),
    ("authority_id", "authority_id"),
];

/// Full identity context attached to state snapshots and wire payloads.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct IdentityContext {
    /// Durable identity of the node where this context originated.
    pub node_id: NodeId,
    /// Stable workload identity (and source identity) for the managed application.
    pub workload: WorkloadIdentity,
    /// Current runtime generation identity for the workload.
    pub runtime: RuntimeIdentity,
    /// Actor currently authorized to publish canonical state for this runtime.
    pub authority: AuthorityIdentity,
}

impl IdentityContext {
    /// Constructs a full identity context used by snapshot/payload types.
    pub fn new(
        node_id: NodeId,
        workload: WorkloadIdentity,
        runtime: RuntimeIdentity,
        authority: AuthorityIdentity,
    ) -> Self {
        Self {
            node_id,
            workload,
            runtime,
            authority,
        }
    }
}
