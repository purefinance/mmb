use crate::event_listener_fields::EventListenerFields;
use crate::interactive_brokers::InteractiveBrokers;
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use function_name::named;
use futures::executor::block_on;
use mmb_core::connectivity::WebSocketRole;
use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
    Support,
};
use mmb_core::infrastructure::spawn_future_standalone;
use mmb_core::settings::ExchangeSettings;
use mmb_domain::market::{CurrencyCode, CurrencyId, CurrencyPair, SpecificCurrencyPair};
use mmb_utils::infrastructure::SpawnFutureFlags;
use std::any::Any;
use std::sync::Arc;
use url::Url;

#[async_trait]
impl Support for InteractiveBrokers {
    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static) {
        self
    }

    async fn initialized(&self, exchange: Arc<Exchange>) {
        self.set_symbols(exchange).await;

        self.get_client()
            .await
            .connect("127.0.0.1", 7497, 0)
            .expect("EClient connect error.");

        let EventListenerFields {
            client,
            channel_senders,
            handlers,
        } = self.take_event_listener_fields().await;

        spawn_future_standalone(
            "InteractiveBrokers::response_listener",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            Self::response_listener(client, channel_senders, handlers),
        );
    }

    fn on_websocket_message(&self, _msg: &str) -> Result<()> {
        Ok(())
    }

    fn on_connecting(&self) -> Result<()> {
        Ok(())
    }

    fn on_connected(&self) -> Result<()> {
        todo!()
    }

    fn on_disconnected(&self) -> Result<()> {
        Ok(())
    }

    fn set_send_websocket_message_callback(&mut self, _callback: SendWebsocketMessageCb) {
        todo!()
    }

    fn set_order_created_callback(&mut self, _callback: OrderCreatedCb) {
        todo!()
    }

    fn set_order_cancelled_callback(&mut self, _callback: OrderCancelledCb) {
        todo!()
    }

    #[named]
    fn set_handle_order_filled_callback(&mut self, callback: HandleOrderFilledCb) {
        let f_n = function_name!();

        let mut event_listener_fields = block_on(self.event_listener_fields.write());

        let event_listener_fields = event_listener_fields
            .as_mut()
            .unwrap_or_else(|| panic!("fn {f_n}: `event_listener_fields` is `None`."));

        event_listener_fields.handlers.order_filled_callback = callback;
    }

    fn set_handle_trade_callback(&mut self, _callback: HandleTradeCb) {
        todo!()
    }

    fn set_traded_specific_currencies(&self, _currencies: Vec<SpecificCurrencyPair>) {
        todo!()
    }

    fn is_websocket_enabled(&self, _role: WebSocketRole) -> bool {
        false
    }

    async fn create_ws_url(&self, _role: WebSocketRole) -> Result<Url> {
        todo!()
    }

    fn get_specific_currency_pair(&self, _currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        todo!()
    }

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode> {
        todo!()
    }

    fn should_log_message(&self, _message: &str) -> bool {
        todo!()
    }

    fn get_settings(&self) -> &ExchangeSettings {
        todo!()
    }
}
