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
