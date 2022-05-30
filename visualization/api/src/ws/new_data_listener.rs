use crate::ws::broker_messages::{LiquidityDataMessage, MessageToClient};
use crate::ws::commands::liquidity::Liquidity;
use actix::{Actor, Context, Handler};
use actix_broker::BrokerIssue;

#[derive(Default)]
pub struct NewDataListener;

/// This Actor intercepts external events
impl Actor for NewDataListener {
    type Context = Context<Self>;
}
/// Global message handler. Intercepting raised LiquidityDataMessage event
impl Handler<LiquidityDataMessage> for NewDataListener {
    type Result = ();

    fn handle(&mut self, data: LiquidityDataMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let content: Liquidity = serde_json::from_value(data.data).expect("Failed to parse json");
        let content = serde_json::to_string(&content).expect("Failed to convert json");
        let message = MessageToClient {
            command: "UpdateOrdersState".to_string(),
            content,
        };
        self.issue_system_async(message);
    }
}
