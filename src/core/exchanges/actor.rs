use super::common_interaction::*;
use crate::core::connectivity::websocket_actor::WebSocketParams;
use crate::core::exchanges::binance::Binance;
use crate::core::exchanges::common::CurrencyPair;
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole, exchanges::common::ExchangeId,
};
use actix::{Actor, Context, Handler, Message, System};
use log::trace;

pub struct ExchangeActor {
    exchange_id: ExchangeId,
    websocket_host: String,
    currency_pairs: Vec<CurrencyPair>,
    websocket_channels: Vec<String>,
    exchange_interaction: Box<dyn CommonInteraction>,
}

impl ExchangeActor {
    pub fn new(
        exchange_id: ExchangeId,
        websocket_host: String,
        currency_pairs: Vec<CurrencyPair>,
        websocket_channels: Vec<String>,
        exchange_interaction: Box<dyn CommonInteraction>,
    ) -> Self {
        ExchangeActor {
            exchange_id,
            websocket_host,
            currency_pairs,
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

    pub async fn create_order(&self) {
        self.exchange_interaction.create_order().await;
    }
}

impl Actor for ExchangeActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        dbg!(&"WORKED");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("ExchangeActor '{}' stopped", self.exchange_id);
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
                let ws_path =
                    Binance::build_ws1_path(&self.currency_pairs[..], &self.websocket_channels[..]);
                Some(self.create_websocket_params(&ws_path))
            }
            WebSocketRole::Secondary => None,
        }
    }
}
