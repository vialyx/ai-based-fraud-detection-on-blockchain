use chrono::Utc;
use fraud_common::error::Error;
use fraud_common::types::{Feature, FeatureVector, PipelineEvent, Transaction};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::graph::AddressGraph;
use crate::stats::RollingStats;

/// Number of past observations kept for rolling statistics.
const ROLLING_WINDOW: usize = 256;

/// Extracts numeric features from raw transactions.
///
/// Maintains internal state (rolling stats, address graph) so features become
/// richer over time.
pub struct FeatureExtractor {
    value_stats: RollingStats,
    gas_stats: RollingStats,
    graph: AddressGraph,
}

impl FeatureExtractor {
    pub fn new() -> Self {
        Self {
            value_stats: RollingStats::new(ROLLING_WINDOW),
            gas_stats: RollingStats::new(ROLLING_WINDOW),
            graph: AddressGraph::new(),
        }
    }

    /// Extract features for a single transaction.
    pub fn extract(&mut self, tx: &Transaction) -> FeatureVector {
        // Update rolling statistics
        let value_eth = tx.value as f64 / 1e18;
        self.value_stats.push(value_eth);
        let gas = tx.gas_used as f64;
        self.gas_stats.push(gas);

        // Update address graph
        if let Some(ref to) = tx.to {
            self.graph.record_transfer(&tx.from, to);
        }

        let mut features = Vec::with_capacity(16);

        // ---- Value features ----
        features.push(Feature {
            name: "value_eth".into(),
            value: value_eth,
        });
        features.push(Feature {
            name: "value_log".into(),
            value: (value_eth + 1.0).ln(),
        });
        features.push(Feature {
            name: "value_z_score".into(),
            value: self.value_stats.z_score(),
        });

        // ---- Gas features ----
        features.push(Feature {
            name: "gas_used".into(),
            value: gas,
        });
        features.push(Feature {
            name: "gas_z_score".into(),
            value: self.gas_stats.z_score(),
        });
        features.push(Feature {
            name: "gas_price_gwei".into(),
            value: tx.gas_price.unwrap_or(0) as f64 / 1e9,
        });

        // ---- Input data features ----
        features.push(Feature {
            name: "input_size".into(),
            value: tx.input.len() as f64,
        });
        features.push(Feature {
            name: "is_contract_call".into(),
            value: if tx.input.len() > 4 { 1.0 } else { 0.0 },
        });
        features.push(Feature {
            name: "is_contract_creation".into(),
            value: if tx.to.is_none() { 1.0 } else { 0.0 },
        });

        // ---- Graph features (sender) ----
        features.push(Feature {
            name: "sender_out_degree".into(),
            value: self.graph.out_degree(&tx.from) as f64,
        });
        features.push(Feature {
            name: "sender_out_tx_count".into(),
            value: self.graph.out_tx_count(&tx.from) as f64,
        });
        features.push(Feature {
            name: "sender_fan_out_ratio".into(),
            value: self.graph.fan_out_ratio(&tx.from),
        });

        // ---- Graph features (receiver) ----
        if let Some(ref to) = tx.to {
            features.push(Feature {
                name: "receiver_in_degree".into(),
                value: self.graph.in_degree(to) as f64,
            });
            features.push(Feature {
                name: "receiver_fan_in_ratio".into(),
                value: self.graph.fan_in_ratio(to),
            });
        }

        // ---- Nonce features ----
        features.push(Feature {
            name: "nonce".into(),
            value: tx.nonce as f64,
        });
        features.push(Feature {
            name: "is_first_tx".into(),
            value: if tx.nonce == 0 { 1.0 } else { 0.0 },
        });

        FeatureVector {
            tx_hash: tx.hash.clone(),
            block_number: tx.block_number,
            features,
            extracted_at: Utc::now(),
        }
    }

    /// Pipeline stage: reads `NewBlock` events from `rx`, extracts features
    /// for every transaction, and sends `FeaturesExtracted` events to `tx`.
    pub async fn run(
        mut self,
        mut rx: mpsc::Receiver<PipelineEvent>,
        tx: mpsc::Sender<PipelineEvent>,
    ) -> Result<(), Error> {
        info!("feature extractor started");

        while let Some(event) = rx.recv().await {
            if let PipelineEvent::NewBlock {
                header,
                transactions,
            } = event
            {
                for txn in &transactions {
                    let fv = self.extract(txn);
                    let out = PipelineEvent::FeaturesExtracted(fv);
                    if tx.send(out).await.is_err() {
                        warn!("downstream channel closed");
                        return Err(Error::ChannelClosed);
                    }
                }
                info!(
                    block = header.number,
                    txs = transactions.len(),
                    "features extracted"
                );
            }
        }

        Ok(())
    }
}

impl Default for FeatureExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_tx() -> Transaction {
        Transaction {
            hash: "0xdeadbeef".into(),
            block_number: 100,
            from: "0xsender".into(),
            to: Some("0xreceiver".into()),
            value: 5_000_000_000_000_000_000, // 5 ETH
            gas_price: Some(20_000_000_000),   // 20 gwei
            max_fee_per_gas: Some(30_000_000_000),
            max_priority_fee_per_gas: Some(1_000_000_000),
            gas_used: 21000,
            input: vec![0xa9, 0x05, 0x9c, 0xbb, 0x01, 0x02], // >4 bytes → contract call
            nonce: 0,
            tx_index: 0,
            timestamp: 1700000000,
        }
    }

    #[test]
    fn extract_produces_expected_features() {
        let mut ext = FeatureExtractor::new();
        let tx = mock_tx();
        let fv = ext.extract(&tx);

        assert_eq!(fv.tx_hash, "0xdeadbeef");
        assert_eq!(fv.block_number, 100);

        let get = |name: &str| -> f64 {
            fv.features.iter().find(|f| f.name == name).map(|f| f.value).unwrap_or(f64::NAN)
        };

        // value_eth should be ~5.0
        assert!((get("value_eth") - 5.0).abs() < 1e-9);

        // gas_used should be 21000
        assert!((get("gas_used") - 21000.0).abs() < 1e-9);

        // gas_price_gwei should be 20.0
        assert!((get("gas_price_gwei") - 20.0).abs() < 1e-9);

        // is_contract_call should be 1.0 (input > 4 bytes)
        assert!((get("is_contract_call") - 1.0).abs() < 1e-9);

        // is_contract_creation should be 0.0 (to is Some)
        assert!((get("is_contract_creation") - 0.0).abs() < 1e-9);

        // is_first_tx should be 1.0 (nonce == 0)
        assert!((get("is_first_tx") - 1.0).abs() < 1e-9);

        // nonce should be 0
        assert!((get("nonce") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn contract_creation_when_to_is_none() {
        let mut ext = FeatureExtractor::new();
        let mut tx = mock_tx();
        tx.to = None;
        let fv = ext.extract(&tx);

        let is_creation = fv.features.iter().find(|f| f.name == "is_contract_creation")
            .map(|f| f.value).unwrap();
        assert!((is_creation - 1.0).abs() < 1e-9);

        // receiver features should not be present
        assert!(fv.features.iter().find(|f| f.name == "receiver_in_degree").is_none());
    }

    #[test]
    fn not_contract_call_when_input_short() {
        let mut ext = FeatureExtractor::new();
        let mut tx = mock_tx();
        tx.input = vec![]; // empty input → not a contract call
        let fv = ext.extract(&tx);

        let is_call = fv.features.iter().find(|f| f.name == "is_contract_call")
            .map(|f| f.value).unwrap();
        assert!((is_call - 0.0).abs() < 1e-9);
    }

    #[test]
    fn z_scores_improve_with_history() {
        let mut ext = FeatureExtractor::new();
        // Push many similar transactions to build statistics
        for _ in 0..50 {
            let tx = mock_tx();
            ext.extract(&tx);
        }

        // Now push an outlier
        let mut outlier = mock_tx();
        outlier.value = 500_000_000_000_000_000_000; // 500 ETH
        outlier.gas_used = 5_000_000;
        let fv = ext.extract(&outlier);

        let value_z = fv.features.iter().find(|f| f.name == "value_z_score")
            .map(|f| f.value).unwrap();
        // Outlier should have a high z-score
        assert!(value_z > 1.0, "value z-score should be high for outlier: {value_z}");
    }

    #[test]
    fn graph_features_accumulate() {
        let mut ext = FeatureExtractor::new();
        let mut tx = mock_tx();

        // First extraction: sender out_degree should be 1
        let fv1 = ext.extract(&tx);
        let out_deg = fv1.features.iter().find(|f| f.name == "sender_out_degree")
            .map(|f| f.value).unwrap();
        assert!((out_deg - 1.0).abs() < 1e-9);

        // Send to a different address
        tx.to = Some("0xother".into());
        tx.hash = "0xdeadbeef2".into();
        let fv2 = ext.extract(&tx);
        let out_deg2 = fv2.features.iter().find(|f| f.name == "sender_out_degree")
            .map(|f| f.value).unwrap();
        assert!((out_deg2 - 2.0).abs() < 1e-9);
    }
}
