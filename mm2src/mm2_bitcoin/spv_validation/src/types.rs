#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SPVError {
    BadMerkleProof,
    UnknownError(String),
}
