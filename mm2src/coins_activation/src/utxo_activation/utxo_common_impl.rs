use crate::utxo_activation::init_utxo_standard_activation::UtxoStandardRpcTaskHandle;
use crate::utxo_activation::init_utxo_standard_activation_error::InitUtxoStandardError;
use crate::utxo_activation::init_utxo_standard_statuses::UtxoStandardInProgressStatus;
use crate::utxo_activation::utxo_standard_activation_result::UtxoStandardActivationResult;
use coins::coin_balance::EnableCoinBalanceOps;
use coins::MarketCoinOps;
use common::mm_error::prelude::*;
use futures::compat::Future01CompatExt;

pub async fn get_activation_result<Coin>(
    coin: &Coin,
    task_handle: &UtxoStandardRpcTaskHandle,
) -> MmResult<UtxoStandardActivationResult, InitUtxoStandardError>
where
    Coin: EnableCoinBalanceOps + MarketCoinOps,
{
    let current_block =
        coin.current_block()
            .compat()
            .await
            .map_to_mm(|error| InitUtxoStandardError::CoinCreationError {
                ticker: coin.ticker().to_owned(),
                error,
            })?;

    task_handle.update_in_progress_status(UtxoStandardInProgressStatus::RequestingWalletBalance)?;
    let wallet_balance = coin
        .enable_coin_balance()
        .await
        .mm_err(|error| InitUtxoStandardError::CoinCreationError {
            ticker: coin.ticker().to_owned(),
            error: error.to_string(),
        })?;
    task_handle.update_in_progress_status(UtxoStandardInProgressStatus::ActivatingCoin)?;

    let result = UtxoStandardActivationResult {
        current_block,
        wallet_balance,
    };
    Ok(result)
}
