use std::sync::Arc;

use futures::StreamExt;
use futures::TryStreamExt;
use stratus::config::ImporterOnlineConfig;
use stratus::eth::primitives::BlockNumber;
use stratus::eth::primitives::ExternalBlock;
use stratus::eth::primitives::ExternalReceipt;
use stratus::eth::primitives::ExternalReceipts;
use stratus::eth::primitives::Hash;
use stratus::infra::metrics;
use stratus::infra::BlockchainClient;
use stratus::init_global_services;
use stratus::log_and_err;

/// Number of transactions receipts that can be fetched in parallel.
const RECEIPTS_PARALELLISM: usize = 10;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // init services
    let config: ImporterOnlineConfig = init_global_services();
    let chain = BlockchainClient::new(&config.external_rpc).await?;
    let storage = Arc::new(config.init_storage().await?);
    let executor = config.init_executor(Arc::clone(&storage));

    // start from last imported block
    let mut number = storage.read_current_block_number().await?;

    // keep importing forever
    loop {
        let start = std::time::Instant::now();
        number = number.next();

        // fetch block and receipts
        let block = fetch_block(&chain, number).await?;

        // fetch receipts in parallel
        let mut receipts = Vec::with_capacity(block.transactions.len());
        for tx in &block.transactions {
            receipts.push(fetch_receipt(&chain, tx.hash()));
        }
        let receipts = futures::stream::iter(receipts).buffered(RECEIPTS_PARALELLISM).try_collect::<Vec<_>>().await?;

        // import block
        let mut receipts: ExternalReceipts = receipts.into();
        executor.import_external(block, &mut receipts).await?;
        metrics::inc_import_online(start.elapsed());
    }
}

async fn fetch_block(chain: &BlockchainClient, number: BlockNumber) -> anyhow::Result<ExternalBlock> {
    let block = loop {
        tracing::info!(%number, "fetching block");
        let block = match chain.get_block_by_number(number).await {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(reason = ?e, "retrying block download because error");
                continue;
            }
        };

        if block.is_null() {
            #[cfg(not(feature = "perf"))]
            {
                tracing::warn!(reason = %"null", "retrying block download because block is not mined yet");
                continue;
            }

            #[cfg(feature = "perf")]
            std::process::exit(0);
        }

        break block;
    };

    match serde_json::from_value(block.clone()) {
        Ok(block) => Ok(block),
        Err(e) => log_and_err!(reason = e, payload = block, "failed to deserialize external block"),
    }
}

async fn fetch_receipt(chain: &BlockchainClient, hash: Hash) -> anyhow::Result<ExternalReceipt> {
    let receipt = loop {
        tracing::info!(%hash, "fetching receipt");
        let receipt = match chain.get_transaction_receipt(&hash).await {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!(reason = ?e, "retrying receipt download because error");
                continue;
            }
        };

        if receipt.is_null() {
            tracing::warn!(reason = %"null", "retrying receipt download because block is not mined yet");
            continue;
        }

        break receipt;
    };

    match serde_json::from_value(receipt.clone()) {
        Ok(receipt) => Ok(receipt),
        Err(e) => log_and_err!(reason = e, payload = receipt, "failed to deserialize external receipt"),
    }
}