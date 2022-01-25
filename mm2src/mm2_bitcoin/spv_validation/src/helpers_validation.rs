use bitcoin_spv::validatespv::prove;
use primitives::hash::H256;
use types::SPVError;

/// Evaluates a Bitcoin merkle inclusion proof.
/// Note that `index` is not a reliable indicator of location within a block.
///
/// # Arguments
///
/// * `txid` - The txid (LE)
/// * `merkle_root` - The merkle root (as in the block header) (LE)
/// * `intermediate_nodes` - The proof's intermediate nodes (digests between leaf and root) (LE)
/// * `index` - The leaf's index in the tree (0-indexed)
///
/// # Notes
/// Wrapper around `bitcoin_spv::validatespv::prove`
pub fn merkle_prove(txid: H256, merkle_root: H256, intermediate_nodes: Vec<H256>, index: u64) -> Result<(), SPVError> {
    if txid == merkle_root && index == 0 && intermediate_nodes.is_empty() {
        return Ok(());
    }
    let mut vec: Vec<u8> = vec![];
    for merkle_node in intermediate_nodes {
        vec.append(&mut merkle_node.as_slice().to_vec());
    }
    let nodes = bitcoin_spv::types::MerkleArray::new(vec.as_slice()).unwrap();
    if !prove(txid.take().into(), merkle_root.take().into(), &nodes, index) {
        return Err(SPVError::BadMerkleProof);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_prove_inclusion() {
        // https://rick.explorer.dexstats.info/tx/7e9797a05abafbc1542449766ef9a41838ebbf6d24cd3223d361aa07c51981df
        // merkle intermediate nodes 2 element
        let tx_id: H256 = H256::from_reversed_str("7e9797a05abafbc1542449766ef9a41838ebbf6d24cd3223d361aa07c51981df");
        let merkle_pos = 1;
        let merkle_root: H256 =
            H256::from_reversed_str("41f138275d13690e3c5d735e2f88eb6f1aaade1207eb09fa27a65b40711f3ae0").into();
        let merkle_nodes: Vec<H256> = vec![
            H256::from_reversed_str("73dfb53e6f49854b09d98500d4899d5c4e703c4fa3a2ddadc2cd7f12b72d4182"),
            H256::from_reversed_str("4274d707b2308d39a04f2940024d382fa80d994152a50d4258f5a7feead2a563"),
        ];
        let result = merkle_prove(tx_id, merkle_root, merkle_nodes, merkle_pos);
        assert_eq!(result.is_err(), false);
    }

    #[test]
    fn test_merkle_prove_inclusion_single_element() {
        // https://www.blockchain.com/btc/tx/c06fbab289f723c6261d3030ddb6be121f7d2508d77862bb1e484f5cd7f92b25
        // merkle intermediate nodes single element
        let tx_id: H256 = H256::from_reversed_str("c06fbab289f723c6261d3030ddb6be121f7d2508d77862bb1e484f5cd7f92b25");
        let merkle_pos = 0;
        let merkle_root: H256 =
            H256::from_reversed_str("8fb300e3fdb6f30a4c67233b997f99fdd518b968b9a3fd65857bfe78b2600719").into();
        let merkle_nodes: Vec<H256> = vec![H256::from_reversed_str(
            "5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2",
        )];
        let result = merkle_prove(tx_id, merkle_root, merkle_nodes, merkle_pos);
        assert_eq!(result.is_err(), false);
    }
}
