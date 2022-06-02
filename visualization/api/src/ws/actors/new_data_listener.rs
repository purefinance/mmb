use crate::ws::broker_messages::{LiquidityResponseMessage, NewLiquidityDataMessage};
use crate::ws::commands::liquidity::LiquidityResponseBody;
use actix::{Actor, Context, Handler};
use actix_broker::BrokerIssue;

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
        let body: LiquidityResponseBody =
            serde_json::from_value(data.data.record.data).expect("Failed to parse json");
        let liquidity_response_message = LiquidityResponseMessage {
            command: "UpdateOrdersState",
            body,
            currency_pair: data.data.currency_pair,
            exchange_id: data.data.exchange_id,
        };
        self.issue_system_async(liquidity_response_message);
    }
}
