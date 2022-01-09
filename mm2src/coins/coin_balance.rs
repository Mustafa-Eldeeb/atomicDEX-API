use crate::{lp_coinfind_or_err, BalanceError, BalanceResult, CoinBalance, CoinFindError, CoinWithDerivationMethod,
            DerivationMethod, Future01CompatExt, MarketCoinOps, MmCoinEnum};
use async_trait::async_trait;
use common::mm_ctx::MmArc;
use common::mm_error::prelude::*;
use common::HttpStatusCode;
use crypto::RpcDerivationPath;
use derive_more::Display;
use http::StatusCode;
use std::fmt;

#[derive(Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum WalletBalanceRpcError {
    #[display(fmt = "No such coin {}", coin)]
    NoSuchCoin { coin: String },
    #[display(fmt = "Transport: {}", _0)]
    Transport(String),
    #[display(fmt = "Internal: {}", _0)]
    Internal(String),
}

impl HttpStatusCode for WalletBalanceRpcError {
    fn status_code(&self) -> StatusCode {
        match self {
            WalletBalanceRpcError::NoSuchCoin { .. } => StatusCode::BAD_REQUEST,
            WalletBalanceRpcError::Transport(_) | WalletBalanceRpcError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            },
        }
    }
}

impl From<CoinFindError> for WalletBalanceRpcError {
    fn from(e: CoinFindError) -> Self {
        match e {
            CoinFindError::NoSuchCoin { coin } => WalletBalanceRpcError::NoSuchCoin { coin },
        }
    }
}

impl From<BalanceError> for WalletBalanceRpcError {
    fn from(e: BalanceError) -> Self {
        match e {
            BalanceError::Transport(transport) | BalanceError::InvalidResponse(transport) => {
                WalletBalanceRpcError::Transport(transport)
            },
            // `wallet_balance` should work with both [`DerivationMethod::Iguana`] and [`DerivationMethod::HDWallet`] correctly.
            BalanceError::DerivationMethodNotSupported(error) => WalletBalanceRpcError::Internal(error.to_string()),
            BalanceError::Internal(internal) => WalletBalanceRpcError::Internal(internal),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "wallet_type")]
pub enum WalletBalance {
    Iguana(IguanaWalletBalance),
    HD(HDWalletBalance),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IguanaWalletBalance {
    pub address: String,
    pub balance: CoinBalance,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDWalletBalance {
    pub accounts: Vec<HDAccountBalance>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDAccountBalance {
    pub account_index: u32,
    pub addresses: Vec<HDAddressBalance>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDAddressBalance {
    pub address: String,
    pub derivation_path: RpcDerivationPath,
    pub balance: CoinBalance,
}

#[async_trait]
pub trait WalletBalanceOps {
    async fn wallet_balance(&self) -> BalanceResult<WalletBalance>;
}

#[async_trait]
impl WalletBalanceOps for MmCoinEnum {
    async fn wallet_balance(&self) -> BalanceResult<WalletBalance> {
        let balance = match self {
            MmCoinEnum::UtxoCoin(utxo) => utxo.wallet_balance().await?,
            MmCoinEnum::QtumCoin(qtum) => qtum.wallet_balance().await?,
            MmCoinEnum::Qrc20Coin(qrc20) => qrc20.wallet_balance().await?,
            MmCoinEnum::EthCoin(eth) => eth.wallet_balance().await?,
            #[cfg(all(not(target_arch = "wasm32"), feature = "zhtlc"))]
            MmCoinEnum::ZCoin(zcoin) => zcoin.wallet_balance().await?,
            MmCoinEnum::Bch(bch) => bch.wallet_balance().await?,
            MmCoinEnum::SlpToken(slp) => slp.wallet_balance().await?,
            MmCoinEnum::LightningCoin(lightning) => lightning.wallet_balance().await?,
            MmCoinEnum::Test(test) => test.wallet_balance().await?,
        };
        Ok(balance)
    }
}

#[async_trait]
impl<Coin, Address, HDWallet> WalletBalanceOps for Coin
where
    Coin: AddressBalanceOps<Address = Address>
        + CoinWithDerivationMethod<Address = Address, HDWallet = HDWallet>
        + HDWalletBalanceOps<HDWallet = HDWallet>
        + Sync,
    Address: fmt::Display + Sync,
    HDWallet: Sync,
{
    async fn wallet_balance(&self) -> BalanceResult<WalletBalance> {
        match self.derivation_method() {
            DerivationMethod::Iguana(address) => self.address_balance(address).await.map(|balance| {
                WalletBalance::Iguana(IguanaWalletBalance {
                    address: address.to_string(),
                    balance,
                })
            }),
            DerivationMethod::HDWallet(hd_wallet) => self
                .hd_wallet_balance(hd_wallet)
                .await
                .map(|accounts| WalletBalance::HD(HDWalletBalance { accounts })),
        }
    }
}

#[async_trait]
pub trait HDWalletBalanceOps: AddressBalanceOps {
    type HDWallet;
    type HDAccount;

    async fn hd_wallet_balance(&self, hd_wallet: &Self::HDWallet) -> BalanceResult<Vec<HDAccountBalance>>;

    async fn hd_account_balance(
        &self,
        hd_wallet: &Self::HDWallet,
        hd_account: &mut Self::HDAccount,
    ) -> BalanceResult<HDAccountBalance>;

    /// Request a balance of the given `address`.
    /// This function is expected to be more efficient than ['HDWalletBalanceOps::check_address_balance'] in most cases
    /// since many of RPC clients allow us to request the address balance without the history.
    async fn known_address_balance(&self, address: &Self::Address) -> BalanceResult<CoinBalance> {
        self.address_balance(address).await
    }

    /// Check if the address has been used by the user by checking if the transaction history of the given `address` is not empty.
    /// Please note the function can return zero balance even if the address has been used before.
    async fn check_address_balance(&self, address: &Self::Address) -> BalanceResult<AddressBalanceStatus<CoinBalance>>;
}

pub enum AddressBalanceStatus<Balance> {
    Empty,
    NonEmpty(Balance),
}

#[async_trait]
pub trait AddressBalanceOps {
    type Address: Sync;

    async fn address_balance(&self, address: &Self::Address) -> BalanceResult<CoinBalance>;
}

#[derive(Deserialize)]
pub struct WalletBalanceRequest {
    coin: String,
}

pub async fn wallet_balance(ctx: MmArc, req: WalletBalanceRequest) -> MmResult<WalletBalance, WalletBalanceRpcError> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    Ok(coin.wallet_balance().await?)
}

pub(crate) async fn iguana_wallet_balance<Coin: MarketCoinOps>(coin: &Coin) -> BalanceResult<WalletBalance> {
    let address = coin.my_address().map_to_mm(BalanceError::Internal)?;
    let balance = coin.my_balance().compat().await?;
    Ok(WalletBalance::Iguana(IguanaWalletBalance { address, balance }))
}
