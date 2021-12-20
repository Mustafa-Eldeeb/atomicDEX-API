use crate::prelude::CoinAddressInfo;
use coins::CoinBalance;
use serde_derive::Serialize;
use std::collections::HashMap;

#[derive(Clone, Serialize)]
pub struct UtxoStandardActivationResult {
    pub current_block: u64,
    pub addresses_infos: HashMap<String, CoinAddressInfo<CoinBalance>>,
}
