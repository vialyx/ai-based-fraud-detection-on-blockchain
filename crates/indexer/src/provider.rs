use alloy::providers::{Provider, ProviderBuilder};
use fraud_common::config::IndexerConfig;
use fraud_common::error::Error;
use fraud_common::types::{BlockHeader, Transaction};
use tracing::info;

/// Wrapper around an Alloy RPC provider.
pub struct RpcProvider {
    provider: Box<dyn Provider + Send + Sync>,
}

impl RpcProvider {
    /// Create a new provider from the indexer configuration.
    pub async fn new(config: &IndexerConfig) -> Result<Self, Error> {
        let url: alloy::transports::http::reqwest::Url =
            config.rpc_url.parse().map_err(|e| {
                Error::Indexer(format!("invalid RPC URL '{}': {e}", config.rpc_url))
            })?;

        let provider = ProviderBuilder::new().connect_http(url);

        info!(rpc_url = %config.rpc_url, "connected to RPC provider");
        Ok(Self {
            provider: Box::new(provider),
        })
    }

    /// Fetch the latest block number.
    pub async fn get_latest_block_number(&self) -> Result<u64, Error> {
        self.provider
            .get_block_number()
            .await
            .map_err(|e| Error::Indexer(format!("failed to get block number: {e}")))
    }

    /// Fetch a full block by number, returning header + transactions.
    ///
    /// For each transaction we also request its receipt so we can record the
    /// *actual* gas consumed (`gas_used`) rather than the block-level total.
    pub async fn get_block(
        &self,
        block_number: u64,
    ) -> Result<Option<(BlockHeader, Vec<Transaction>)>, Error> {
        use alloy::consensus::Transaction as TxTrait;
        use alloy::eips::{BlockId, BlockNumberOrTag};

        let block = self
            .provider
            .get_block_by_number(BlockNumberOrTag::Number(block_number))
            .await
            .map_err(|e| Error::Indexer(format!("failed to fetch block {block_number}: {e}")))?;

        let Some(block) = block else {
            return Ok(None);
        };

        let header = BlockHeader {
            number: block.header.number,
            hash: format!("{:?}", block.header.hash),
            parent_hash: format!("{:?}", block.header.parent_hash),
            timestamp: block.header.timestamp,
            gas_used: block.header.gas_used as u64,
            gas_limit: block.header.gas_limit as u64,
            base_fee_per_gas: block.header.base_fee_per_gas.map(|v| v as u128),
        };

        // Fetch all receipts for the block in one call when available,
        // falling back to per-tx receipt fetches.
        let receipts = self
            .provider
            .get_block_receipts(BlockId::Number(BlockNumberOrTag::Number(block_number)))
            .await
            .map_err(|e| {
                Error::Indexer(format!(
                    "failed to fetch receipts for block {block_number}: {e}"
                ))
            })?;

        let transactions: Vec<Transaction> = block
            .transactions
            .txns()
            .enumerate()
            .map(|(i, tx)| {
                let from_addr = format!("{:?}", tx.inner.signer());

                // Use per-tx gas_used from the receipt when available;
                // fall back to block-level total otherwise.
                let gas_used = receipts
                    .as_ref()
                    .and_then(|r| r.get(i))
                    .map(|receipt| receipt.gas_used as u64)
                    .unwrap_or(block.header.gas_used as u64);

                Transaction {
                    hash: format!("{:?}", tx.inner.tx_hash()),
                    block_number,
                    from: from_addr,
                    to: TxTrait::to(tx.inner.inner()).map(|a| format!("{a:?}")),
                    value: TxTrait::value(tx.inner.inner())
                        .try_into()
                        .unwrap_or(u128::MAX),
                    gas_price: tx.effective_gas_price.map(|g| g as u128),
                    max_fee_per_gas: Some(
                        TxTrait::max_fee_per_gas(tx.inner.inner()) as u128,
                    ),
                    max_priority_fee_per_gas: TxTrait::max_priority_fee_per_gas(
                        tx.inner.inner(),
                    )
                    .map(|g| g as u128),
                    gas_used,
                    input: TxTrait::input(tx.inner.inner()).to_vec(),
                    nonce: TxTrait::nonce(tx.inner.inner()),
                    tx_index: i as u64,
                    timestamp: block.header.timestamp,
                }
            })
            .collect();

        Ok(Some((header, transactions)))
    }
}
