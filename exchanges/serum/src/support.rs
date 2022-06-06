use crate::serum::{downcast_mut_to_serum_extension_data, Serum};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use rust_decimal::Decimal;
use serum_dex::matching::Side;
use serum_dex::state::EventView;
use solana_account_decoder::UiAccount;
use url::Url;

use crate::helpers::ToOrderSide;
use mmb_core::connectivity::WebSocketRole;
use mmb_core::exchanges::common::{
    send_event, Amount, CurrencyCode, CurrencyId, CurrencyPair, Price, SortedOrderData,
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
use mmb_core::orders::order::{ClientOrderId, OrderInfo, OrderRole, OrderSide, OrderStatus};
use mmb_core::settings::ExchangeSettings;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::nothing_to_do;

use crate::solana_client::{SolanaMessage, SubscriptionAccountType};

#[async_trait]
impl Support for Serum {
    fn on_websocket_message(&self, msg: &str) -> Result<()> {
        match self.rpc_client.handle_on_message(msg) {
            SolanaMessage::AccountUpdated(currency_pair, side, ui_account, account_type) => {
                self.handle_account_market_changed(currency_pair, side, ui_account, account_type)
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
        account_type: SubscriptionAccountType,
    ) -> Result<()> {
        let market = self.get_market_data(currency_pair)?;
        let market_info = &market.metadata;

        match account_type {
            SubscriptionAccountType::OrderBook => {
                let orders =
                    self.get_orders_from_order_book(ui_account, market_info, side, currency_pair)?;
                self.handle_order_book_snapshot(&orders, currency_pair)?;
                self.handle_order_event(&orders, currency_pair);
            }
            SubscriptionAccountType::EventQueue => {
                let events = self.get_event_queue_data(ui_account, market_info)?;
                self.handle_order_fill_event(&events);
            }
            SubscriptionAccountType::OpenOrders => {
                let _orders =
                    self.get_orders_from_open_orders_account(ui_account, &market, currency_pair)?;
            }
        }

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

    fn handle_order_fill_event(&self, events: &[EventView]) {
        let events: DashMap<ClientOrderId, &EventView> = events
            .iter()
            .map(|event| {
                let client_order_id_option = match event {
                    &EventView::Fill {
                        client_order_id, ..
                    }
                    | &EventView::Out {
                        client_order_id, ..
                    } => client_order_id,
                };

                if let Some(client_order_id) = client_order_id_option {
                    (client_order_id.to_string().as_str().into(), event)
                } else {
                    ("0".into(), event)
                }
            })
            .collect();

        self.orders.cache_by_client_id.iter().for_each(|order_ref| {
            let (client_order_id, order_status) = order_ref
                .fn_ref(|order| (order.header.client_order_id.clone(), order.props.status));
            if OrderStatus::Creating == order_status || OrderStatus::Created == order_status {
                if let Some(event) = events.get(&client_order_id) {
                    self.handle_order_fill(event.value(), &client_order_id);
                    // TODO Handle trades
                }
            }
        });
    }

    fn handle_order_event(&self, orders: &[OrderInfo], currency_pair: CurrencyPair) {
        let orders: DashMap<ClientOrderId, &OrderInfo> = orders
            .iter()
            .map(|order| (order.client_order_id.clone(), order))
            .collect();

        self.orders
            .cache_by_client_id
            .iter()
            .filter(|order| order.currency_pair() == currency_pair)
            .for_each(|order_ref| {
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
                        _ => nothing_to_do(),
                    }
                });
            });
    }

    fn handle_order_fill(&self, event: &EventView, client_order_id: &ClientOrderId) {
        if let EventView::Fill {
            side,
            maker,
            native_qty_received,
            order_id,
            ..
        } = event
        {
            (self.handle_order_filled_callback)(FillEvent {
                source_type: EventSourceType::Rpc,
                trade_id: None,
                client_order_id: Some(client_order_id.clone()),
                exchange_order_id: order_id.to_string().as_str().into(),
                fill_price: Price::into(Decimal::from(*native_qty_received)),
                fill_amount: FillAmount::Incremental {
                    fill_amount: Amount::into(Decimal::from(*native_qty_received)),
                    total_filled_amount: None,
                },
                order_role: Some(if *maker {
                    OrderRole::Maker
                } else {
                    OrderRole::Taker
                }),
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::UserTrade,
                trade_currency_pair: None,
                order_side: Some(side.to_order_side()),
                order_amount: None,
                // There is no information about exact time of order fill so we use current utc time
                fill_date: Some(Utc::now()),
            });
        }
    }
}
