use crate::serum::{downcast_mut_to_serum_extension_data, Serum};
use std::cmp::Ordering;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use serum_dex::matching::Side;
use solana_account_decoder::UiAccount;
use url::Url;

use mmb_core::connectivity::WebSocketRole;
use mmb_core::exchanges::common::{
    send_event, Amount, CurrencyCode, CurrencyId, CurrencyPair, SortedOrderData,
    SpecificCurrencyPair,
};
use mmb_core::exchanges::events::ExchangeEvent;
use mmb_core::exchanges::general::handlers::handle_order_filled::{FillAmount, FillEvent};
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
    Support,
};
use mmb_core::order_book::event::{EventType, OrderBookEvent};
use mmb_core::order_book::order_book_data::OrderBookData;
use mmb_core::orders::fill::{EventSourceType, OrderFillType};
use mmb_core::orders::order::{
    ClientOrderId, OrderInfo, OrderRole, OrderSide, OrderSnapshot, OrderStatus,
};
use mmb_core::settings::ExchangeSettings;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::nothing_to_do;

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
        struct OrderFillData<'order>(Option<(&'order OrderInfo, Amount)>);
        impl<'order> OrderFillData<'order> {
            pub fn need_to_fill(
                order_from_event: &'order OrderInfo,
                local_order: &OrderSnapshot,
            ) -> Self {
                match local_order.amount().cmp(&order_from_event.amount) {
                    Ordering::Greater => OrderFillData(Some((
                        order_from_event,
                        local_order.amount() - order_from_event.amount,
                    ))),
                    Ordering::Equal => OrderFillData(None),
                    Ordering::Less => {
                        log::error!(
                            "Filled order amount {} more than created order amount {}",
                            order_from_event.amount,
                            local_order.amount()
                        );
                        OrderFillData(None)
                    }
                }
            }
        }

        let orders: DashMap<ClientOrderId, &OrderInfo> = orders
            .iter()
            .map(|order| (order.client_order_id.clone(), order))
            .collect();

        self.orders
            .cache_by_client_id
            .iter()
            .filter(|order| order.currency_pair() == currency_pair)
            .for_each(|order_ref| {
                let mut order_fill_data = OrderFillData(None);
                order_ref.fn_mut(|order| {
                    let client_order_id = &order.header.client_order_id;
                    match order.props.status {
                        OrderStatus::Creating => {
                            let serum_extension_data =
                                downcast_mut_to_serum_extension_data(order.extension_data.as_deref_mut());

                            if let Some(order_from_event) = orders.get(client_order_id) {
                                if OrderStatus::Created != serum_extension_data.actual_status {
                                    (self.order_created_callback)(
                                        client_order_id.clone(),
                                        order_from_event.exchange_order_id.clone(),
                                        EventSourceType::Rpc,
                                    );

                                    serum_extension_data.actual_status = OrderStatus::Created;
                                }

                                order_fill_data = OrderFillData::need_to_fill(order_from_event.value(), order);
                            }
                        }
                        OrderStatus::Canceling => {
                            let serum_extension_data =
                                downcast_mut_to_serum_extension_data(order.extension_data.as_deref_mut());

                            if OrderStatus::Canceled != serum_extension_data.actual_status && !orders.contains_key(client_order_id) {
                                let exchange_order_id = order.props.exchange_order_id.as_ref().with_expect(|| {
                                    format!(
                                        "Failed to get exchange order id for order {client_order_id}"
                                    )
                                });
                                (self.order_cancelled_callback)(
                                    client_order_id.clone(),
                                    exchange_order_id.clone(),
                                    EventSourceType::Rpc,
                                );

                                serum_extension_data.actual_status = OrderStatus::Canceled;
                            }
                        }
                        OrderStatus::Created => {
                            if let Some(order_from_event) = orders.get(client_order_id) {
                                order_fill_data = OrderFillData::need_to_fill(order_from_event.value(), order);
                            }
                        }
                        _ => nothing_to_do(),
                    }
                });
                if let Some((order_to_fill, total_filled_amount)) = order_fill_data.0 {
                    self.handle_order_fill(order_to_fill, total_filled_amount);
                }
            });
    }

    fn handle_order_fill(&self, order_from_event: &OrderInfo, total_filled_amount: Amount) {
        (self.handle_order_filled_callback)(FillEvent {
            source_type: EventSourceType::Rpc,
            trade_id: None,
            client_order_id: Some(order_from_event.client_order_id.clone()),
            exchange_order_id: order_from_event.exchange_order_id.clone(),
            fill_price: order_from_event.price,
            fill_amount: FillAmount::Total {
                total_filled_amount,
            },
            // TODO Need to find out is order maker or taker. Now it's impossible because order book accounts data has no info about it
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::UserTrade,
            trade_currency_pair: None,
            order_side: Some(order_from_event.order_side),
            order_amount: None,
            // There is no information about exact time of order fill so we use current utc time
            fill_date: Some(Utc::now()),
        });
    }
}
