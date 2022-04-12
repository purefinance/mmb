use crate::serum::Serum;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use serum_dex::matching::Side;
use solana_account_decoder::UiAccount;
use url::Url;

use mmb_core::connectivity::connectivity_manager::WebSocketRole;
use mmb_core::exchanges::common::{send_event, CurrencyPair, SortedOrderData};
use mmb_core::exchanges::common::{CurrencyCode, CurrencyId, SpecificCurrencyPair};
use mmb_core::exchanges::events::ExchangeEvent;
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
    Support,
};
use mmb_core::order_book::event::{EventType, OrderBookEvent};
use mmb_core::order_book::order_book_data::OrderBookData;
use mmb_core::orders::fill::EventSourceType;
use mmb_core::orders::order::{OrderInfo, OrderSide, OrderStatus};
use mmb_core::settings::ExchangeSettings;

use crate::solana_client::SolanaMessage;

#[async_trait]
impl Support for Serum {
    fn on_websocket_message(&self, msg: &str) -> Result<()> {
        match self.rpc_client.handle_on_message(msg) {
            SolanaMessage::AccountUpdated(currency_pair, side, ui_account) => {
                self.handle_account_market_changed(currency_pair, side, ui_account)
            }
            _ => Ok(()),
        }
    }

    fn on_connecting(&self) -> Result<()> {
        // Not needed for implementation Serum
        Ok(())
    }

    fn set_send_websocket_message_callback(&self, callback: SendWebsocketMessageCb) {
        self.rpc_client
            .set_send_websocket_message_callback(callback);
    }

    fn set_order_created_callback(&self, callback: OrderCreatedCb) {
        *self.order_created_callback.lock() = callback;
    }

    fn set_order_cancelled_callback(&self, callback: OrderCancelledCb) {
        *self.order_cancelled_callback.lock() = callback;
    }

    fn set_handle_order_filled_callback(&self, callback: HandleOrderFilledCb) {
        *self.handle_order_filled_callback.lock() = callback;
    }

    fn set_handle_trade_callback(&self, callback: HandleTradeCb) {
        *self.handle_trade_callback.lock() = callback;
    }

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>) {
        *self.traded_specific_currencies.lock() = currencies;
    }

    fn is_websocket_enabled(&self, role: WebSocketRole) -> bool {
        match role {
            WebSocketRole::Main => true,
            WebSocketRole::Secondary => false,
        }
    }

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Url> {
        let url = match role {
            WebSocketRole::Main => self.network_type.ws(),
            WebSocketRole::Secondary => unimplemented!("Not needed for implementation Serum"),
        };

        Url::parse(url).with_context(|| format!("Unable parse websocket {role:?} uri from {url}"))
    }

    fn get_specific_currency_pair(&self, currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        self.unified_to_specific.read()[&currency_pair]
    }

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode> {
        &self.supported_currencies
    }

    fn should_log_message(&self, message: &str) -> bool {
        message.contains("executionReport")
    }

    fn get_settings(&self) -> &ExchangeSettings {
        todo!()
    }
}

impl Serum {
    fn handle_account_market_changed(
        &self,
        currency_pair: CurrencyPair,
        side: Side,
        ui_account: UiAccount,
    ) -> Result<()> {
        let market = self.get_market_data(currency_pair)?;
        let market_info = &market.metadata;
        let orders =
            self.get_orders_from_ui_account(ui_account, market_info, side, currency_pair)?;

        self.handle_order_event(&orders, currency_pair);
        self.handle_order_book_snapshot(&orders, currency_pair)?;

        Ok(())
    }

    fn handle_order_book_snapshot(
        &self,
        orders: &[OrderInfo],
        currency_pair: CurrencyPair,
    ) -> Result<()> {
        let mut asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        for order in orders.iter() {
            match order.order_side {
                OrderSide::Buy => &mut bids,
                OrderSide::Sell => &mut asks,
            }
            .entry(order.price)
            .and_modify(|amount| *amount += order.amount)
            .or_insert(order.amount);
        }

        let order_book_event = OrderBookEvent::new(
            Utc::now(),
            self.id,
            currency_pair,
            "".to_string(),
            EventType::Snapshot,
            Arc::new(OrderBookData::new(asks, bids)),
        );

        let event = ExchangeEvent::OrderBookEvent(order_book_event);

        // TODO safe event in database if needed

        send_event(
            &self.events_channel,
            self.lifetime_manager.clone(),
            self.id,
            event,
        )
    }

    fn handle_order_event(&self, orders: &[OrderInfo], currency_pair: CurrencyPair) {
        let mut guard_lock = self.open_orders_by_owner.write();
        guard_lock
            .iter_mut()
            .filter(|(_, order_info)| order_info.currency_pair == currency_pair)
            .for_each(|(client_order_id, order_info)| match order_info.status {
                OrderStatus::Creating => {
                    if let Some(order) = orders
                        .iter()
                        .find(|order| order.client_order_id == *client_order_id)
                    {
                        order_info.status = OrderStatus::Created;
                        order_info.exchange_order_id = order.exchange_order_id.clone();
                        (&self.order_created_callback).lock()(
                            order.client_order_id.clone(),
                            order.exchange_order_id.clone(),
                            EventSourceType::WebSocket,
                        );
                    }
                }
                OrderStatus::Canceling => {
                    if !orders
                        .iter()
                        .any(|order| order.client_order_id == *client_order_id)
                    {
                        order_info.status = OrderStatus::Canceled;
                        (&self.order_cancelled_callback).lock()(
                            client_order_id.clone(),
                            order_info.exchange_order_id.clone(),
                            EventSourceType::WebSocket,
                        );
                    }
                }
                _ => {}
            });
        guard_lock.retain(|_, order_info| !order_info.status.is_finished());
    }
}
