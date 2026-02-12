use crate::provider::RpcProvider;
use fraud_common::config::IndexerConfig;
use fraud_common::error::Error;
use fraud_common::types::PipelineEvent;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Continuously streams new blocks into the pipeline channel.
pub struct BlockStream {
    provider: RpcProvider,
    poll_interval: Duration,
    start_block: Option<u64>,
}

impl BlockStream {
    pub async fn new(config: &IndexerConfig) -> Result<Self, Error> {
        let provider = RpcProvider::new(config).await?;
        Ok(Self {
            provider,
            poll_interval: Duration::from_millis(config.poll_interval_ms),
            start_block: config.start_block,
        })
    }

    /// Start polling for new blocks and forward them into `tx`.
    /// Runs forever until the channel is closed or an unrecoverable error
    /// occurs.
    pub async fn run(self, tx: mpsc::Sender<PipelineEvent>) -> Result<(), Error> {
        let mut cursor = match self.start_block {
            Some(n) => n,
            None => self
                .provider
                .get_latest_block_number()
                .await
                .map_err(|e| Error::Indexer(format!("failed to get start block: {e}")))?,
        };

        info!(start_block = cursor, "block stream started");

        loop {
            match self.provider.get_latest_block_number().await {
                Ok(latest) => {
                    while cursor <= latest {
                        match self.provider.get_block(cursor).await {
                            Ok(Some((header, transactions))) => {
                                let tx_count = transactions.len();
                                let event = PipelineEvent::NewBlock {
                                    header,
                                    transactions,
                                };
                                if tx.send(event).await.is_err() {
                                    warn!("pipeline channel closed, stopping indexer");
                                    return Err(Error::ChannelClosed);
                                }
                                info!(
                                    block = cursor,
                                    txs = tx_count,
                                    "indexed block"
                                );
                            }
                            Ok(None) => {
                                warn!(block = cursor, "block not found, skipping");
                            }
                            Err(e) => {
                                error!(block = cursor, error = %e, "failed to fetch block");
                            }
                        }
                        cursor += 1;
                    }
                }
                Err(e) => {
                    error!(error = %e, "failed to poll latest block number");
                }
            }

            tokio::time::sleep(self.poll_interval).await;
        }
    }
}
