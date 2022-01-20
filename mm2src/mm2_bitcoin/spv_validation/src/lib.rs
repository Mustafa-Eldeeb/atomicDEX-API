extern crate bitcoin_spv;
extern crate primitives;

/// `types` exposes simple types for on-chain evaluation of SPV proofs
pub mod types;

/// `helpers_validation` Override function modules from bitcoin_spv and adapt for our mm2_bitcoin library
pub mod helpers_validation;
