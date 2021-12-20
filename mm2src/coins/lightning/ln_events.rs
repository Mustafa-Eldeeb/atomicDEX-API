use super::*;
use bitcoin::blockdata::script::Script;
use bitcoin::blockdata::transaction::{Transaction, TxOut};
use common::{block_on, log};
use lightning::chain::transaction::OutPoint;
use lightning::chain::Filter;
use lightning::util::events::{Event, EventHandler};
use script::{Builder, SignatureVersion};
use std::convert::TryFrom;
use std::sync::Arc;
use utxo_signer::with_key_pair::sign_tx;

pub struct LightningEventHandler {
    filter: Arc<PlatformFields>,
    channel_manager: Arc<ChannelManager>,
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
            Event::PaymentReceived { .. } => (),
            Event::PaymentSent { .. } => (),
            Event::PaymentPathFailed { .. } => (),
            Event::PaymentFailed { .. } => (),
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
    pub fn new(filter: Arc<PlatformFields>, channel_manager: Arc<ChannelManager>) -> Self {
        LightningEventHandler {
            filter,
            channel_manager,
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
}
