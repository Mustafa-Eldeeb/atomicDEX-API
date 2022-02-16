use crate::utxo::rpc_clients::ElectrumBlockHeader;
use async_trait::async_trait;
use chain::BlockHeader;
use common::{mm_ctx::MmArc, mm_error::MmError, mm_error::NotMmError, NotSame};
use std::path::PathBuf;

pub trait BlockHeaderStorageError: std::fmt::Debug + NotMmError + NotSame + Send {}

#[async_trait]
pub trait BlockHeaderStorage: Send + Sync + 'static {
    type Error: BlockHeaderStorageError;

    /// Initializes collection/tables in storage for a specified coin
    async fn init(&self, for_coin: &str) -> Result<(), MmError<Self::Error>>;

    async fn is_initialized_for(&self, for_coin: &str) -> Result<bool, MmError<Self::Error>>;

    // Adds multiple block headers to the selected coin's header storage
    // Should store it as `TICKER_HEIGHT=hex_string`
    async fn add_block_headers_to_storage(
        &self,
        for_coin: &str,
        headers: Vec<ElectrumBlockHeader>,
    ) -> Result<(), MmError<Self::Error>>;

    /// Gets the block header by height from the selected coin's storage as BlockHeader
    async fn get_block_header(&self, for_coin: &str, height: u64) -> Result<Option<BlockHeader>, MmError<Self::Error>>;

    /// Gets the block header by height from the selected coin's storage as hex
    async fn get_block_header_raw(&self, for_coin: &str, height: u64) -> Result<Option<String>, MmError<Self::Error>>;
}

fn block_header_storage_dir(ctx: &MmArc, ticker: &str) -> PathBuf { ctx.dbdir().join("BLOCK_HEADERS").join(ticker) }

mod tests {
    use crate::utxo::utxo_block_header_storage::block_header_storage_dir;
    use common::mm_ctx::MmCtxBuilder;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_block_header_storage_dir() {
        let ctx = MmCtxBuilder::new().into_mm_arc();
        assert_eq!(block_header_storage_dir(&ctx, "BTC").as_os_str().is_empty(), false)
    }
}
