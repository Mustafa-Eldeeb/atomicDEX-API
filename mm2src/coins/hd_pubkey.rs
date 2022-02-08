use crate::hd_wallet::{HDWalletRpcError, NewAccountCreatingError};
use async_trait::async_trait;
use common::mm_ctx::MmArc;
use common::mm_error::prelude::*;
use crypto::hw_rpc_task::{HwConnectStatuses, TrezorRpcTaskConnectProcessor};
use crypto::trezor::trezor_rpc_task::TrezorRpcTaskProcessor;
use crypto::trezor::utxo::TrezorUtxoCoin;
use crypto::trezor::{ProcessTrezorResponse, TrezorError, TrezorPinMatrix3x3Response, TrezorProcessingError};
use crypto::{Bip32Error, CryptoCtx, CryptoInitError, DerivationPath, EcdsaCurve, HardwareWalletArc, HwError,
             HwProcessingError, XPub};
use rpc_task::{RpcTaskError, RpcTaskHandle};
use serde::Serialize;
use std::convert::TryInto;

pub enum HDExtractPubkeyError {
    IguanaPrivKeyNotAllowed,
    CoinDoesntSupportTrezor,
    RpcTaskError(RpcTaskError),
    HardwareWalletError(HwError),
    InvalidXpub(Bip32Error),
    Internal(String),
}

impl From<CryptoInitError> for HDExtractPubkeyError {
    fn from(e: CryptoInitError) -> Self { HDExtractPubkeyError::Internal(e.to_string()) }
}

impl From<TrezorError> for HDExtractPubkeyError {
    fn from(e: TrezorError) -> Self { HDExtractPubkeyError::HardwareWalletError(HwError::from(e)) }
}

impl From<HwError> for HDExtractPubkeyError {
    fn from(e: HwError) -> Self { HDExtractPubkeyError::HardwareWalletError(e) }
}

impl From<TrezorProcessingError<RpcTaskError>> for HDExtractPubkeyError {
    fn from(e: TrezorProcessingError<RpcTaskError>) -> Self {
        match e {
            TrezorProcessingError::TrezorError(trezor) => HDExtractPubkeyError::from(HwError::from(trezor)),
            TrezorProcessingError::ProcessorError(rpc) => HDExtractPubkeyError::RpcTaskError(rpc),
        }
    }
}

impl From<HwProcessingError<RpcTaskError>> for HDExtractPubkeyError {
    fn from(e: HwProcessingError<RpcTaskError>) -> Self {
        match e {
            HwProcessingError::HwError(hw) => HDExtractPubkeyError::from(hw),
            HwProcessingError::ProcessorError(rpc) => HDExtractPubkeyError::RpcTaskError(rpc),
        }
    }
}

impl From<HDExtractPubkeyError> for NewAccountCreatingError {
    fn from(e: HDExtractPubkeyError) -> Self {
        match e {
            HDExtractPubkeyError::IguanaPrivKeyNotAllowed => NewAccountCreatingError::IguanaPrivKeyNotAllowed,
            HDExtractPubkeyError::CoinDoesntSupportTrezor => NewAccountCreatingError::CoinDoesntSupportTrezor,
            HDExtractPubkeyError::RpcTaskError(rpc) => NewAccountCreatingError::RpcTaskError(rpc),
            HDExtractPubkeyError::HardwareWalletError(hw) => NewAccountCreatingError::HardwareWalletError(hw),
            HDExtractPubkeyError::InvalidXpub(xpub) => {
                NewAccountCreatingError::HardwareWalletError(HwError::InvalidXpub(xpub))
            },
            HDExtractPubkeyError::Internal(internal) => NewAccountCreatingError::Internal(internal),
        }
    }
}

impl From<HDExtractPubkeyError> for HDWalletRpcError {
    fn from(e: HDExtractPubkeyError) -> Self {
        match e {
            HDExtractPubkeyError::IguanaPrivKeyNotAllowed => HDWalletRpcError::IguanaPrivKeyNotAllowed,
            HDExtractPubkeyError::CoinDoesntSupportTrezor => HDWalletRpcError::CoinDoesntSupportTrezor,
            HDExtractPubkeyError::RpcTaskError(rpc) => HDWalletRpcError::from(rpc),
            HDExtractPubkeyError::HardwareWalletError(hw) => HDWalletRpcError::from(hw),
            HDExtractPubkeyError::InvalidXpub(xpub) => HDWalletRpcError::from(HwError::InvalidXpub(xpub)),
            HDExtractPubkeyError::Internal(internal) => HDWalletRpcError::Internal(internal),
        }
    }
}

#[async_trait]
pub trait ExtractExtendedPubkey {
    type ExtendedPublicKey;

    async fn extract_extended_pubkey<XPubExtractor>(
        &self,
        xpub_extractor: &XPubExtractor,
        derivation_path: DerivationPath,
    ) -> MmResult<Self::ExtendedPublicKey, HDExtractPubkeyError>
    where
        XPubExtractor: HDXPubExtractor + Sync;
}

#[async_trait]
pub trait HDXPubExtractor {
    async fn extract_utxo_xpub(
        &self,
        trezor_utxo_coin: TrezorUtxoCoin,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError>;
}

pub enum RpcTaskXPubExtractor<'task, Item, Error, InProgressStatus, AwaitingStatus, UserAction>
where
    Item: Serialize,
    Error: SerMmErrorType,
{
    Trezor {
        hw_ctx: HardwareWalletArc,
        task_handle: &'task RpcTaskHandle<Item, Error, InProgressStatus, AwaitingStatus, UserAction>,
        statuses: HwConnectStatuses<InProgressStatus, AwaitingStatus>,
    },
}

#[async_trait]
impl<'task, Item, Error, InProgressStatus, AwaitingStatus, UserAction> HDXPubExtractor
    for RpcTaskXPubExtractor<'task, Item, Error, InProgressStatus, AwaitingStatus, UserAction>
where
    Item: Serialize + Send,
    Error: SerMmErrorType + Send,
    InProgressStatus: Clone + Send + Sync,
    AwaitingStatus: Clone + Send + Sync,
    UserAction: TryInto<TrezorPinMatrix3x3Response, Error = RpcTaskError> + Send,
{
    async fn extract_utxo_xpub(
        &self,
        trezor_utxo_coin: TrezorUtxoCoin,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError> {
        match self {
            RpcTaskXPubExtractor::Trezor {
                hw_ctx,
                task_handle,
                statuses,
            } => {
                Self::extract_utxo_xpub_from_trezor(hw_ctx, task_handle, statuses, trezor_utxo_coin, derivation_path)
                    .await
            },
        }
    }
}

impl<'task, Item, Error, InProgressStatus, AwaitingStatus, UserAction>
    RpcTaskXPubExtractor<'task, Item, Error, InProgressStatus, AwaitingStatus, UserAction>
where
    Item: Serialize,
    Error: SerMmErrorType,
{
    pub fn new(
        ctx: &MmArc,
        task_handle: &'task RpcTaskHandle<Item, Error, InProgressStatus, AwaitingStatus, UserAction>,
        statuses: HwConnectStatuses<InProgressStatus, AwaitingStatus>,
    ) -> MmResult<Self, HDExtractPubkeyError> {
        let crypto_ctx = CryptoCtx::from_ctx(ctx)?;
        // Don't use [`CryptoCtx::hw_ctx`] because we are planning to support HD master key.
        match *crypto_ctx {
            CryptoCtx::HardwareWallet(ref hw_ctx) => Ok(RpcTaskXPubExtractor::Trezor {
                hw_ctx: hw_ctx.clone(),
                task_handle,
                statuses,
            }),
            CryptoCtx::KeyPair(_) => MmError::err(HDExtractPubkeyError::IguanaPrivKeyNotAllowed),
        }
    }

    /// Constructs an Xpub extractor without checking if the MarketMaker is initialized with a hardware wallet.
    pub fn new_unchecked(
        crypto_ctx: &CryptoCtx,
        task_handle: &'task RpcTaskHandle<Item, Error, InProgressStatus, AwaitingStatus, UserAction>,
        statuses: HwConnectStatuses<InProgressStatus, AwaitingStatus>,
    ) -> XPubExtractorUnchecked<Self> {
        // Don't use [`CryptoCtx::hw_ctx`] because we are planning to support HD master key.
        match *crypto_ctx {
            CryptoCtx::HardwareWallet(ref hw_ctx) => XPubExtractorUnchecked::Allowed(RpcTaskXPubExtractor::Trezor {
                hw_ctx: hw_ctx.clone(),
                task_handle,
                statuses,
            }),
            CryptoCtx::KeyPair(_) => XPubExtractorUnchecked::IguanaPrivKeyNotAllowed,
        }
    }
}

impl<'task, Item, Error, InProgressStatus, AwaitingStatus, UserAction>
    RpcTaskXPubExtractor<'task, Item, Error, InProgressStatus, AwaitingStatus, UserAction>
where
    Item: Serialize + Send,
    Error: SerMmErrorType + Send,
    InProgressStatus: Clone + Send + Sync,
    AwaitingStatus: Clone + Send + Sync,
    UserAction: TryInto<TrezorPinMatrix3x3Response, Error = RpcTaskError> + Send,
{
    async fn extract_utxo_xpub_from_trezor(
        hw_ctx: &HardwareWalletArc,
        task_handle: &RpcTaskHandle<Item, Error, InProgressStatus, AwaitingStatus, UserAction>,
        statuses: &HwConnectStatuses<InProgressStatus, AwaitingStatus>,
        trezor_coin: TrezorUtxoCoin,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError> {
        let connect_processor = TrezorRpcTaskConnectProcessor::new(task_handle, statuses.clone());
        let trezor = hw_ctx.trezor(&connect_processor).await?;
        let mut trezor_session = trezor.session().await?;

        let pubkey_processor = TrezorRpcTaskProcessor::new(task_handle, statuses.to_trezor_request_statuses());
        trezor_session
            .get_public_key(derivation_path, trezor_coin, EcdsaCurve::Secp256k1)
            .await?
            .process(&pubkey_processor)
            .await
            .mm_err(HDExtractPubkeyError::from)
    }
}

/// This is a wrapper over `XPubExtractor`. The main goal of this enum is to allow construct an Xpub extractor
/// even if HD wallet is not supported. But if someone tries to extract an Xpub despite HD wallet is not supported,
/// it fails with the [`HDExtractPubkeyError::IguanaPrivKeyNotAllowed`] error.
pub enum XPubExtractorUnchecked<XPubExtractor> {
    Allowed(XPubExtractor),
    IguanaPrivKeyNotAllowed,
}

#[async_trait]
impl<XPubExtractor> HDXPubExtractor for XPubExtractorUnchecked<XPubExtractor>
where
    XPubExtractor: HDXPubExtractor + Sync,
{
    async fn extract_utxo_xpub(
        &self,
        trezor_utxo_coin: TrezorUtxoCoin,
        derivation_path: DerivationPath,
    ) -> MmResult<XPub, HDExtractPubkeyError> {
        match self {
            XPubExtractorUnchecked::Allowed(xpub_extractor) => {
                xpub_extractor
                    .extract_utxo_xpub(trezor_utxo_coin, derivation_path)
                    .await
            },
            XPubExtractorUnchecked::IguanaPrivKeyNotAllowed => {
                MmError::err(HDExtractPubkeyError::IguanaPrivKeyNotAllowed)
            },
        }
    }
}