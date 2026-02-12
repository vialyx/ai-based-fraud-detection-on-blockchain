use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Blockchain primitives
// ---------------------------------------------------------------------------

/// A 20-byte Ethereum address stored as a hex string (0x-prefixed).
pub type Address = String;

/// A 32-byte transaction hash stored as a hex string (0x-prefixed).
pub type TxHash = String;

/// A 32-byte block hash stored as a hex string (0x-prefixed).
pub type BlockHash = String;

/// Minimal block header needed by the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub number: u64,
    pub hash: BlockHash,
    pub parent_hash: BlockHash,
    pub timestamp: u64,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub base_fee_per_gas: Option<u128>,
}

/// Single on-chain transaction with the fields the pipeline cares about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: TxHash,
    pub block_number: u64,
    pub from: Address,
    pub to: Option<Address>,
    pub value: u128,
    pub gas_price: Option<u128>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
    pub gas_used: u64,
    pub input: Vec<u8>,
    pub nonce: u64,
    pub tx_index: u64,
    pub timestamp: u64,
}

/// Log / event emitted by a smart contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub address: Address,
    pub topics: Vec<String>,
    pub data: Vec<u8>,
    pub tx_hash: TxHash,
    pub block_number: u64,
    pub log_index: u64,
}

// ---------------------------------------------------------------------------
// Feature & scoring types
// ---------------------------------------------------------------------------

/// A named numeric feature extracted from one transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    pub value: f64,
}

/// Complete feature vector for a single transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    pub tx_hash: TxHash,
    pub block_number: u64,
    pub features: Vec<Feature>,
    pub extracted_at: DateTime<Utc>,
}

/// Risk level assigned by the scorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "LOW"),
            RiskLevel::Medium => write!(f, "MEDIUM"),
            RiskLevel::High => write!(f, "HIGH"),
            RiskLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Fraud score produced by the scoring engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudScore {
    pub id: Uuid,
    pub tx_hash: TxHash,
    pub block_number: u64,
    pub score: f64,
    pub risk_level: RiskLevel,
    pub model_scores: Vec<ModelScore>,
    pub triggered_rules: Vec<String>,
    pub scored_at: DateTime<Utc>,
}

/// Individual model contribution to the ensemble score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelScore {
    pub model_name: String,
    pub score: f64,
    pub weight: f64,
}

// ---------------------------------------------------------------------------
// Alert types
// ---------------------------------------------------------------------------

/// An alert generated when a transaction exceeds the risk threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub fraud_score: FraudScore,
    pub from: Address,
    pub to: Option<Address>,
    pub value: u128,
    pub created_at: DateTime<Utc>,
    pub acknowledged: bool,
}

// ---------------------------------------------------------------------------
// Pipeline event – the message that flows between stages
// ---------------------------------------------------------------------------

/// Events flowing through the pipeline channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineEvent {
    /// New block with its transactions.
    NewBlock {
        header: BlockHeader,
        transactions: Vec<Transaction>,
    },
    /// Features extracted for a transaction.
    FeaturesExtracted(FeatureVector),
    /// Scoring complete for a transaction.
    Scored(FraudScore),
    /// Alert emitted.
    AlertRaised(Alert),
}
