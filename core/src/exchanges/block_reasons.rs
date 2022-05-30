use crate::exchanges::exchange_blocker::BlockReason;

macro_rules! impl_block_reason {
    ($name: ident) => {
        pub static $name: BlockReason = BlockReason::new(stringify!($name));
    };
}

impl_block_reason!(WEBSOCKET_DISCONNECTED);
impl_block_reason!(REQUEST_LIMIT);
impl_block_reason!(CREATE_ORDER_INSUFFICIENT_FUNDS);
impl_block_reason!(REST_RATE_LIMIT);
impl_block_reason!(GRACEFUL_SHUTDOWN);
impl_block_reason!(EXCHANGE_UNAVAILABLE);
