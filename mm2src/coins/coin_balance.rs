use crate::{BalanceResult, CoinWithDerivationMethod, DerivationMethod};
use async_trait::async_trait;
use crypto::RpcDerivationPath;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "wallet_type")]
pub enum WalletBalance<Balance> {
    Iguana(IguanaWalletBalance<Balance>),
    HD(HDWalletBalances<Balance>),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IguanaWalletBalance<Balance> {
    address: String,
    balance: Balance,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDWalletBalances<Balance> {
    pub accounts: Vec<HDAccountBalances<Balance>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDAccountBalances<Balance> {
    pub account_index: u32,
    pub addresses: Vec<HDAddressBalance<Balance>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HDAddressBalance<Balance> {
    pub address: String,
    pub derivation_path: RpcDerivationPath,
    pub balance: Balance,
}

#[async_trait]
pub trait WalletBalancesOps<Address, Balance, HDWallet>:
    AddressBalanceOps<Address = Address, Balance = Balance>
    + CoinWithDerivationMethod<Address = Address, HDWallet = HDWallet>
    + HDWalletBalanceOps<HDWallet = HDWallet>
where
    Address: fmt::Display + Sync,
    HDWallet: Sync,
{
    async fn wallet_balances(&self) -> BalanceResult<WalletBalance<Balance>> {
        match self.derivation_method() {
            DerivationMethod::Iguana(address) => self.address_balance(address).await.map(|balance| {
                WalletBalance::Iguana(IguanaWalletBalance {
                    address: address.to_string(),
                    balance,
                })
            }),
            DerivationMethod::HDWallet(hd_wallet) => self
                .hd_wallet_balances(hd_wallet)
                .await
                .map(|accounts| WalletBalance::HD(HDWalletBalances { accounts })),
        }
    }
}

#[async_trait]
pub trait HDWalletBalanceOps: AddressBalanceOps {
    type HDWallet;
    type HDAccount;

    fn gap_limit(&self, _hd_wallet: &Self::HDWallet) -> u32;

    async fn hd_wallet_balances(
        &self,
        hd_wallet: &Self::HDWallet,
    ) -> BalanceResult<Vec<HDAccountBalances<Self::Balance>>>;

    async fn hd_account_balances(
        &self,
        hd_wallet: &Self::HDWallet,
        hd_account: &mut Self::HDAccount,
    ) -> BalanceResult<HDAccountBalances<Self::Balance>>;

    /// Request a balance of the given `address`.
    /// This function is expected to be more efficient than ['HDWalletBalanceOps::check_address_balance'] in most cases
    /// since many of RPC clients allows to request a balance without the history.
    async fn known_address_balance(&self, address: &Self::Address) -> BalanceResult<Self::Balance> {
        self.address_balance(address).await
    }

    /// Check if the address has been used by the user by checking if the transaction history of the given `address` is not empty.
    /// Please note the function can return zero balance even if the address has been used before.
    async fn check_address_balance(
        &self,
        address: &Self::Address,
    ) -> BalanceResult<AddressBalanceStatus<Self::Balance>>;
}

pub enum AddressBalanceStatus<Balance> {
    Empty,
    NonEmpty(Balance),
}

#[async_trait]
pub trait AddressBalanceOps {
    type Address: Sync;
    type Balance;

    async fn address_balance(&self, address: &Self::Address) -> BalanceResult<Self::Balance>;
}
