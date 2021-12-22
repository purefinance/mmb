use crate::exchanges::exchange_blocker::BlockReason;

pub static CONNECTIVITY_MANAGER_RECONNECT: BlockReason =
    BlockReason::new("CONNECTIVITY_MANAGER_RECONNECT");
pub static REQUEST_LIMIT: BlockReason = BlockReason::new("REQUEST_LIMIT");
pub static CREATE_ORDER_INSUFFICIENT_FUNDS: BlockReason =
    BlockReason::new("CREATE_ORDER_INSUFFICIENT_FUNDS");
pub static REST_RATE_LIMIT: BlockReason = BlockReason::new("REST_RATE_LIMIT");
pub static GRACEFUL_SHUTDOWN: BlockReason = BlockReason::new("GRACEFUL_SHUTDOWN");
pub static EXCHANGE_UNAVAILABLE: BlockReason = BlockReason::new("EXCHANGE_UNAVAILABLE");
