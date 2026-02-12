use fraud_common::types::{BlockHeader, Transaction};
use serde::{Deserialize, Serialize};

/// Internal representation of a full block with transactions that came off
/// the provider. Converted into `PipelineEvent::NewBlock` before being sent
/// downstream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedBlock {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}
