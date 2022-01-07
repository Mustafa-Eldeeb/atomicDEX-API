#[cfg(not(target_arch = "wasm32"))]
use super::{lp_coinfind_or_err, MmCoinEnum};
#[cfg(not(target_arch = "wasm32"))]
use crate::utxo::rpc_clients::UtxoRpcClientEnum;
#[cfg(not(target_arch = "wasm32"))]
use crate::utxo::utxo_common::UtxoTxBuilder;
#[cfg(not(target_arch = "wasm32"))]
use crate::utxo::{sat_from_big_decimal, FeePolicy, UtxoCommonOps, UtxoTxGenerationOps};
use crate::{BalanceFut, CoinBalance, FeeApproxStage, FoundSwapTxSpend, HistorySyncState, MarketCoinOps, MmCoin,
            NegotiateSwapContractAddrErr, SwapOps, TradeFee, TradePreimageFut, TradePreimageValue, TransactionEnum,
            TransactionFut, UtxoStandardCoin, ValidateAddressResult, WithdrawFut, WithdrawRequest};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use bitcoin::blockdata::script::Script;
use bitcoin::hash_types::Txid;
#[cfg(not(target_arch = "wasm32"))] use bitcoin::hashes::Hash;
#[cfg(not(target_arch = "wasm32"))] use chain::TransactionOutput;
#[cfg(not(target_arch = "wasm32"))] use common::async_blocking;
#[cfg(not(target_arch = "wasm32"))]
use common::ip_addr::myipaddr;
use common::mm_ctx::MmArc;
use common::mm_error::prelude::*;
use common::mm_number::MmNumber;
use futures::lock::Mutex as AsyncMutex;
use futures01::Future;
#[cfg(not(target_arch = "wasm32"))] use keys::AddressHashEnum;
use lightning::chain::keysinterface::KeysManager;
use lightning::chain::WatchedOutput;
use lightning::ln::channelmanager::ChannelDetails;
use lightning::ln::{PaymentHash, PaymentPreimage, PaymentSecret};
#[cfg(not(target_arch = "wasm32"))]
use lightning_background_processor::BackgroundProcessor;
#[cfg(not(target_arch = "wasm32"))]
use lightning_invoice::utils::create_invoice_from_channelmanager;
#[cfg(not(target_arch = "wasm32"))]
use lightning_invoice::Invoice;
use ln_errors::{ConnectToNodeError, ConnectToNodeResult, EnableLightningError, EnableLightningResult,
                GenerateInvoiceError, GenerateInvoiceResult, GetNodeIdError, GetNodeIdResult, ListChannelsError,
                ListChannelsResult, ListPaymentsError, ListPaymentsResult, OpenChannelError, OpenChannelResult,
                SendPaymentError, SendPaymentResult};
#[cfg(not(target_arch = "wasm32"))]
use ln_events::LightningEventHandler;
#[cfg(not(target_arch = "wasm32"))]
use ln_storage::{last_request_id_path, nodes_data_path, parse_node_info, read_last_request_id_from_file,
                 read_nodes_data_from_file, save_last_request_id_to_file, save_node_data_to_file};
#[cfg(not(target_arch = "wasm32"))]
use ln_utils::{connect_to_node, open_ln_channel, ChannelManager, InvoicePayer, PeerManager};
use parking_lot::Mutex as PaMutex;
use rpc::v1::types::Bytes as BytesJson;
#[cfg(not(target_arch = "wasm32"))] use script::Builder;
use script::TransactionInputSigner;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};
use std::fmt;
#[cfg(not(target_arch = "wasm32"))] use std::str::FromStr;
use std::sync::Arc;

pub mod ln_errors;
#[cfg(not(target_arch = "wasm32"))] mod ln_events;
mod ln_rpc;
#[cfg(not(target_arch = "wasm32"))] mod ln_storage;
pub mod ln_utils;

#[derive(Debug)]
pub struct LightningProtocolConf {
    pub platform_coin_ticker: String,
}

pub struct PlatformFields {
    pub platform_coin: UtxoStandardCoin,
    // This cache stores the transactions that the LN node has interest in.
    pub registered_txs: PaMutex<HashMap<Txid, HashSet<Script>>>,
    // This cache stores the outputs that the LN node has interest in.
    pub registered_outputs: PaMutex<Vec<WatchedOutput>>,
    // This cache stores transactions to be broadcasted once the other node accepts the channel
    pub unsigned_funding_txs: AsyncMutex<HashMap<u64, TransactionInputSigner>>,
}

impl PlatformFields {
    pub fn add_tx(&self, txid: &Txid, script_pubkey: &Script) {
        let mut registered_txs = self.registered_txs.lock();
        match registered_txs.get_mut(txid) {
            Some(h) => {
                h.insert(script_pubkey.clone());
            },
            None => {
                let mut script_pubkeys = HashSet::new();
                script_pubkeys.insert(script_pubkey.clone());
                registered_txs.insert(*txid, script_pubkeys);
            },
        }
    }

    pub fn add_output(&self, output: WatchedOutput) {
        let mut registered_outputs = self.registered_outputs.lock();
        registered_outputs.push(output);
    }
}

#[derive(Debug)]
pub struct LightningCoinConf {
    ticker: String,
}

#[derive(Clone, Serialize)]
pub enum HTLCStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Clone)]
pub struct PaymentInfo {
    pub preimage: Option<PaymentPreimage>,
    pub secret: Option<PaymentSecret>,
    pub status: HTLCStatus,
    pub amt_msat: Option<u64>,
}

#[derive(Clone)]
pub struct LightningCoin {
    pub platform_fields: Arc<PlatformFields>,
    pub conf: Arc<LightningCoinConf>,
    /// The lightning node peer manager that takes care of connecting to peers, etc..
    #[cfg(not(target_arch = "wasm32"))]
    pub peer_manager: Arc<PeerManager>,
    /// The lightning node background processor that takes care of tasks that need to happen periodically
    #[cfg(not(target_arch = "wasm32"))]
    pub background_processor: Arc<BackgroundProcessor>,
    /// The lightning node channel manager which keeps track of the number of open channels and sends messages to the appropriate
    /// channel, also tracks HTLC preimages and forwards onion packets appropriately.
    #[cfg(not(target_arch = "wasm32"))]
    pub channel_manager: Arc<ChannelManager>,
    /// The lightning node keys manager that takes care of signing invoices.
    pub keys_manager: Arc<KeysManager>,
    /// The lightning node invoice payer.
    #[cfg(not(target_arch = "wasm32"))]
    pub invoice_payer: Arc<InvoicePayer<Arc<LightningEventHandler>>>,
    /// The mutex storing the inbound payments info.
    pub inbound_payments: Arc<AsyncMutex<HashMap<PaymentHash, PaymentInfo>>>,
    /// The mutex storing the outbound payments info.
    pub outbound_payments: Arc<AsyncMutex<HashMap<PaymentHash, PaymentInfo>>>,
}

impl fmt::Debug for LightningCoin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "LightningCoin {{ conf: {:?} }}", self.conf) }
}

impl LightningCoin {
    fn platform_coin(&self) -> &UtxoStandardCoin { &self.platform_fields.platform_coin }
}

#[async_trait]
impl SwapOps for LightningCoin {
    fn send_taker_fee(&self, _fee_addr: &[u8], _amount: BigDecimal, _uuid: &[u8]) -> TransactionFut { unimplemented!() }

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
        _uuid: &[u8],
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        unimplemented!()
    }

    fn validate_maker_payment(
        &self,
        _payment_tx: &[u8],
        _time_lock: u32,
        _maker_pub: &[u8],
        _secret_hash: &[u8],
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
        _secret_hash: &[u8],
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

    async fn search_for_swap_tx_spend_my(
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

    async fn search_for_swap_tx_spend_other(
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

    fn extract_secret(&self, _secret_hash: &[u8], _spend_tx: &[u8]) -> Result<Vec<u8>, String> { unimplemented!() }

    fn negotiate_swap_contract_addr(
        &self,
        _other_side_address: Option<&[u8]>,
    ) -> Result<Option<BytesJson>, MmError<NegotiateSwapContractAddrErr>> {
        unimplemented!()
    }
}

impl MarketCoinOps for LightningCoin {
    fn ticker(&self) -> &str { &self.conf.ticker }

    // Returns platform_coin address for now
    fn my_address(&self) -> Result<String, String> { self.platform_coin().my_address() }

    // Returns platform_coin balance for now
    fn my_balance(&self) -> BalanceFut<CoinBalance> { self.platform_coin().my_balance() }

    fn base_coin_balance(&self) -> BalanceFut<BigDecimal> { unimplemented!() }

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

    fn current_block(&self) -> Box<dyn Future<Item = u64, Error = String> + Send> {
        self.platform_coin().current_block()
    }

    fn display_priv_key(&self) -> Result<String, String> { unimplemented!() }

    fn min_tx_amount(&self) -> BigDecimal { unimplemented!() }

    fn min_trading_vol(&self) -> MmNumber { unimplemented!() }
}

impl MmCoin for LightningCoin {
    fn is_asset_chain(&self) -> bool { unimplemented!() }

    fn withdraw(&self, _req: WithdrawRequest) -> WithdrawFut { unimplemented!() }

    fn decimals(&self) -> u8 { unimplemented!() }

    fn convert_to_address(&self, _from: &str, _to_address_format: Json) -> Result<String, String> { unimplemented!() }

    fn validate_address(&self, _address: &str) -> ValidateAddressResult { unimplemented!() }

    fn process_history_loop(&self, _ctx: MmArc) -> Box<dyn Future<Item = (), Error = ()> + Send> { unimplemented!() }

    fn history_sync_status(&self) -> HistorySyncState { unimplemented!() }

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

    fn required_confirmations(&self) -> u64 { self.platform_coin().required_confirmations() }

    fn requires_notarization(&self) -> bool { self.platform_coin().requires_notarization() }

    fn set_required_confirmations(&self, _confirmations: u64) { unimplemented!() }

    fn set_requires_notarization(&self, _requires_nota: bool) { unimplemented!() }

    fn swap_contract_address(&self) -> Option<BytesJson> { unimplemented!() }

    fn mature_confirmations(&self) -> Option<u32> { self.platform_coin().mature_confirmations() }

    fn coin_protocol_info(&self) -> Vec<u8> { unimplemented!() }

    fn is_coin_protocol_supported(&self, _info: &Option<Vec<u8>>) -> bool { unimplemented!() }
}

#[derive(Deserialize)]
pub struct ConnectToNodeRequest {
    pub coin: String,
    pub node_id: String,
}

#[cfg(target_arch = "wasm32")]
pub async fn connect_to_lightning_node(_ctx: MmArc, _req: ConnectToNodeRequest) -> ConnectToNodeResult<String> {
    MmError::err(ConnectToNodeError::UnsupportedMode(
        "'connect_to_lightning_node'".into(),
        "native".into(),
    ))
}

/// Connect to a certain node on the lightning network.
#[cfg(not(target_arch = "wasm32"))]
pub async fn connect_to_lightning_node(ctx: MmArc, req: ConnectToNodeRequest) -> ConnectToNodeResult<String> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    let ln_coin = match coin {
        MmCoinEnum::LightningCoin(c) => c,
        _ => return MmError::err(ConnectToNodeError::UnsupportedCoin(coin.ticker().to_string())),
    };

    let (node_pubkey, node_addr) = parse_node_info(req.node_id.clone())?;
    let res = connect_to_node(node_pubkey, node_addr, ln_coin.peer_manager.clone()).await?;

    Ok(res.to_string())
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum ChannelOpenAmount {
    Exact(BigDecimal),
    Max,
}

fn get_true() -> bool { true }

#[allow(dead_code)]
#[derive(Debug, Deserialize, PartialEq)]
pub struct OpenChannelRequest {
    pub coin: String,
    pub node_id: String,
    pub amount: ChannelOpenAmount,
    /// The amount to push to the counterparty as part of the open, in milli-satoshi. Creates inbound liquidity for the channel.
    /// By setting push_msat to a value, opening channel request will be equivalent to opening a channel then sending a payment with
    /// the push_msat amount.
    #[serde(default)]
    pub push_msat: u64,
    #[serde(default = "get_true")]
    pub announce_channel: bool,
}

#[derive(Serialize)]
pub struct OpenChannelResponse {
    temporary_channel_id: [u8; 32],
    node_id: String,
    request_id: u64,
}

#[cfg(target_arch = "wasm32")]
pub async fn open_channel(_ctx: MmArc, _req: OpenChannelRequest) -> OpenChannelResult<OpenChannelResponse> {
    MmError::err(OpenChannelError::UnsupportedMode(
        "'open_channel'".into(),
        "native".into(),
    ))
}

/// Opens a channel on the lightning network.
#[cfg(not(target_arch = "wasm32"))]
pub async fn open_channel(ctx: MmArc, req: OpenChannelRequest) -> OpenChannelResult<OpenChannelResponse> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    let ln_coin = match coin {
        MmCoinEnum::LightningCoin(c) => c,
        _ => return MmError::err(OpenChannelError::UnsupportedCoin(coin.ticker().to_string())),
    };

    // Making sure that the node data is correct and that we can connect to it before doing more operations
    let (node_pubkey, node_addr) = parse_node_info(req.node_id.clone())?;
    connect_to_node(node_pubkey, node_addr, ln_coin.peer_manager.clone()).await?;

    let platform_coin = ln_coin.platform_coin().clone();
    let decimals = platform_coin.as_ref().decimals;
    let my_address = platform_coin.as_ref().derivation_method.iguana_or_err()?;
    let (unspents, _) = platform_coin.ordered_mature_unspents(my_address).await?;
    let (value, fee_policy) = match req.amount.clone() {
        ChannelOpenAmount::Max => (
            unspents.iter().fold(0, |sum, unspent| sum + unspent.value),
            FeePolicy::DeductFromOutput(0),
        ),
        ChannelOpenAmount::Exact(v) => {
            let value = sat_from_big_decimal(&v, decimals)?;
            (value, FeePolicy::SendExact)
        },
    };

    // The actual script_pubkey will replace this before signing the transaction after receiving the required
    // output script from the other node when the channel is accepted
    let script_pubkey =
        Builder::build_witness_script(&AddressHashEnum::WitnessScriptHash(Default::default())).to_bytes();
    let outputs = vec![TransactionOutput { value, script_pubkey }];

    let mut tx_builder = UtxoTxBuilder::new(&platform_coin)
        .add_available_inputs(unspents)
        .add_outputs(outputs)
        .with_fee_policy(fee_policy);

    let fee = platform_coin
        .get_tx_fee()
        .await
        .map_err(|e| OpenChannelError::RpcError(e.to_string()))?;
    tx_builder = tx_builder.with_fee(fee);

    let (unsigned, _) = tx_builder.build().await?;

    // Saving node data to reconnect to it on restart
    let ticker = ln_coin.ticker();
    let nodes_data = read_nodes_data_from_file(&nodes_data_path(&ctx, ticker))?;
    if !nodes_data.contains_key(&node_pubkey) {
        save_node_data_to_file(&nodes_data_path(&ctx, ticker), &req.node_id)?;
    }

    // Helps in tracking which FundingGenerationReady events corresponds to which open_channel call
    let request_id = match read_last_request_id_from_file(&last_request_id_path(&ctx, ticker)) {
        Ok(id) => id + 1,
        Err(e) => match e.get_inner() {
            OpenChannelError::InvalidPath(_) => 1,
            _ => return Err(e),
        },
    };
    save_last_request_id_to_file(&last_request_id_path(&ctx, ticker), request_id)?;

    let amount_in_sat = unsigned.outputs[0].value;
    let push_msat = req.push_msat;
    let announce_channel = req.announce_channel;
    let channel_manager = ln_coin.channel_manager.clone();
    let temporary_channel_id = async_blocking(move || {
        open_ln_channel(
            node_pubkey,
            amount_in_sat,
            push_msat,
            request_id,
            announce_channel,
            channel_manager,
        )
    })
    .await?;

    let mut unsigned_funding_txs = ln_coin.platform_fields.unsigned_funding_txs.lock().await;
    unsigned_funding_txs.insert(request_id, unsigned);

    Ok(OpenChannelResponse {
        temporary_channel_id,
        node_id: req.node_id,
        request_id,
    })
}

#[derive(Deserialize)]
pub struct ListChannelsRequest {
    pub coin: String,
}

#[derive(Serialize)]
pub struct ChannelDetailsForRPC {
    pub channel_id: String,
    pub counterparty_node_id: String,
    pub funding_tx: Option<String>,
    pub funding_tx_output_index: Option<u16>,
    pub funding_tx_value: u64,
    /// True if the channel was initiated (and thus funded) by us.
    pub is_outbound: bool,
    pub balance_msat: u64,
    pub outbound_capacity_msat: u64,
    pub inbound_capacity_msat: u64,
    // Channel is confirmed onchain, this means that funding_locked messages have been exchanged,
    // the channel is not currently being shut down, and the required confirmation count has been reached.
    pub confirmed: bool,
    // Channel is confirmed and funding_locked messages have been exchanged, the peer is connected,
    // and the channel is not currently negotiating a shutdown.
    pub is_usable: bool,
    // A publicly-announced channel.
    pub is_public: bool,
}

impl From<ChannelDetails> for ChannelDetailsForRPC {
    fn from(details: ChannelDetails) -> ChannelDetailsForRPC {
        ChannelDetailsForRPC {
            channel_id: hex::encode(details.channel_id),
            counterparty_node_id: details.counterparty.node_id.to_string(),
            funding_tx: details.funding_txo.map(|tx| tx.txid.to_string()),
            funding_tx_output_index: details.funding_txo.map(|tx| tx.index),
            funding_tx_value: details.channel_value_satoshis,
            is_outbound: details.is_outbound,
            balance_msat: details.balance_msat,
            outbound_capacity_msat: details.outbound_capacity_msat,
            inbound_capacity_msat: details.inbound_capacity_msat,
            confirmed: details.is_funding_locked,
            is_usable: details.is_usable,
            is_public: details.is_public,
        }
    }
}

#[derive(Serialize)]
pub struct ListChannelsResponse {
    channels: Vec<ChannelDetailsForRPC>,
}

#[cfg(target_arch = "wasm32")]
pub async fn list_channels(_ctx: MmArc, _req: ListChannelsRequest) -> ListChannelsResult<ListChannelsResponse> {
    MmError::err(ListChannelsError::UnsupportedMode(
        "'list_channels'".into(),
        "native".into(),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn list_channels(ctx: MmArc, req: ListChannelsRequest) -> ListChannelsResult<ListChannelsResponse> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    let ln_coin = match coin {
        MmCoinEnum::LightningCoin(c) => c,
        _ => return MmError::err(ListChannelsError::UnsupportedCoin(coin.ticker().to_string())),
    };
    let channels = ln_coin
        .channel_manager
        .list_channels()
        .into_iter()
        .map(From::from)
        .collect();

    Ok(ListChannelsResponse { channels })
}

#[derive(Deserialize)]
pub struct GenerateInvoiceRequest {
    pub coin: String,
    pub amount_in_msat: Option<u64>,
    pub description: String,
}

#[derive(Serialize)]
pub struct GenerateInvoiceResponse {
    invoice: String,
}

#[cfg(target_arch = "wasm32")]
pub async fn generate_invoice(
    _ctx: MmArc,
    _req: GenerateInvoiceRequest,
) -> GenerateInvoiceResult<GenerateInvoiceResponse> {
    MmError::err(GenerateInvoiceError::UnsupportedMode(
        "'generate_invoice'".into(),
        "native".into(),
    ))
}

/// Generates an invoice (request for payment) that can be paid on the lightning network by another node using send_payment.
#[cfg(not(target_arch = "wasm32"))]
pub async fn generate_invoice(
    ctx: MmArc,
    req: GenerateInvoiceRequest,
) -> GenerateInvoiceResult<GenerateInvoiceResponse> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    let ln_coin = match coin {
        MmCoinEnum::LightningCoin(c) => c,
        _ => return MmError::err(GenerateInvoiceError::UnsupportedCoin(coin.ticker().to_string())),
    };
    let network = ln_coin.platform_coin().as_ref().network.clone().into();
    let invoice = create_invoice_from_channelmanager(
        &ln_coin.channel_manager,
        ln_coin.keys_manager,
        network,
        req.amount_in_msat,
        req.description,
    )?
    .to_string();
    Ok(GenerateInvoiceResponse { invoice })
}

#[derive(Deserialize)]
pub struct GetNodeIdReq {
    pub coin: String,
}

#[derive(Serialize)]
pub struct GetNodeIdResponse {
    node_id: String,
}

#[cfg(target_arch = "wasm32")]
pub async fn get_ln_node_id(_ctx: MmArc, _req: GetNodeIdReq) -> GetNodeIdResult<GetNodeIdResponse> {
    MmError::err(GetNodeIdError::UnsupportedMode(
        "'get_ln_node_id'".into(),
        "native".into(),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn get_ln_node_id(ctx: MmArc, req: GetNodeIdReq) -> GetNodeIdResult<GetNodeIdResponse> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    let ln_coin = match coin {
        MmCoinEnum::LightningCoin(c) => c,
        _ => return MmError::err(GetNodeIdError::UnsupportedCoin(coin.ticker().to_string())),
    };
    let node_id = ln_coin.channel_manager.get_our_node_id().to_string();
    Ok(GetNodeIdResponse { node_id })
}

#[derive(Deserialize)]
pub struct SendPaymentReq {
    pub coin: String,
    pub invoice: String,
}

#[derive(Serialize)]
pub struct SendPaymentResponse {
    payment_id: String,
    payment_hash: String,
}

#[cfg(target_arch = "wasm32")]
pub async fn send_payment(_ctx: MmArc, _req: SendPaymentReq) -> SendPaymentResult<()> {
    MmError::err(SendPaymentError::UnsupportedMode(
        "'send_payment'".into(),
        "native".into(),
    ))
}

// TODO: Implement spontaneous payment (payment by node id).
#[cfg(not(target_arch = "wasm32"))]
pub async fn send_payment(ctx: MmArc, req: SendPaymentReq) -> SendPaymentResult<SendPaymentResponse> {
    let invoice = Invoice::from_str(&req.invoice).map_to_mm(|e| SendPaymentError::InvalidInvoice(e.to_string()))?;
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    let ln_coin = match coin {
        MmCoinEnum::LightningCoin(c) => c,
        _ => return MmError::err(SendPaymentError::UnsupportedCoin(coin.ticker().to_string())),
    };
    let payment_id = ln_coin
        .invoice_payer
        .pay_invoice(&invoice)
        .map_to_mm(|e| SendPaymentError::PaymentError(format!("{:?}", e)))?;
    let payment_hash = PaymentHash((*invoice.payment_hash()).into_inner());
    let payment_secret = Some(*invoice.payment_secret());
    let mut outbound_payments = ln_coin.outbound_payments.lock().await;
    outbound_payments.insert(payment_hash, PaymentInfo {
        preimage: None,
        secret: payment_secret,
        status: HTLCStatus::Pending,
        amt_msat: invoice.amount_milli_satoshis(),
    });
    Ok(SendPaymentResponse {
        payment_id: hex::encode(payment_id.0),
        payment_hash: hex::encode(payment_hash.0),
    })
}

#[derive(Deserialize)]
pub struct ListPaymentsReq {
    pub coin: String,
}

#[derive(Serialize)]
pub struct PaymentInfoForRPC {
    status: HTLCStatus,
    amount_in_msat: Option<u64>,
}

impl From<PaymentInfo> for PaymentInfoForRPC {
    fn from(info: PaymentInfo) -> Self {
        PaymentInfoForRPC {
            status: info.status,
            amount_in_msat: info.amt_msat,
        }
    }
}

#[derive(Serialize)]
pub struct ListPaymentsResponse {
    pub inbound_payments: HashMap<String, PaymentInfoForRPC>,
    pub outbound_payments: HashMap<String, PaymentInfoForRPC>,
}

#[cfg(target_arch = "wasm32")]
pub async fn list_payments(_ctx: MmArc, _req: ListPaymentsReq) -> ListPaymentsResult<()> {
    MmError::err(ListPaymentsError::UnsupportedMode(
        "'list_payments'".into(),
        "native".into(),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn list_payments(ctx: MmArc, req: ListPaymentsReq) -> ListPaymentsResult<ListPaymentsResponse> {
    let coin = lp_coinfind_or_err(&ctx, &req.coin).await?;
    let ln_coin = match coin {
        MmCoinEnum::LightningCoin(c) => c,
        _ => return MmError::err(ListPaymentsError::UnsupportedCoin(coin.ticker().to_string())),
    };
    let inbound_payments_info = ln_coin.inbound_payments.lock().await.clone();
    let mut inbound_payments = HashMap::new();
    for (payment_hash, payment_info) in inbound_payments_info.into_iter() {
        inbound_payments.insert(hex::encode(payment_hash.0), payment_info.into());
    }
    let outbound_payments_info = ln_coin.outbound_payments.lock().await.clone();
    let mut outbound_payments = HashMap::new();
    for (payment_hash, payment_info) in outbound_payments_info.into_iter() {
        outbound_payments.insert(hex::encode(payment_hash.0), payment_info.into());
    }

    Ok(ListPaymentsResponse {
        inbound_payments,
        outbound_payments,
    })
}
