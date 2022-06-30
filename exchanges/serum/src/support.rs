use crate::serum::{downcast_mut_to_serum_extension_data, Serum};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use serum_dex::matching::Side;
use serum_dex::state::EventView;
use solana_account_decoder::UiAccount;
use url::Url;

use crate::helpers::ToOrderSide;
use crate::market::MarketMetaData;
use mmb_core::connectivity::WebSocketRole;
use mmb_core::exchanges::common::{
    send_event, Amount, CurrencyCode, CurrencyId, CurrencyPair, Price, SortedOrderData,
    SpecificCurrencyPair,
};
use mmb_core::exchanges::events::{ExchangeEvent, TradeId};
use mmb_core::exchanges::general::commission::Percent;
use mmb_core::exchanges::general::handlers::handle_order_filled::{FillAmount, FillEvent};
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
    Support,
};
use mmb_core::misc::time::time_manager;
use mmb_core::order_book::event::{EventType, OrderBookEvent};
use mmb_core::order_book::order_book_data::OrderBookData;
use mmb_core::orders::fill::{EventSourceType, OrderFillType};
use mmb_core::orders::order::{
    ClientOrderId, ExchangeOrderId, OrderInfo, OrderRole, OrderSide, OrderStatus,
};
use mmb_core::settings::ExchangeSettings;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::{nothing_to_do, DateTime};

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
                self.handle_event_queue_orders(&events, currency_pair, market_info)?;
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

    fn handle_event_queue_orders(
        &self,
        events: &[EventView],
        currency_pair: CurrencyPair,
        market_metadata: &MarketMetaData,
    ) -> Result<()> {
        for event in events {
            if let EventView::Fill {
                side,
                maker,
                native_qty_paid,
                native_qty_received,
                native_fee_or_rebate,
                order_id,
                client_order_id: Some(client_order_id_value),
                ..
            } = *event
            {
                let (price, amount) = calc_order_fill_price_and_amount(
                    side,
                    maker,
                    native_qty_paid,
                    native_qty_received,
                    native_fee_or_rebate,
                    market_metadata,
                );
                let fill_data = OrderFillData {
                    // Serum doesn't store trade id so we have to create it by ourself
                    trade_id: TradeId::Number(self.generate_trade_id()),
                    client_order_id: client_order_id_value.to_string().as_str().into(),
                    exchange_order_id: order_id.to_string().as_str().into(),
                    price,
                    fill_amount: amount,
                    order_role: if maker {
                        OrderRole::Maker
                    } else {
                        OrderRole::Taker
                    },
                    commission: OrderTradeCommission {
                        currency_code: currency_pair.to_codes().quote,
                        amount: calc_order_fee(maker, native_fee_or_rebate, market_metadata),
                    },
                    fill_type: OrderFillType::UserTrade,
                    currency_pair,
                    order_side: side.to_order_side(),
                    date: time_manager::now(),
                };
                if self
                    .orders
                    .cache_by_client_id
                    .get(&fill_data.client_order_id)
                    .is_some()
                {
                    self.handle_order_fill(&fill_data);
                }
                self.handle_order_trade(&fill_data);
            }
            // There is no point to return error cause it's ordinary situation when we received no one fill event
        }

        Ok(())
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

    fn handle_order_fill(&self, fill_data: &OrderFillData) {
        (self.handle_order_filled_callback)(FillEvent {
            source_type: EventSourceType::Rpc,
            trade_id: Some(fill_data.trade_id.clone()),
            client_order_id: Some(fill_data.client_order_id.clone()),
            exchange_order_id: fill_data.exchange_order_id.clone(),
            fill_price: fill_data.price,
            fill_amount: FillAmount::Incremental {
                fill_amount: fill_data.fill_amount,
                total_filled_amount: None,
            },
            order_role: Some(fill_data.order_role),
            commission_currency_code: Some(fill_data.commission.currency_code),
            commission_rate: None,
            commission_amount: Some(fill_data.commission.amount),
            fill_type: fill_data.fill_type,
            trade_currency_pair: Some(fill_data.currency_pair),
            order_side: Some(fill_data.order_side),
            // TODO Add order amount value cause it can be new order
            order_amount: None,
            // There is no information about exact time of order fill so we use current utc time
            fill_date: Some(fill_data.date),
        });
    }

    fn handle_order_trade(&self, fill_data: &OrderFillData) {
        (self.handle_trade_callback)(
            fill_data.currency_pair,
            fill_data.trade_id.clone(),
            fill_data.price,
            fill_data.fill_amount,
            fill_data.order_side,
            fill_data.date,
        );
    }
}

struct OrderFillData {
    trade_id: TradeId,
    client_order_id: ClientOrderId,
    exchange_order_id: ExchangeOrderId,
    price: Price,
    fill_amount: Amount,
    order_role: OrderRole,
    commission: OrderTradeCommission,
    fill_type: OrderFillType,
    currency_pair: CurrencyPair,
    order_side: OrderSide,
    date: DateTime,
}

struct OrderTradeCommission {
    currency_code: CurrencyCode,
    amount: Amount,
}

fn calc_order_fill_price_and_amount(
    side: Side,
    maker: bool,
    native_qty_paid: u64,
    native_qty_received: u64,
    native_fee_or_rebate: u64,
    market_metadata: &MarketMetaData,
) -> (Price, Amount) {
    let signed_fee = if maker {
        -(native_fee_or_rebate as i64)
    } else {
        native_fee_or_rebate as i64
    };

    let (price_before_fees, quantity) = if side == Side::Bid {
        (native_qty_paid as i64 - signed_fee, native_qty_received)
    } else {
        (native_qty_received as i64 + signed_fee, native_qty_paid)
    };

    let price = Decimal::from(price_before_fees)
        * dec!(10).powi(market_metadata.coin_decimal as i64 - market_metadata.price_decimal as i64)
        / Decimal::from(quantity);
    let amount = Decimal::from(quantity) / dec!(10).powi(market_metadata.coin_decimal as i64);

    (price, amount)
}

fn calc_order_fee(
    maker: bool,
    native_fee_or_rebate: u64,
    market_meta_data: &MarketMetaData,
) -> Amount {
    let fee_rate = if maker { dec!(-1) } else { dec!(1) };

    (Decimal::from(native_fee_or_rebate) / dec!(10).powi(market_meta_data.price_decimal as i64))
        * fee_rate
}

// There is no public method for fee rate calculation so we use getFeeRates() from serum-js
// https://github.com/project-serum/serum-js/blob/312672d845f780d08ba827ace21555d571359d63/src/fees.ts#L8
// Function is never used cause we need either commission amount or rate and we use amount
// But it may be possible in future to use this code
#[allow(dead_code)]
fn calc_fee_rate(fee_tier: u8, maker: bool) -> Percent {
    Percent::try_from(
        (if maker {
            -0.0003
        } else {
            match fee_tier.into() {
                FeeTier::Srm2 => 0.002,
                FeeTier::Srm3 => 0.0018,
                FeeTier::Srm4 => 0.0016,
                FeeTier::Srm5 => 0.0014,
                FeeTier::Srm6 => 0.0012,
                FeeTier::Msrm => 0.001,
                // Note that there is one case for Base and Stable in JS code but in serum-dex Rust code they have different values
                _ => 0.0022,
            }
        }) * 100., // All numbers above are rates so we have to get percents
    )
    // We are sure that all numbers are correct so panic is impossible
    .expect("Unable to convert float number to Decimal")
}

// Copied from serum-dex source cause fees.rs is not public module and we get FeeTier enum from event data
// https://github.com/project-serum/serum-dex/blob/0c23a513403d20cc21e47f8ddde3eb90fbb302bb/dex/src/fees.rs#L32
enum FeeTier {
    Base,
    Srm2,
    Srm3,
    Srm4,
    Srm5,
    Srm6,
    Msrm,
    Stable,
}

impl From<u8> for FeeTier {
    fn from(number: u8) -> Self {
        use FeeTier::*;
        match number {
            1 => Srm2,
            2 => Srm3,
            3 => Srm4,
            4 => Srm5,
            5 => Srm6,
            6 => Msrm,
            7 => Stable,
            _ => Base,
        }
    }
}
