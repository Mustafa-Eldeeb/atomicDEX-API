use bitcoin_spv::btcspv::{validate_vin, validate_vout};
use chain::BlockHeader;
use helpers_validation::merkle_prove;
use primitives::hash::H256;
use types::SPVError;

#[derive(PartialEq, Clone)]
pub struct SPVProof {
    /// The tx id
    pub tx_id: H256,
    /// The vin serialized
    pub vin: Vec<u8>,
    /// The vout serialized
    pub vout: Vec<u8>,
    /// The transaction index in the merkle tree
    pub index: u64,
    /// The confirming UTXO header
    pub confirming_header: BlockHeader,
    /// The intermediate nodes (digests between leaf and root)
    pub intermediate_nodes: Vec<H256>,
}

/// Checks validity of an entire SPV Proof
///
/// # Arguments
///
/// * `self` - The SPV Proof
///
/// # Errors
///
/// * Errors if any of the SPV Proof elements are invalid.
///
/// # Notes
/// Re-write with our own types based on `bitcoin_spv::std_types::SPVProof::validate`
/// Support only merkle proof inclusion,vin,vout for now
impl SPVProof {
    pub fn validate(&self) -> Result<(), SPVError> {
        if !validate_vin(self.vin.as_slice()) {
            return Err(SPVError::InvalidVin);
        }
        if !validate_vout(self.vout.as_slice()) {
            return Err(SPVError::InvalidVout);
        }
        merkle_prove(
            self.tx_id,
            self.confirming_header.merkle_root_hash,
            self.intermediate_nodes.clone(),
            self.index,
        )
    }
}
