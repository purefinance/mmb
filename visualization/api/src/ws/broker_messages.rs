use std::collections::HashSet;

use actix::prelude::*;
use serde_json::Value;

use crate::services::data_provider::balances::BalancesData;
use crate::services::data_provider::liquidity::LiquidityData;
use crate::ws::actors::ws_client_session::WsClientSession;
use crate::ws::commands::liquidity::LiquidityResponseBody;
use crate::ws::subscribes::balance::BalancesSubscription;
use crate::ws::subscribes::liquidity::LiquiditySubscription;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct LiquidityResponseMessage {
    pub command: &'static str,
    pub body: LiquidityResponseBody,
    pub subscription: LiquiditySubscription,
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct BalancesResponseMessage {
    pub command: &'static str,
    pub body: BalancesData,
    pub subscription: BalancesSubscription,
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
#[rtype(result = "()")]
pub struct NewBalancesDataMessage {
    pub data: BalancesData,
    pub subscription: BalancesSubscription,
}

#[derive(Clone, Message)]
#[rtype(result = "GetSubscriptionsResponse")]
pub struct GetSubscriptions;

#[derive(Clone, Message)]
#[rtype(result = "bool")]
pub struct GetBalanceSubscriptions;

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
#[rtype(result = "Option<BalancesSubscription>")]
pub struct GetSessionBalancesSubscription;

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct ClearSubscriptions;

#[derive(Message)]
#[rtype(result = "()")]
pub struct SubscriptionErrorMessage {
    pub subscription: u64,
    pub message: String,
}

pub struct GetSubscriptionsResponse {
    pub liquidity: HashSet<LiquiditySubscription>,
    pub balances: Option<BalancesSubscription>,
}
