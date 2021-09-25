use super::{CoinBalance, HistorySyncState, MarketCoinOps, MmCoin, SwapOps, TradeFee, TransactionEnum, TransactionFut};
use crate::{BalanceError, BalanceFut, FeeApproxStage, FoundSwapTxSpend, NegotiateSwapContractAddrErr,
            TradePreimageFut, TradePreimageValue, ValidateAddressResult, WithdrawFut, WithdrawRequest};
use base58::ToBase58;
use bigdecimal::BigDecimal;
use common::{mm_ctx::MmArc, mm_ctx::MmWeak, mm_error::MmError, mm_number::MmNumber};
use futures::{FutureExt, TryFutureExt};
use futures01::Future;
use mocktopus::macros::*;
use rpc::v1::types::Bytes as BytesJson;
use serde_json::Value as Json;
use solana_client::{client_error::{ClientError, ClientErrorKind},
                    rpc_client::RpcClient};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::ops::Deref;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

#[cfg(test)] mod solana_tests;

impl From<ClientError> for BalanceError {
    fn from(e: ClientError) -> Self {
        match e.kind {
            ClientErrorKind::Io(e) => BalanceError::Transport(e.to_string()),
            ClientErrorKind::Reqwest(e) => BalanceError::Transport(e.to_string()),
            ClientErrorKind::RpcError(e) => BalanceError::Transport(format!("{:?}", e)),
            ClientErrorKind::SerdeJson(e) => BalanceError::InvalidResponse(e.to_string()),
            ClientErrorKind::Custom(e) => BalanceError::Internal(e),
            ClientErrorKind::SigningError(_)
            | ClientErrorKind::TransactionError(_)
            | ClientErrorKind::FaucetError(_) => BalanceError::Internal("not_reacheable".to_string()),
        }
    }
}

pub struct SplTokenInfos {
    ticker: String,
    decimals: u8,
    token_address: String,
}

lazy_static! {
    static ref SPL_PROGRAM_ID: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".parse().unwrap();
}

/// pImpl idiom.
pub struct SolanaCoinImpl {
    ticker: String,
    coin_type: SolanaCoinType,
    key_pair: Keypair,
    client: RpcClient,
    decimals: u8,
    required_confirmations: AtomicU64,
    ctx: MmWeak,
    my_address: String,
}

impl Debug for SolanaCoinImpl {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult { f.write_str(format!("{}", self.ticker).as_str()) }
}

#[derive(Clone, Debug)]
pub struct SolanaCoin(Arc<SolanaCoinImpl>);
impl Deref for SolanaCoin {
    type Target = SolanaCoinImpl;
    fn deref(&self) -> &SolanaCoinImpl { &*self.0 }
}

impl SolanaCoin {
    fn my_balance_impl(&self) -> BalanceFut<u64> {
        let coin = self.clone();
        let fut = async move {
            match coin.coin_type {
                SolanaCoinType::Solana => Ok(coin.client.get_balance(&coin.key_pair.pubkey())?),
                SolanaCoinType::Spl { .. } => MmError::err(BalanceError::Internal("not implemented_yet".to_string())),
            }
        };
        Box::new(fut.boxed().compat())
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SolanaCoinType {
    /// Solana itself or it's forks
    Solana,
    /// SPL token with smart contract address
    /// https://spl.solana.com/
    Spl {
        platform: String,
        token_addr: solana_sdk::pubkey::Pubkey,
    },
}

#[mockable]
#[allow(clippy::forget_ref, clippy::forget_copy, clippy::cast_ref_to_mut)]
impl MarketCoinOps for SolanaCoin {
    fn ticker(&self) -> &str { &self.ticker }

    fn my_address(&self) -> Result<String, String> { Ok(self.my_address.clone()) }

    fn my_balance(&self) -> BalanceFut<CoinBalance> {
        let fut = self.my_balance_impl().and_then(move |result| {
            Ok(solana_sdk::native_token::lamports_to_sol(result)).map(|spendable| CoinBalance {
                spendable: BigDecimal::from(spendable),
                unspendable: BigDecimal::from(0),
            })
        });
        Box::new(fut)
    }

    fn base_coin_balance(&self) -> BalanceFut<BigDecimal> {
        let fut = self.my_balance_impl().and_then(move |result| {
            Ok(solana_sdk::native_token::lamports_to_sol(result)).map(|spendable| BigDecimal::from(spendable))
        });
        Box::new(fut)
    }

    fn send_raw_tx(&self, _tx: &str) -> Box<dyn Future<Item = String, Error = String> + Send> { unimplemented!() }

    fn wait_for_confirmations(
        &self,
        _tx: &[u8],
        _confirmations: u64,
        _requires_nota: bool,
        _wait_until: u64,
        _check_every: u64,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        unimplemented!()
    }

    fn wait_for_tx_spend(
        &self,
        _transaction: &[u8],
        _wait_until: u64,
        _from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> TransactionFut {
        unimplemented!()
    }

    fn tx_enum_from_bytes(&self, _bytes: &[u8]) -> Result<TransactionEnum, String> { unimplemented!() }

    fn current_block(&self) -> Box<dyn Future<Item = u64, Error = String> + Send> { unimplemented!() }

    fn display_priv_key(&self) -> String { self.key_pair.secret().to_bytes()[..].to_base58() }

    fn min_tx_amount(&self) -> BigDecimal { BigDecimal::from(0) }

    fn min_trading_vol(&self) -> MmNumber {
        // todo use a proper way for tokens, maybe an optional<String> ticker
        let pow = self.decimals / 3;
        MmNumber::from(1) / MmNumber::from(10u64.pow(pow as u32))
    }
}

#[mockable]
#[allow(clippy::forget_ref, clippy::forget_copy, clippy::cast_ref_to_mut)]
impl SwapOps for SolanaCoin {
    fn send_taker_fee(&self, _fee_addr: &[u8], amount: BigDecimal) -> TransactionFut { unimplemented!() }

    fn send_maker_payment(
        &self,
        _time_lock: u32,
        _taker_pub: &[u8],
        _secret_hash: &[u8],
        _amount: BigDecimal,
        _swap_contract_address: &Option<BytesJson>,
    ) -> TransactionFut {
        unimplemented!()
    }

    fn send_taker_payment(
        &self,
        _time_lock: u32,
        _maker_pub: &[u8],
        _secret_hash: &[u8],
        _amount: BigDecimal,
        _swap_contract_address: &Option<BytesJson>,
    ) -> TransactionFut {
        unimplemented!()
    }

    fn send_maker_spends_taker_payment(
        &self,
        _taker_payment_tx: &[u8],
        _time_lock: u32,
        _taker_pub: &[u8],
        _secret: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> TransactionFut {
        unimplemented!()
    }

    fn send_taker_spends_maker_payment(
        &self,
        _maker_payment_tx: &[u8],
        _time_lock: u32,
        _maker_pub: &[u8],
        _secret: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> TransactionFut {
        unimplemented!()
    }

    fn send_taker_refunds_payment(
        &self,
        _taker_payment_tx: &[u8],
        _time_lock: u32,
        _maker_pub: &[u8],
        _secret_hash: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> TransactionFut {
        unimplemented!()
    }

    fn send_maker_refunds_payment(
        &self,
        _maker_payment_tx: &[u8],
        _time_lock: u32,
        _taker_pub: &[u8],
        _secret_hash: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> TransactionFut {
        unimplemented!()
    }

    fn validate_fee(
        &self,
        _fee_tx: &TransactionEnum,
        _expected_sender: &[u8],
        _fee_addr: &[u8],
        _amount: &BigDecimal,
        _min_block_number: u64,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        unimplemented!()
    }

    fn validate_maker_payment(
        &self,
        _payment_tx: &[u8],
        _time_lock: u32,
        _maker_pub: &[u8],
        _priv_bn_hash: &[u8],
        _amount: BigDecimal,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        unimplemented!()
    }

    fn validate_taker_payment(
        &self,
        _payment_tx: &[u8],
        _time_lock: u32,
        _taker_pub: &[u8],
        _priv_bn_hash: &[u8],
        _amount: BigDecimal,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        unimplemented!()
    }

    fn check_if_my_payment_sent(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Box<dyn Future<Item = Option<TransactionEnum>, Error = String> + Send> {
        unimplemented!()
    }

    fn search_for_swap_tx_spend_my(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _tx: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Result<Option<FoundSwapTxSpend>, String> {
        unimplemented!()
    }

    fn search_for_swap_tx_spend_other(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _tx: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Result<Option<FoundSwapTxSpend>, String> {
        unimplemented!()
    }

    fn extract_secret(&self, secret_hash: &[u8], spend_tx: &[u8]) -> Result<Vec<u8>, String> { unimplemented!() }

    fn negotiate_swap_contract_addr(
        &self,
        _other_side_address: Option<&[u8]>,
    ) -> Result<Option<BytesJson>, MmError<NegotiateSwapContractAddrErr>> {
        unimplemented!()
    }
}

#[mockable]
#[allow(clippy::forget_ref, clippy::forget_copy, clippy::cast_ref_to_mut)]
impl MmCoin for SolanaCoin {
    fn is_asset_chain(&self) -> bool { unimplemented!() }

    fn withdraw(&self, _req: WithdrawRequest) -> WithdrawFut { unimplemented!() }

    fn decimals(&self) -> u8 { self.decimals }

    fn convert_to_address(&self, _from: &str, _to_address_format: Json) -> Result<String, String> { unimplemented!() }

    fn validate_address(&self, _address: &str) -> ValidateAddressResult { unimplemented!() }

    fn process_history_loop(&self, _ctx: MmArc) -> Box<dyn Future<Item = (), Error = ()> + Send> { unimplemented!() }

    fn history_sync_status(&self) -> HistorySyncState { unimplemented!() }

    /// Get fee to be paid per 1 swap transaction
    fn get_trade_fee(&self) -> Box<dyn Future<Item = TradeFee, Error = String> + Send> { unimplemented!() }

    fn get_sender_trade_fee(&self, _value: TradePreimageValue, _stage: FeeApproxStage) -> TradePreimageFut<TradeFee> {
        unimplemented!()
    }

    fn get_receiver_trade_fee(&self, _stage: FeeApproxStage) -> TradePreimageFut<TradeFee> { unimplemented!() }

    fn get_fee_to_send_taker_fee(
        &self,
        _dex_fee_amount: BigDecimal,
        _stage: FeeApproxStage,
    ) -> TradePreimageFut<TradeFee> {
        unimplemented!()
    }

    fn required_confirmations(&self) -> u64 { 1 }

    fn requires_notarization(&self) -> bool { false }

    fn set_required_confirmations(&self, _confirmations: u64) { unimplemented!() }

    fn set_requires_notarization(&self, _requires_nota: bool) { unimplemented!() }

    fn swap_contract_address(&self) -> Option<BytesJson> { unimplemented!() }

    fn mature_confirmations(&self) -> Option<u32> { unimplemented!() }

    fn coin_protocol_info(&self) -> Vec<u8> { Vec::new() }

    fn is_coin_protocol_supported(&self, _info: &Option<Vec<u8>>) -> bool { true }
}
