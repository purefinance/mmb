use actix::{Actor, Context, Handler};
use actix_broker::BrokerIssue;

use crate::ws::broker_messages::{
    BalancesResponseMessage, LiquidityResponseMessage, NewBalancesDataMessage,
    NewLiquidityDataMessage,
};
use crate::ws::commands::liquidity::LiquidityResponseBody;

#[derive(Default)]
pub struct NewDataListener;

/// This Actor intercepts external events
impl Actor for NewDataListener {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("Data listener started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("Data listener stopped");
    }
}

impl Handler<NewLiquidityDataMessage> for NewDataListener {
    type Result = ();

    fn handle(&mut self, data: NewLiquidityDataMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let body: LiquidityResponseBody = LiquidityResponseBody::from(data.data);
        let liquidity_response_message = LiquidityResponseMessage {
            command: "UpdateOrdersState",
            body,
            subscription: data.subscription,
        };
        self.issue_system_async(liquidity_response_message);
    }
}

impl Handler<NewBalancesDataMessage> for NewDataListener {
    type Result = ();

    fn handle(&mut self, data: NewBalancesDataMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let balances_response_message = BalancesResponseMessage {
            command: "UpdateBalances",
            body: data.data,
            subscription: data.subscription,
        };
        self.issue_system_async(balances_response_message);
    }
}
