use super::*;
use bitcoin::blockdata::script::Script;
use bitcoin::blockdata::transaction::{Transaction, TxOut};
use common::{block_on, log};
use lightning::chain::transaction::OutPoint;
use lightning::chain::Filter;
use lightning::util::events::{Event, EventHandler, PaymentPurpose};
use script::{Builder, SignatureVersion};
use std::collections::hash_map::Entry;
use std::convert::TryFrom;
use std::sync::Arc;
use utxo_signer::with_key_pair::sign_tx;

pub struct LightningEventHandler {
    filter: Arc<PlatformFields>,
    channel_manager: Arc<ChannelManager>,
    inbound_payments: Arc<AsyncMutex<HashMap<PaymentHash, PaymentInfo>>>,
    outbound_payments: Arc<AsyncMutex<HashMap<PaymentHash, PaymentInfo>>>,
}

impl EventHandler for LightningEventHandler {
    // TODO: Implement all the cases
    fn handle_event(&self, event: &Event) {
        match event {
            Event::FundingGenerationReady {
                temporary_channel_id,
                channel_value_satoshis,
                output_script,
                user_channel_id,
            } => self.handle_funding_generation_ready(
                *temporary_channel_id,
                *channel_value_satoshis,
                output_script,
                *user_channel_id,
            ),
            Event::PaymentReceived {
                payment_hash,
                amt,
                purpose,
            } => self.handle_payment_received(*payment_hash, *amt, purpose),
            Event::PaymentSent {
                payment_preimage,
                payment_hash,
                ..
            } => self.handle_payment_sent(*payment_preimage, *payment_hash),
            Event::PaymentPathFailed { .. } => (),
            Event::PaymentFailed { payment_hash, .. } => self.handle_payment_failed(payment_hash),
            Event::PendingHTLCsForwardable { .. } => (),
            Event::SpendableOutputs { .. } => (),
            Event::PaymentForwarded { .. } => (),
            Event::ChannelClosed { .. } => (),
            Event::DiscardFunding { .. } => (),
            Event::PaymentPathSuccessful { .. } => (),
        }
    }
}

// Generates the raw funding transaction with one output equal to the channel value.
async fn sign_funding_transaction(
    request_id: u64,
    output_script: &Script,
    filter: Arc<PlatformFields>,
) -> OpenChannelResult<Transaction> {
    let coin = &filter.platform_coin;
    let mut unsigned = {
        let unsigned_funding_txs = filter.unsigned_funding_txs.lock().await;
        unsigned_funding_txs
            .get(&request_id)
            .ok_or_else(|| {
                OpenChannelError::InternalError(format!("Unsigned funding tx not found for request id: {}", request_id))
            })?
            .clone()
    };
    unsigned.outputs[0].script_pubkey = output_script.to_bytes().into();

    let my_address = coin.as_ref().derivation_method.iguana_or_err()?;
    let key_pair = coin.as_ref().priv_key_policy.key_pair_or_err()?;

    let prev_script = Builder::build_p2pkh(&my_address.hash);
    let signed = sign_tx(
        unsigned,
        key_pair,
        prev_script,
        SignatureVersion::WitnessV0,
        coin.as_ref().conf.fork_id,
    )?;

    Transaction::try_from(signed).map_to_mm(|e| OpenChannelError::ConvertTxErr(e.to_string()))
}

impl LightningEventHandler {
    pub fn new(
        filter: Arc<PlatformFields>,
        channel_manager: Arc<ChannelManager>,
        inbound_payments: Arc<AsyncMutex<HashMap<PaymentHash, PaymentInfo>>>,
        outbound_payments: Arc<AsyncMutex<HashMap<PaymentHash, PaymentInfo>>>,
    ) -> Self {
        LightningEventHandler {
            filter,
            channel_manager,
            inbound_payments,
            outbound_payments,
        }
    }

    fn handle_funding_generation_ready(
        &self,
        temporary_channel_id: [u8; 32],
        channel_value_satoshis: u64,
        output_script: &Script,
        user_channel_id: u64,
    ) {
        let funding_tx = match block_on(sign_funding_transaction(
            user_channel_id,
            output_script,
            self.filter.clone(),
        )) {
            Ok(tx) => tx,
            Err(e) => {
                log::error!(
                    "Error generating funding transaction for temporary channel id {:?}: {}",
                    temporary_channel_id,
                    e.to_string()
                );
                // TODO: use issue_channel_close_events here when implementing channel closure this will push a Event::DiscardFunding
                // event for the other peer
                return;
            },
        };
        // Give the funding transaction back to LDK for opening the channel.
        match self
            .channel_manager
            .funding_transaction_generated(&temporary_channel_id, funding_tx.clone())
        {
            Ok(_) => {
                let txid = funding_tx.txid();
                self.filter.register_tx(&txid, output_script);
                let output_to_be_registered = TxOut {
                    value: channel_value_satoshis,
                    script_pubkey: output_script.clone(),
                };
                let output_index = match funding_tx
                    .output
                    .iter()
                    .position(|tx_out| tx_out == &output_to_be_registered)
                {
                    Some(i) => i,
                    None => {
                        log::error!(
                            "Output to register is not found in the output of the transaction: {}",
                            txid
                        );
                        return;
                    },
                };
                self.filter.register_output(WatchedOutput {
                    block_hash: None,
                    outpoint: OutPoint {
                        txid,
                        index: output_index as u16,
                    },
                    script_pubkey: output_script.clone(),
                });
            },
            // When transaction is unconfirmed by process_txs_confirmations LDK will try to rebroadcast the tx
            Err(e) => log::error!("{:?}", e),
        }
    }

    fn handle_payment_received(&self, payment_hash: PaymentHash, amt: u64, purpose: &PaymentPurpose) {
        let (payment_preimage, payment_secret) = match purpose {
            PaymentPurpose::InvoicePayment {
                payment_preimage,
                payment_secret,
            } => match payment_preimage {
                Some(preimage) => (*preimage, Some(*payment_secret)),
                None => return,
            },
            PaymentPurpose::SpontaneousPayment(preimage) => (*preimage, None),
        };
        let status = match self.channel_manager.claim_funds(payment_preimage) {
            true => {
                log::info!(
                    "Received an amount of {} millisatoshis for payment hash {}",
                    amt,
                    hex::encode(payment_hash.0)
                );
                HTLCStatus::Succeeded
            },
            false => HTLCStatus::Failed,
        };
        let mut payments = block_on(self.inbound_payments.lock());
        match payments.entry(payment_hash) {
            Entry::Occupied(mut e) => {
                let payment = e.get_mut();
                payment.status = status;
                payment.preimage = Some(payment_preimage);
                payment.secret = payment_secret;
            },
            Entry::Vacant(e) => {
                e.insert(PaymentInfo {
                    preimage: Some(payment_preimage),
                    secret: payment_secret,
                    status,
                    amt_msat: Some(amt),
                });
            },
        }
    }

    fn handle_payment_sent(&self, payment_preimage: PaymentPreimage, payment_hash: PaymentHash) {
        let mut outbound_payments = block_on(self.outbound_payments.lock());
        for (hash, payment) in outbound_payments.iter_mut() {
            if *hash == payment_hash {
                payment.preimage = Some(payment_preimage);
                payment.status = HTLCStatus::Succeeded;
                log::info!(
                    "Successfully sent payment of {} millisatoshis with payment hash {}",
                    payment.amt_msat.unwrap_or_default(),
                    hex::encode(payment_hash.0)
                );
            }
        }
    }

    fn handle_payment_failed(&self, payment_hash: &PaymentHash) {
        let mut outbound_payments = block_on(self.outbound_payments.lock());
        let outbound_payment = outbound_payments.get_mut(payment_hash);
        if let Some(payment) = outbound_payment {
            payment.status = HTLCStatus::Failed;
        }
    }
}
