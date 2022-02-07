#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SPVError {
    TxHistoryNotAvailable,
    TxHeightNotAvailable,
    BadMerkleProof,
    UnknownError(String),
}
