use crate::bitmex::Bitmex;
use anyhow::{Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use mmb_core::connectivity::WebSocketRole;
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
    Support,
};
use mmb_core::settings::ExchangeSettings;
use mmb_domain::market::{CurrencyCode, CurrencyId, CurrencyPair, SpecificCurrencyPair};
use std::any::Any;
use url::Url;

#[async_trait]
impl Support for Bitmex {
    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static) {
        todo!()
    }

    fn on_websocket_message(&self, _msg: &str) -> Result<()> {
        // TODO Implement it!
        Ok(())
    }

    fn on_connecting(&self) -> Result<()> {
        // TODO Implement it!
        Ok(())
    }

    fn on_disconnected(&self) -> Result<()> {
        // TODO Implement it!
        Ok(())
    }

    fn set_send_websocket_message_callback(&self, _callback: SendWebsocketMessageCb) {}

    fn set_order_created_callback(&mut self, callback: OrderCreatedCb) {
        self.order_created_callback = callback;
    }

    fn set_order_cancelled_callback(&mut self, callback: OrderCancelledCb) {
        self.order_cancelled_callback = callback;
    }

    fn set_handle_order_filled_callback(&mut self, callback: HandleOrderFilledCb) {
        self.handle_order_filled_callback = callback;
    }

    fn set_handle_trade_callback(&mut self, callback: HandleTradeCb) {
        self.handle_trade_callback = callback;
    }

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>) {
        *self.traded_specific_currencies.lock() = currencies;
    }

    fn is_websocket_enabled(&self, _role: WebSocketRole) -> bool {
        !self.settings.api_key.is_empty() && !self.settings.secret_key.is_empty()
    }

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Url> {
        Url::parse(self.hosts.web_socket_host)
            .with_context(|| format!("Unable parse websocket {role:?} uri"))
    }

    fn get_specific_currency_pair(&self, currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        self.unified_to_specific.read()[&currency_pair]
    }

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode> {
        &self.supported_currencies
    }

    fn should_log_message(&self, message: &str) -> bool {
        let lowercase_message = message.to_lowercase();
        lowercase_message.contains("table")
            && (lowercase_message.contains(r#"\execution\"#)
                || lowercase_message.contains(r#"\order\"#))
    }

    fn get_settings(&self) -> &ExchangeSettings {
        &self.settings
    }
}
