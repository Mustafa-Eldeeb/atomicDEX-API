use crate::context::CoinsActivationContext;
use crate::prelude::TryFromCoinProtocol;
use crate::standalone_coin::InitStandaloneCoinActivationOps;
use crate::utxo_activation::init_utxo_standard_activation_error::InitUtxoStandardError;
use crate::utxo_activation::init_utxo_standard_statuses::{UtxoStandardAwaitingStatus, UtxoStandardInProgressStatus,
                                                          UtxoStandardUserAction};
use crate::utxo_activation::utxo_standard_activation_result::UtxoStandardActivationResult;
use crate::utxo_activation::utxo_standard_coin_hw_ops::UtxoStandardCoinHwOps;
use async_trait::async_trait;
use coins::utxo::utxo_builder::{UtxoArcBuilder, UtxoCoinBuilder};
use coins::utxo::utxo_standard::UtxoStandardCoin;
use coins::utxo::UtxoActivationParams;
use coins::{CoinProtocol, MarketCoinOps, PrivKeyBuildPolicy};
use common::mm_ctx::MmArc;
use common::mm_error::prelude::*;
use common::Future01CompatExt;
use crypto::trezor::trezor_rpc_task::RpcTaskHandle;
use rpc_task::RpcTaskManagerShared;
use serde_json::Value as Json;
use std::collections::HashMap;

pub type UtxoStandardTaskManagerShared = RpcTaskManagerShared<
    UtxoStandardActivationResult,
    InitUtxoStandardError,
    UtxoStandardInProgressStatus,
    UtxoStandardAwaitingStatus,
    UtxoStandardUserAction,
>;

pub type UtxoStandardRpcTaskHandle = RpcTaskHandle<
    UtxoStandardActivationResult,
    InitUtxoStandardError,
    UtxoStandardInProgressStatus,
    UtxoStandardAwaitingStatus,
    UtxoStandardUserAction,
>;

pub struct UtxoStandardProtocolInfo;

impl TryFromCoinProtocol for UtxoStandardProtocolInfo {
    fn try_from_coin_protocol(proto: CoinProtocol) -> Result<Self, MmError<CoinProtocol>>
    where
        Self: Sized,
    {
        match proto {
            CoinProtocol::UTXO => Ok(UtxoStandardProtocolInfo),
            protocol => MmError::err(protocol),
        }
    }
}

#[async_trait]
impl InitStandaloneCoinActivationOps for UtxoStandardCoin {
    type ActivationRequest = UtxoActivationParams;
    type StandaloneProtocol = UtxoStandardProtocolInfo;
    type ActivationResult = UtxoStandardActivationResult;
    type ActivationError = InitUtxoStandardError;
    type InProgressStatus = UtxoStandardInProgressStatus;
    type AwaitingStatus = UtxoStandardAwaitingStatus;
    type UserAction = UtxoStandardUserAction;

    fn rpc_task_manager(activation_ctx: &CoinsActivationContext) -> &UtxoStandardTaskManagerShared {
        &activation_ctx.init_utxo_standard_task_manager
    }

    async fn init_standalone_coin(
        ctx: MmArc,
        ticker: String,
        coin_conf: Json,
        activation_request: Self::ActivationRequest,
        _protocol_info: Self::StandaloneProtocol,
        priv_key_policy: PrivKeyBuildPolicy<'_>,
        task_handle: &UtxoStandardRpcTaskHandle,
    ) -> MmResult<Self, InitUtxoStandardError> {
        let hw_ops = UtxoStandardCoinHwOps::new(&ctx, task_handle);
        UtxoArcBuilder::new(
            &ctx,
            &ticker,
            &coin_conf,
            activation_request,
            priv_key_policy,
            hw_ops,
            UtxoStandardCoin::from,
        )
        .build()
        .await
        .mm_err(|e| InitUtxoStandardError::from_build_err(e, ticker))
    }

    // TODO finish implementing this method.
    async fn get_activation_result(
        &self,
        _task_handle: &UtxoStandardRpcTaskHandle,
    ) -> MmResult<Self::ActivationResult, InitUtxoStandardError> {
        let current_block =
            self.current_block()
                .compat()
                .await
                .map_to_mm(|error| InitUtxoStandardError::CoinCreationError {
                    ticker: self.ticker().to_owned(),
                    error,
                })?;
        let result = UtxoStandardActivationResult {
            current_block,
            addresses_infos: HashMap::new(),
        };

        Ok(result)
    }
}
