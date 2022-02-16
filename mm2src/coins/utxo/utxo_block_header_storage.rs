use async_trait::async_trait;
use common::mm_ctx::MmArc;
use common::{mm_error::NotMmError, NotSame};
use std::path::PathBuf;

pub trait BlockHeaderStorageError: std::fmt::Debug + NotMmError + NotSame + Send {}

#[async_trait]
pub trait BlockHeaderStorage: Send + Sync + 'static {
    type Error: BlockHeaderStorageError;
}

fn block_header_storage_dir(ctx: &MmArc, ticker: &str) -> PathBuf { ctx.dbdir().join("BLOCK_HEADERS").join(ticker) }

mod tests {
    use super::*;
    use common::mm_ctx::MmCtxBuilder;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_block_header_storage_dir() {
        let ctx = MmCtxBuilder::new().into_mm_arc();
        assert_eq!(block_header_storage_dir(&ctx, "BTC").as_os_str().is_empty(), false)
    }
}
