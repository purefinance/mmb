use mmb_domain::position::{ActivePosition, DerivativePosition};
use mmb_utils::infrastructure::{SpawnFutureFlags, WithExpect};
use std::any::Any;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use dashmap::DashMap;
use itertools::Itertools;
use mmb_domain::order::snapshot::{Amount, Price};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

use super::binance::Binance;
use mmb_core::connectivity::WebSocketRole;
use mmb_core::exchanges::common::send_event;
use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::traits::Support;
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
};
use mmb_core::infrastructure::spawn_by_timer;
use mmb_core::settings::ExchangeSettings;
use mmb_domain::events::{ExchangeEvent, Trade, TradeId};
use mmb_domain::market::{CurrencyCode, CurrencyPair};
use mmb_domain::market::{CurrencyId, SpecificCurrencyPair};
use mmb_domain::order::snapshot::SortedOrderData;
use mmb_domain::order::snapshot::*;
use mmb_domain::order_book::event::{EventType, OrderBookEvent};
use mmb_domain::order_book::order_book_data::OrderBookData;

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BinanceOrderInfo {
    #[serde(rename = "symbol")]
    pub specific_currency_pair: SpecificCurrencyPair,
    #[serde(rename = "orderId")]
    pub exchange_order_id: i64, //< local type is ExchangeOrderId
    #[serde(rename = "clientOrderId")]
    pub client_order_id: ClientOrderId,
    pub price: Price,
    #[serde(rename = "origQty")]
    pub orig_quantity: Amount,
    #[serde(rename = "executedQty")]
    pub executed_quantity: Amount,
    pub status: String,
    pub side: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BinanceAccountInfo {
    pub balances: Option<Vec<BinanceSpotBalances>>,
    pub assets: Option<Vec<BinanceMarginBalances>>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BinanceSpotBalances {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

// Corresponds https://binance-docs.github.io/apidocs/futures/en/#account-information-v2-user_data
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceMarginBalances {
    pub asset: String,                      // asset name
    pub wallet_balance: Decimal,            // wallet balance
    pub unrealized_profit: Decimal,         // unrealized profit
    pub margin_balance: Decimal,            // margin balance
    pub maint_margin: Decimal,              // maintenance margin required
    pub initial_margin: Decimal,            // total initial margin required with current mark price
    pub position_initial_margin: Decimal, //initial margin required for positions with current mark price
    pub open_order_initial_margin: Decimal, // initial margin required for open orders with current mark price
    pub cross_wallet_balance: Decimal,      // crossed wallet balance
    pub cross_un_pnl: Decimal,              // unrealized profit of crossed positions
    pub available_balance: Decimal,         // available balance
    pub max_withdraw_amount: Decimal,       // maximum amount for transfer out
    pub margin_available: bool, // whether the asset can be used as margin in Multi-Assets mode
    pub update_time: Decimal,   // last update time
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub(super) struct BinancePosition {
    #[serde(rename = "symbol")]
    pub specific_currency_pair: SpecificCurrencyPair,
    #[serde(rename = "PositionAmt")]
    pub position_amount: Amount,
    #[serde(rename = "LiquidationPrice")]
    pub liquidation_price: Price,
    pub leverage: Decimal,
    #[serde(rename = "PositionSide")]
    pub position_side: Decimal,
}

#[async_trait]
impl Support for Binance {
    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static) {
        self
    }

    async fn initialized(&self, exchange: Arc<Exchange>) {
        self.initialize_working_currencies(&exchange);

        start_updating_listen_key(&exchange);
    }

    fn on_websocket_message(&self, msg: &str) -> Result<()> {
        let mut data: Value =
            serde_json::from_str(msg).context("Unable to parse websocket message")?;
        // Public stream
        if let Some(stream) = data.get("stream") {
            let stream = stream
                .as_str()
                .ok_or_else(|| anyhow!("Unable to parse stream data"))?;

            if let Some(byte_index) = stream.find('@') {
                let currency_pair = self.currency_pair_from_web_socket(&stream[..byte_index])?;
                let data = &data["data"];

                if stream.ends_with("@trade") {
                    self.handle_trade(currency_pair, data)?;
                    return Ok(());
                }

                // TODO handle public stream
                let stream_tail = &stream[byte_index + 1..];
                if stream_tail.starts_with("depth1000") {
                    log::warn!("depth1000 is unsuported for Binance in current implementation");
                    return Ok(());
                }

                if stream_tail.starts_with("depth") {
                    self.process_snapshot_update(currency_pair, data)?;
                    return Ok(());
                }
            }

            return Ok(());
        }

        // so it is userData stream
        let event_type = data["e"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse event_type"))?;
        if event_type == "executionReport" {
            let event_time = Self::get_event_time(&data)?;
            self.handle_order_fill(msg, data, event_time)?;
        } else if event_type == "ORDER_TRADE_UPDATE" {
            let json_response = data["o"].take();
            let event_time = Self::get_event_time(&data)?;
            self.handle_order_fill(msg, json_response, event_time)?;
        } else {
            self.log_unknown_message(self.id, msg);
        }

        Ok(())
    }

    fn on_connecting(&self) -> Result<()> {
        self.unified_to_specific
            .read()
            .iter()
            .for_each(|(currency_pair, _)| {
                let _ = self
                    .last_trade_ids
                    .insert(*currency_pair, TradeId::Number(0));
            });

        Ok(())
    }

    fn on_connected(&self) -> Result<()> {
        Ok(())
    }

    fn on_disconnected(&self) -> Result<()> {
        *self.listen_key.write() = None;

        Ok(())
    }

    fn set_send_websocket_message_callback(&mut self, _callback: SendWebsocketMessageCb) {}

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
            WebSocketRole::Secondary => {
                !self.settings.api_key.is_empty() && !self.settings.secret_key.is_empty()
            }
        }
    }

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Url> {
        let (host, path) = match role {
            WebSocketRole::Main => (
                &self.hosts.web_socket_host,
                self.build_ws_main_path(&self.settings.websocket_channels[..]),
            ),
            WebSocketRole::Secondary => (
                &self.hosts.web_socket2_host,
                self.build_ws_secondary_path().await?,
            ),
        };

        Url::parse(&format!("{host}{path}"))
            .with_context(|| format!("Unable parse websocket {role:?} uri"))
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

    fn log_unknown_message(
        &self,
        exchange_account_id: mmb_domain::market::ExchangeAccountId,
        message: &str,
    ) {
        log::info!("Unknown message for {exchange_account_id}: {message}");
    }

    fn get_settings(&self) -> &ExchangeSettings {
        &self.settings
    }
}

impl Binance {
    pub(crate) fn handle_trade(&self, currency_pair: CurrencyPair, data: &Value) -> Result<()> {
        let trade_id = TradeId::from(data["t"].clone());

        let mut trade_id_from_lasts =
            self.last_trade_ids.get_mut(&currency_pair).with_expect(|| {
                format!("There are no last_trade_id for given currency_pair {currency_pair}")
            });

        if self.is_reducing_market_data && trade_id_from_lasts.get_number() >= trade_id.get_number()
        {
            log::info!("Current last_trade_id for currency_pair {currency_pair} is {} >= trade_id {trade_id}", *trade_id_from_lasts);

            return Ok(());
        }

        *trade_id_from_lasts = trade_id.clone();

        let price: Decimal = data["p"]
            .as_str()
            .context("Unable to get string from 'p' field json data")?
            .parse()?;

        let quantity: Decimal = data["q"]
            .as_str()
            .context("Unable to get string from 'q' field json data")?
            .parse()?;
        let order_side = if data["m"] == true {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        let datetime = data["T"]
            .as_i64()
            .context("Unable to get i64 from 'T' field json data")?;

        (self.handle_trade_callback)(
            currency_pair,
            Trade {
                trade_id,
                price,
                quantity,
                side: order_side,
                transaction_time: Utc.timestamp_millis(datetime),
            },
        );

        Ok(())
    }

    pub fn process_snapshot_update(&self, currency_pair: CurrencyPair, data: &Value) -> Result<()> {
        let (last_update_id, raw_asks, raw_bids) = match self.settings.is_margin_trading {
            true => {
                let last_update_id = data["u"].to_string();
                let raw_asks = data["a"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Unable to parse 'asks' in Binance"))?;
                let raw_bids = data["b"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Unable to parse 'bids' in Binance"))?;
                (last_update_id, raw_asks, raw_bids)
            }
            false => {
                let last_update_id = data["lastUpdateId"].to_string();
                let raw_asks = data["asks"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Unable to parse 'asks' in Binance"))?;
                let raw_bids = data["bids"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Unable to parse 'bids' in Binance"))?;
                (last_update_id, raw_asks, raw_bids)
            }
        };

        let asks = get_order_book_side(raw_asks)?;
        let bids = get_order_book_side(raw_bids)?;

        let order_book_data = OrderBookData::new(asks, bids);
        self.handle_order_book_snapshot(currency_pair, &last_update_id, order_book_data, None)
    }

    fn handle_order_book_snapshot(
        &self,
        currency_pair: CurrencyPair,
        event_id: &str,
        mut order_book_data: OrderBookData,
        order_book_update: Option<Vec<OrderBookData>>,
    ) -> Result<()> {
        if !self.subscribe_to_market_data {
            return Ok(());
        }

        //Some exchanges like Binance don't give us Snapshot in Web Socket, so we have to request Snapshot using Rest
        //and then update it with orderBookUpdates that we received while Rest request was being executed
        if let Some(updates) = order_book_update {
            order_book_data.update(updates)
        }

        let order_book_event = OrderBookEvent::new(
            Utc::now(),
            self.id,
            currency_pair,
            event_id.to_string(),
            EventType::Snapshot,
            Arc::new(order_book_data),
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

    fn currency_pair_from_web_socket(&self, currency_pair: &str) -> Result<CurrencyPair> {
        let specific_currency_pair = currency_pair.to_uppercase().as_str().into();
        self.get_unified_currency_pair(&specific_currency_pair)
    }

    fn build_ws_main_path(&self, websocket_channels: &[String]) -> String {
        let stream_names = self
            .traded_specific_currencies
            .lock()
            .iter()
            .flat_map(|currency_pair| {
                let mut results = Vec::new();
                for channel in websocket_channels {
                    let result = Self::get_stream_name(currency_pair, channel);
                    results.push(result);
                }
                results
            })
            .join("/");
        let ws_path = format!("/stream?streams={stream_names}");
        ws_path.to_lowercase()
    }

    async fn build_ws_secondary_path(&self) -> Result<String> {
        let listen_key = self.receive_listen_key().await;

        let ws_path = format!("/ws/{listen_key}");

        *self.listen_key.write() = Some(listen_key);

        Ok(ws_path)
    }

    pub(super) fn binance_position_to_active_position(
        &self,
        binance_position: BinancePosition,
    ) -> ActivePosition {
        let currency_pair = self
            .get_unified_currency_pair(&binance_position.specific_currency_pair)
            .with_expect(|| {
                let specific_currency_pair = binance_position.specific_currency_pair;
                format!("Failed to get_unified_currency_pair for {specific_currency_pair:?}")
            });

        let side = match binance_position.position_side > dec!(0) {
            true => OrderSide::Buy,
            false => OrderSide::Sell,
        };

        let derivative_position = DerivativePosition::new(
            currency_pair,
            binance_position.position_amount,
            Some(side),
            dec!(0),
            binance_position.liquidation_price,
            binance_position.leverage,
        );

        ActivePosition::new(derivative_position)
    }
}

fn start_updating_listen_key(exchange: &Arc<Exchange>) {
    let exchange_wk = Arc::downgrade(exchange);
    let period = Duration::from_secs(20 * 60);
    spawn_by_timer(
        "Update listen key",
        period,
        period,
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        move || {
            let exchange_wk = exchange_wk.clone();
            async move {
                let exchange = match exchange_wk.upgrade() {
                    None => return,
                    Some(v) => v,
                };

                exchange
                    .exchange_client
                    .as_any()
                    .downcast_ref::<Binance>()
                    .expect("received non Binance exchange client in method of updating listen keys by timer")
                    .ping_listen_key()
                    .await;
            }
        },
    );
}

fn get_order_book_side(levels: &[Value]) -> Result<SortedOrderData> {
    levels
        .iter()
        .map(|x| {
            let price = x[0]
                .as_str()
                .ok_or_else(|| anyhow!("Unable parse price of order book side in Binance"))?
                .parse()?;
            let amount = x[1]
                .as_str()
                .ok_or_else(|| anyhow!("Unable parse amount of order book side in Binance"))?
                .parse()?;
            Ok((price, amount))
        })
        .try_collect()
}
