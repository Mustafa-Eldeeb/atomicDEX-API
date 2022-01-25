#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SPVError {
    NonSpvClient,
    TxHistoryNotAvailable,
    TxHeightNotAvailable,
    BadMerkleProof,
    UnknownError(String),
}
