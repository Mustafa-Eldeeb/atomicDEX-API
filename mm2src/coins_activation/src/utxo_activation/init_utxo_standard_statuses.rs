use crate::standalone_coin::InitStandaloneCoinInitialStatus;
use crypto::trezor::TrezorPinMatrix3x3Response;
use rpc_task::RpcTaskError;
use serde_derive::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Clone, Serialize)]
pub enum UtxoStandardInProgressStatus {
    ActivatingCoin,
    Finishing,
    /// This status doesn't require the user to send `UserAction`,
    /// but it tells the user that he should confirm/decline an address on his device.
    WaitingForTrezorToConnect,
    WaitingForUserToConfirmPubkey,
}

impl InitStandaloneCoinInitialStatus for UtxoStandardInProgressStatus {
    fn initial_status() -> Self { UtxoStandardInProgressStatus::ActivatingCoin }
}

#[derive(Clone, Deserialize, Serialize)]
pub enum UtxoStandardAwaitingStatus {
    WaitForTrezorPin,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "action_type")]
pub enum UtxoStandardUserAction {
    TrezorPin(TrezorPinMatrix3x3Response),
}

impl TryFrom<UtxoStandardUserAction> for TrezorPinMatrix3x3Response {
    type Error = RpcTaskError;

    fn try_from(value: UtxoStandardUserAction) -> Result<Self, Self::Error> {
        match value {
            UtxoStandardUserAction::TrezorPin(pin) => Ok(pin),
        }
    }
}
