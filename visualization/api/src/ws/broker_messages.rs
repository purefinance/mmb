use actix::prelude::*;
use serde_json::Value;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct MessageToClient {
    pub command: String,
    pub content: String,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct LiquidityDataMessage {
    pub data: Value,
}
