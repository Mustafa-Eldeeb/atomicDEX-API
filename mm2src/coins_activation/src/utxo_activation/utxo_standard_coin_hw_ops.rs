use crate::utxo_activation::init_utxo_standard_activation::UtxoStandardRpcTaskHandle;
use crate::utxo_activation::init_utxo_standard_statuses::{UtxoStandardAwaitingStatus, UtxoStandardInProgressStatus};
use async_trait::async_trait;
use coins::utxo::utxo_builder::{UtxoCoinBuildError, UtxoCoinBuildHwOps, UtxoCoinBuildResult, UtxoConfError};
use coins::utxo::UtxoCoinConf;
use common::mm_ctx::MmArc;
use common::mm_error::prelude::*;
use crypto::hw_rpc_task::{TrezorConnectStatuses, TrezorRpcTaskConnectProcessor};
use crypto::trezor::trezor_rpc_task::{TrezorRequestStatuses, TrezorRpcTaskProcessor};
use crypto::trezor::ProcessTrezorResponse;
use crypto::{CryptoCtx, DerivationPath, EcdsaCurve, ExtendedPublicKey, HwError, Secp256k1ExtendedPublicKey};
use std::str::FromStr;

pub struct UtxoStandardCoinHwOps<'a> {
    ctx: &'a MmArc,
    task_handle: &'a UtxoStandardRpcTaskHandle,
}

#[async_trait]
impl<'a> UtxoCoinBuildHwOps for UtxoStandardCoinHwOps<'a> {
    async fn extended_public_key(
        &self,
        conf: &UtxoCoinConf,
        derivation_path: DerivationPath,
    ) -> UtxoCoinBuildResult<Secp256k1ExtendedPublicKey> {
        let trezor_coin = conf
            .trezor_coin
            .or_mm_err(|| UtxoCoinBuildError::ConfError(UtxoConfError::TrezorCoinIsNotSet))?;

        let crypto_ctx = CryptoCtx::from_ctx(self.ctx)?;
        let hw_ctx = crypto_ctx.hw_ctx().or_mm_err(|| {
            let error = "'UtxoCoinBuildHwOps::extended_public_key' is expected to be used if 'HardwareWalletCtx' is initialized only".to_owned();
            UtxoCoinBuildError::Internal(error)
        })?;

        let connect_processor = TrezorRpcTaskConnectProcessor::new(self.task_handle, TrezorConnectStatuses {
            on_connect: UtxoStandardInProgressStatus::WaitingForTrezorToConnect,
            on_connected: UtxoStandardInProgressStatus::ActivatingCoin,
            on_connection_failed: UtxoStandardInProgressStatus::Finishing,
            on_button_request: UtxoStandardInProgressStatus::WaitingForUserToConfirmPubkey,
            on_pin_request: UtxoStandardAwaitingStatus::WaitForTrezorPin,
            on_ready: UtxoStandardInProgressStatus::ActivatingCoin,
        });

        let pubkey_processor = TrezorRpcTaskProcessor::new(self.task_handle, TrezorRequestStatuses {
            on_button_request: UtxoStandardInProgressStatus::WaitingForUserToConfirmPubkey,
            on_pin_request: UtxoStandardAwaitingStatus::WaitForTrezorPin,
            on_ready: UtxoStandardInProgressStatus::ActivatingCoin,
        });

        let trezor = hw_ctx
            .trezor(&connect_processor)
            .await
            .mm_err(|e| UtxoCoinBuildError::ErrorProcessingHwRequest(e.to_string()))?;
        let mut trezor_session = trezor.session().await?;

        let xpub = trezor_session
            .get_public_key(derivation_path, trezor_coin, EcdsaCurve::Secp256k1)
            .await?
            .process(&pubkey_processor)
            .await
            .mm_err(|e| UtxoCoinBuildError::ErrorProcessingHwRequest(e.to_string()))?;
        ExtendedPublicKey::from_str(&xpub)
            .map_to_mm(|e| UtxoCoinBuildError::HardwareWalletError(HwError::InvalidXpub(e)))
    }
}

impl<'a> UtxoStandardCoinHwOps<'a> {
    pub fn new(ctx: &'a MmArc, task_handle: &'a UtxoStandardRpcTaskHandle) -> UtxoStandardCoinHwOps<'a> {
        UtxoStandardCoinHwOps { ctx, task_handle }
    }
}
