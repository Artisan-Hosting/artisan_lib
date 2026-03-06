//! Runtime identity domain.
//!
//! Runtime identity tracks one concrete execution generation of a workload.
//! Unlike `WorkloadId`, runtime identity is expected to rotate whenever a new
//! generation starts (restart, rebuild, respawn, or takeover).
//!
//! This allows the system to detect stale updates from older runs and to bind
//! metrics/logs/state updates to a specific generation.

use dusa_collection_utils::core::errors::ErrorArrayItem;
use serde::{Deserialize, Serialize};

use crate::timestamp::current_timestamp;

use super::{
    node::{Identifier, NodeId},
    workload::WorkloadId,
};

/// Ephemeral runtime generation key.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct RuntimeId(pub u64);

#[cfg(target_os = "linux")]
impl RuntimeId {
    /// Generates a runtime identity key using the existing snowflake/identifier machinery.
    ///
    /// Keeping this generation path aligned with [`Identifier`] avoids changing
    /// entropy/format characteristics during the reorganization.
    pub async fn generate() -> Result<Self, ErrorArrayItem> {
        Ok(Self(Identifier::new().await?.id))
    }
}

/// Identity envelope for one runtime generation of a workload.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RuntimeIdentity {
    /// Node where this runtime generation executes.
    pub node_id: NodeId,
    /// Stable workload this runtime belongs to.
    pub workload_id: WorkloadId,
    /// Ephemeral runtime generation identifier.
    pub runtime_id: RuntimeId,
    /// Monotonic generation number scoped to the workload.
    pub generation: u64,
    /// Start timestamp for this generation.
    pub started_at: u64,
    /// Optional end timestamp once runtime is terminated/retired.
    pub ended_at: Option<u64>,
}

impl RuntimeIdentity {
    /// Creates a runtime identity envelope from explicit values.
    pub fn new(
        node_id: NodeId,
        workload_id: WorkloadId,
        runtime_id: RuntimeId,
        generation: u64,
        started_at: u64,
        ended_at: Option<u64>,
    ) -> Self {
        Self {
            node_id,
            workload_id,
            runtime_id,
            generation,
            started_at,
            ended_at,
        }
    }
}

#[cfg(target_os = "linux")]
impl RuntimeIdentity {
    /// Generates a new runtime identity for a workload generation.
    ///
    /// `started_at` is set at creation time and `ended_at` is initialized as `None`.
    pub async fn generate(
        node_id: NodeId,
        workload_id: WorkloadId,
        generation: u64,
    ) -> Result<Self, ErrorArrayItem> {
        Ok(Self {
            node_id,
            workload_id,
            runtime_id: RuntimeId::generate().await?,
            generation,
            started_at: current_timestamp(),
            ended_at: None,
        })
    }
}
