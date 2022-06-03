use crate::services::liquidity::LiquidityData;
use crate::ws::actors::ws_client_session::WsClientSession;
use crate::ws::commands::liquidity::LiquidityResponseBody;
use crate::ws::subscribes::liquidity::LiquiditySubscription;
use actix::prelude::*;
use std::collections::HashSet;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct LiquidityResponseMessage {
    pub command: &'static str,
    pub exchange_id: String,
    pub currency_pair: String,
    pub body: LiquidityResponseBody,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct NewLiquidityDataMessage {
    pub data: LiquidityData,
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
