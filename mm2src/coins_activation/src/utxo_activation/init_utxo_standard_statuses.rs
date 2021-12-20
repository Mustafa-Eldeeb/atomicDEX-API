use crate::standalone_coin::InitStandaloneCoinInitialStatus;
use crypto::trezor::TrezorPinMatrix3x3Response;
use serde_derive::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
pub enum UtxoStandardInProgressStatus {
    ActivatingCoin,
    /// This status doesn't require the user to send `UserAction`,
    /// but it tells the user that he should confirm/decline an address on his device.
    #[allow(dead_code)]
    WaitingForUserToConfirmAddress {
        address: String,
    },
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
