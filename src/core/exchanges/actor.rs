use actix::{Actor, Context, Handler, Message};
use log::trace;
use crate::core::{
        connectivity::connectivity_manager::WebSocketRole,
        exchanges::common::ExchangeAccountId
};
use crate::core::connectivity::websocket_actor::WebSocketParams;
use crate::core::exchanges::binance::Binance;
use crate::core::exchanges::common::SpecificCurrencyPair;

pub struct ExchangeActor {
    exchange_account_id: ExchangeAccountId,
    websocket_host: String,
    specific_currency_pairs: Vec<SpecificCurrencyPair>,
    websocket_channels: Vec<String>
}

impl ExchangeActor {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        websocket_host: String,
        specific_currency_pairs: Vec<SpecificCurrencyPair>,
        websocket_channels: Vec<String>
    ) -> Self {
        ExchangeActor {
            exchange_account_id,
            websocket_host,
            specific_currency_pairs,
            websocket_channels
        }
    }

    pub fn create_websocket_params(&mut self, ws_path: &str) -> WebSocketParams {
        WebSocketParams::new(format!("{}{}", self.websocket_host, ws_path).parse().expect("should be valid url"))
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
                let ws_path = Binance::build_ws1_path(&self.specific_currency_pairs[..], &self.websocket_channels[..]);
                Some(self.create_websocket_params(&ws_path))
            }
            WebSocketRole::Secondary => None
        }
    }
}