use dusa_collection_utils::{core::errors::ErrorArrayItem, core::types::stringy::Stringy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use crate::aggregator::Metrics;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoricalUsage {
    pub total_cpu_time: f64,
    pub total_memory_bytes: u64,
    pub total_net_rx: u64,
    pub total_net_tx: u64,
    pub last_metrics: Option<Metrics>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageLedger {
    pub applications: HashMap<Stringy, HistoricalUsage>, // PID-based for now
}

impl UsageLedger {
    pub fn new() -> Self {
        Self {
            applications: HashMap::new(),
        }
    }

    pub fn update_application_usage(&mut self, app_id: Stringy, current: Metrics) {
        let entry = self.applications.entry(app_id).or_insert(HistoricalUsage {
            total_cpu_time: 0.0,
            total_memory_bytes: 0,
            total_net_rx: 0,
            total_net_tx: 0,
            last_metrics: None,
        });

        if let Some(last) = &entry.last_metrics {
            let cpu_delta = (current.cpu_usage - last.cpu_usage).max(0.0) as f64;
            let mem_delta = current.memory_usage as u64;
            let net_rx_delta = current
                .other
                .as_ref()
                .map(|net| {
                    net.rx_bytes
                        .saturating_sub(last.other.as_ref().map(|n| n.rx_bytes).unwrap_or(0))
                })
                .unwrap_or(0);
            let net_tx_delta = current
                .other
                .as_ref()
                .map(|net| {
                    net.tx_bytes
                        .saturating_sub(last.other.as_ref().map(|n| n.tx_bytes).unwrap_or(0))
                })
                .unwrap_or(0);

            entry.total_cpu_time += cpu_delta;
            entry.total_memory_bytes = entry.total_memory_bytes.max(mem_delta);
            entry.total_net_rx += net_rx_delta;
            entry.total_net_tx += net_tx_delta;
        }

        entry.last_metrics = Some(current);
    }

    pub fn persist_to_disk(&self, path: &str) -> Result<(), ErrorArrayItem> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_disk(path: &str) -> Result<Self, ErrorArrayItem> {
        let data = fs::read_to_string(path)?;
        let ledger = serde_json::from_str(&data)?;
        Ok(ledger)
    }
}
