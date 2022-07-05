use crate::ws::broker_messages::{ClientErrorResponseMessage, SubscriptionErrorMessage};
use actix::{Actor, Context, Handler};
use actix_broker::BrokerIssue;
use serde_json::json;

#[derive(Default)]
pub struct ErrorListener;

/// This Actor intercepts errors
impl Actor for ErrorListener {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("Error listener started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("Error listener stopped");
    }
}

impl Handler<SubscriptionErrorMessage> for ErrorListener {
    type Result = ();

    fn handle(&mut self, data: SubscriptionErrorMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let error = ClientErrorResponseMessage {
            command: "Error",
            subscription: data.subscription,
            content: json!({"message" : data.message}),
        };
        self.issue_system_async(error);
    }
}
