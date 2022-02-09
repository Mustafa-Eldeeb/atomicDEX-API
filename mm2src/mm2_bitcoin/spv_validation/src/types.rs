#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SPVError {
    TxHistoryNotAvailable,
    TxHeightNotAvailable,
    /// A `vin` (transaction input vector) is malformatted.
    InvalidVin,
    /// A `vout` (transaction output vector) is malformatted.
    InvalidVout,
    BadMerkleProof,
    UnknownError(String),
}
