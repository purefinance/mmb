use crate::services::liquidity::LiquidityData;
use crate::ws::actors::ws_client_session::WsClientSession;
use crate::ws::commands::liquidity::LiquidityResponseBody;
use crate::ws::subscribes::liquidity::LiquiditySubscription;
use actix::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct LiquidityResponseMessage {
    pub command: &'static str,
    pub body: LiquidityResponseBody,
    pub subscription: LiquiditySubscription,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct ClientErrorResponseMessage {
    pub command: &'static str,
    pub subscription: u64,
    pub content: Value,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct NewLiquidityDataMessage {
    pub data: LiquidityData,
    pub subscription: LiquiditySubscription,
}

#[derive(Clone, Message)]
#[rtype(result = "HashSet<LiquiditySubscription>")]
pub struct GetLiquiditySubscriptions;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct ClientConnected {
    pub data: Addr<WsClientSession>,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct ClientDisconnected {
    pub data: Addr<WsClientSession>,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct GatherSubscriptions;

#[derive(Clone, Message)]
#[rtype(result = "Option<LiquiditySubscription>")]
pub struct GetSessionLiquiditySubscription;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct ClearSubscriptions;

#[derive(Message)]
#[rtype(result = "()")]
pub struct SubscriptionErrorMessage {
    pub subscription: u64,
    pub message: String,
}
