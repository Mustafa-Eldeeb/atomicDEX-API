mod init_standalone_coin;
mod init_standalone_coin_error;

pub use init_standalone_coin::{init_standalone_coin, init_standalone_coin_status, InitStandaloneCoinActivationOps,
                               InitStandaloneCoinInitialStatus, init_standalone_coin_user_action};
pub use init_standalone_coin_error::InitStandaloneCoinError;
