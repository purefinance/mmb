use super::common::CurrencyPair;
use super::common_interaction::*;
use crate::core::connectivity::websocket_actor::WebSocketParams;
use crate::core::exchanges::binance::Binance;
use crate::core::exchanges::common::SpecificCurrencyPair;
use crate::core::orders::order::{DataToCancelOrder, DataToCreateOrder};
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole, exchanges::common::ExchangeAccountId,
};
use actix::{Actor, Context, Handler, Message};
use log::trace;

// TODO implement Common_interaction for ExchangeActor? Just redirect there
pub struct ExchangeActor {
    exchange_account_id: ExchangeAccountId,
    websocket_host: String,
    specific_currency_pairs: Vec<SpecificCurrencyPair>,
    websocket_channels: Vec<String>,
    exchange_interaction: Box<dyn CommonInteraction>,
}

impl ExchangeActor {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        websocket_host: String,
        specific_currency_pairs: Vec<SpecificCurrencyPair>,
        websocket_channels: Vec<String>,
        exchange_interaction: Box<dyn CommonInteraction>,
    ) -> Self {
        ExchangeActor {
            exchange_account_id,
            websocket_host,
            specific_currency_pairs,
            websocket_channels,
            exchange_interaction,
        }
    }

    pub fn create_websocket_params(&mut self, ws_path: &str) -> WebSocketParams {
        WebSocketParams::new(
            format!("{}{}", self.websocket_host, ws_path)
                .parse()
                .expect("should be valid url"),
        )
    }

    pub async fn create_order(&self, order: &DataToCreateOrder) {
        let order_create_task = self.exchange_interaction.create_order(&order);

        tokio::select! {
            _ = order_create_task => {
                dbg!(&"Request complited");
            // TODO Also here has to be cancelation_token_task
            }
        }
    }

    pub async fn cancel_order(&self, order: &DataToCancelOrder) {
        self.exchange_interaction.cancel_order(&order).await;
    }

    pub async fn cancel_all_orders(&self) {
        self.exchange_interaction
            .cancel_all_orders(CurrencyPair::new("".into()))
            .await;
    }

    pub async fn get_account_info(&self) {
        self.exchange_interaction.get_account_info().await;
    }
}

impl Actor for ExchangeActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        trace!("ExchangeActor '{}' started", self.exchange_account_id);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("ExchangeActor '{}' stopped", self.exchange_account_id);
    }
}

pub struct GetWebSocketParams(pub WebSocketRole);

impl Message for GetWebSocketParams {
    type Result = Option<WebSocketParams>;
}

impl Handler<GetWebSocketParams> for ExchangeActor {
    type Result = Option<WebSocketParams>;

    fn handle(&mut self, msg: GetWebSocketParams, _ctx: &mut Self::Context) -> Self::Result {
        let websocket_role = msg.0;
        match websocket_role {
            WebSocketRole::Main => {
                // TODO remove hardcode
                let ws_path = Binance::build_ws1_path(
                    &self.specific_currency_pairs[..],
                    &self.websocket_channels[..],
                );
                Some(self.create_websocket_params(&ws_path))
            }
            WebSocketRole::Secondary => None,
        }
    }
}
