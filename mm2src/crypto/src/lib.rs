#[macro_use] extern crate serde_derive;

mod crypto_ctx;
mod hw_client;
mod hw_ctx;
pub mod hw_rpc_task;
mod key_pair_ctx;

pub use crypto_ctx::{CryptoCtx, CryptoInitError, CryptoInitResult};
pub use hw_client::TrezorConnectProcessor;
pub use hw_client::{HwClient, HwError, HwProcessingError, HwResult, HwWalletType};
pub use hw_common::primitives::{Bip32Error, ChildNumber, DerivationPath, EcdsaCurve, ExtendedPublicKey,
                                Secp256k1ExtendedPublicKey};
pub use hw_ctx::HardwareWalletCtx;
pub use key_pair_ctx::KeyPairCtx;
use std::str::FromStr;
pub use trezor;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq)]
pub struct RpcDerivationPath(pub DerivationPath);

impl From<DerivationPath> for RpcDerivationPath {
    fn from(der: DerivationPath) -> Self { RpcDerivationPath(der) }
}

impl Serialize for RpcDerivationPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for RpcDerivationPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = String::deserialize(deserializer)?;
        let inner = DerivationPath::from_str(&path).map_err(|e| D::Error::custom(format!("{}", e)))?;
        Ok(RpcDerivationPath(inner))
    }
}
