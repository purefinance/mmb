use crate::bitmex::Bitmex;
use crate::types::{
    BitmexOrderBookDelete, BitmexOrderBookInsert, BitmexOrderBookUpdate, BitmexOrderFillDummy,
    BitmexOrderFillTrade, BitmexOrderStatus, BitmexTradePayload,
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bstr::ByteSlice;
use chrono::Utc;
use dashmap::DashMap;
use mmb_core::connectivity::WebSocketRole;
use mmb_core::exchanges::common::send_event;
use mmb_core::exchanges::general::handlers::handle_order_filled::{
    FillAmount, FillEvent, SpecialOrderData,
};
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
    Support,
};
use mmb_core::settings::ExchangeSettings;
use mmb_domain::events::{ExchangeEvent, Trade};
use mmb_domain::market::{CurrencyCode, CurrencyId, CurrencyPair, SpecificCurrencyPair};
use mmb_domain::order::fill::{EventSourceType, OrderFillType};
use mmb_domain::order::snapshot::{Amount, OrderSide, Price};
use mmb_domain::order_book::event::{EventType, OrderBookEvent};
use mmb_domain::order_book::order_book_data::OrderBookData;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::sync::Arc;
use url::Url;

#[async_trait]
impl Support for Bitmex {
    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static) {
        self
    }

    fn on_websocket_message(&self, msg: &str) -> Result<()> {
        let message: WebsocketMessage = serde_json::from_str(msg)
            .with_context(|| format!("Unable to parse websocket message:\n{}", msg))?;

        match message {
            WebsocketMessage::SubscriptionResult(subscription_result) => {
                self.handle_subscription_result(subscription_result)?
            }
            WebsocketMessage::Payload(payload_data) => self.handle_websocket_data(payload_data)?,
            WebsocketMessage::Info(info) => {
                log::info!("{info:?}")
            }
            WebsocketMessage::Unknown(_) => {
                let error = format!("Unsupported Bitmex websocket message: {msg}");
                log::error!("{error}");
                panic!("{error}");
            }
        }

        Ok(())
    }

    fn on_connecting(&self) -> Result<()> {
        Ok(())
    }

    fn on_connected(&self) -> Result<()> {
        // First of all we should auth to be able to subscribe to private messages
        let expire_time = Bitmex::get_key_expire_time(60);
        let signature =
            Bitmex::create_signature(&self.settings.secret_key, "GET/realtime", expire_time)
                .to_str()
                .expect("Failed to convert signature to string")
                .to_owned();

        #[derive(Serialize)]
        #[serde(untagged)]
        enum ArgVariant {
            String(String),
            Number(u64),
        }
        let request = Request {
            operation: SubscriptionOperationType::AuthKeyExpires,
            args: vec![
                ArgVariant::String(self.settings.api_key.clone()),
                ArgVariant::Number(expire_time),
                ArgVariant::String(signature),
            ],
        };
        let private_auth = serde_json::to_string(&request)
            .expect("Failed to serialize Bitmex private auth message");

        (self.websocket_message_callback)(WebSocketRole::Main, private_auth)
    }

    fn on_disconnected(&self) -> Result<()> {
        Ok(())
    }

    fn set_send_websocket_message_callback(&mut self, callback: SendWebsocketMessageCb) {
        self.websocket_message_callback = callback;
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
            WebSocketRole::Main => {
                !self.settings.api_key.is_empty() && !self.settings.secret_key.is_empty()
            }
            WebSocketRole::Secondary => false,
        }
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

impl Bitmex {
    fn handle_subscription_result(&self, subscription_result: SubscriptionResult) -> Result<()> {
        match subscription_result.success {
            true => {
                log::info!("Bitmex websocket: successful subscription: {subscription_result:?}");
                if subscription_result
                    .request
                    .to_string()
                    .contains(SubscriptionOperationType::AuthKeyExpires.as_str())
                {
                    self.on_auth_success()?;
                }

                Ok(())
            }
            false => {
                let err = format!(
                    "Bitmex websocket: failed subscription: {:?}",
                    subscription_result.request
                );
                log::error!("{err}");
                bail!(err)
            }
        }
    }

    fn handle_websocket_data(&self, payload: BitmexPayloadData) -> Result<()> {
        match payload {
            BitmexPayloadData::OrderBookL2(data) | BitmexPayloadData::OrderBookL2_25(data) => {
                self.handle_order_book_data(data)?
            }
            BitmexPayloadData::Trade { action, data } => self.handle_trade(action, data)?,
            BitmexPayloadData::Execution { action, data } => self.handle_execution(action, data)?,
        }

        Ok(())
    }

    fn handle_order_book_data(&self, order_book_data: BitmexOrderBookPayload) -> Result<()> {
        match order_book_data {
            BitmexOrderBookPayload::Update { data } => {
                if !data.is_empty() {
                    // All symbols in event's array are the same
                    let symbol = data[0].symbol;
                    let order_book_dictionary = self.order_book_ids.lock();
                    let mut order_book_data = OrderBookData::default();
                    for record in data {
                        let price = order_book_dictionary
                            .get(&(record.symbol, record.id))
                            .with_context(|| {
                                format!(
                                    "Cannot find order id {} for symbol {}",
                                    record.id, record.symbol
                                )
                            })?;

                        Self::add_order_book_info(
                            *price,
                            record.size,
                            record.side,
                            &mut order_book_data,
                        );
                    }
                    self.send_order_book_event(symbol, order_book_data, EventType::Update)?;
                }
            }
            BitmexOrderBookPayload::Delete { data } => {
                if !data.is_empty() {
                    // All symbols in event's array are the same
                    let symbol = data[0].symbol;
                    let mut order_book_dictionary = self.order_book_ids.lock();
                    let mut order_book_data = OrderBookData::default();
                    for record in data {
                        let price = order_book_dictionary
                            .get(&(record.symbol, record.id))
                            .with_context(|| {
                                format!(
                                    "Cannot find order id {} for symbol {}",
                                    record.id, record.symbol
                                )
                            })?;

                        Self::add_order_book_info(
                            *price,
                            dec!(0),
                            record.side,
                            &mut order_book_data,
                        );

                        order_book_dictionary.remove(&(record.symbol, record.id));
                    }

                    self.send_order_book_event(symbol, order_book_data, EventType::Update)?;
                }
            }
            BitmexOrderBookPayload::Insert { data } => {
                if !self.is_order_book_initialized() {
                    // We should skip all data before partial action received
                    return Ok(());
                }

                if !data.is_empty() {
                    // All symbols in event's array are the same
                    let symbol = data[0].symbol;
                    let mut order_book_dictionary = self.order_book_ids.lock();
                    let mut order_book_data = OrderBookData::default();
                    for record in data {
                        Self::add_order_book_info(
                            record.price,
                            record.size,
                            record.side,
                            &mut order_book_data,
                        );
                        order_book_dictionary.insert((record.symbol, record.id), record.price);
                    }

                    self.send_order_book_event(symbol, order_book_data, EventType::Update)?;
                }
            }
            BitmexOrderBookPayload::Partial { data } => {
                if !data.is_empty() {
                    // All symbols in event's array are the same
                    let symbol = data[0].symbol;
                    let mut order_book_dictionary = self.order_book_ids.lock();
                    let mut order_book_data = OrderBookData::default();
                    for record in data {
                        Self::add_order_book_info(
                            record.price,
                            record.size,
                            record.side,
                            &mut order_book_data,
                        );
                        order_book_dictionary.insert((record.symbol, record.id), record.price);
                    }

                    self.send_order_book_event(symbol, order_book_data, EventType::Snapshot)?;
                }
            }
        }

        Ok(())
    }

    fn is_order_book_initialized(&self) -> bool {
        !self.order_book_ids.lock().is_empty()
    }

    fn send_order_book_event(
        &self,
        specific_currency_pair: SpecificCurrencyPair,
        order_book: OrderBookData,
        update_type: EventType,
    ) -> Result<()> {
        let currency_pair = self.get_unified_currency_pair(&specific_currency_pair)?;
        let order_book_event = OrderBookEvent::new(
            Utc::now(),
            self.settings.exchange_account_id,
            currency_pair,
            String::default(),
            update_type,
            Arc::new(order_book),
        );

        send_event(
            &self.events_channel,
            self.lifetime_manager.clone(),
            self.settings.exchange_account_id,
            ExchangeEvent::OrderBookEvent(order_book_event),
        )
    }

    fn add_order_book_info(
        price: Price,
        amount: Amount,
        side: OrderSide,
        order_book_map: &mut OrderBookData,
    ) {
        match side {
            OrderSide::Sell => {
                order_book_map.asks.insert(price, amount);
            }
            OrderSide::Buy => {
                order_book_map.bids.insert(price, amount);
            }
        }
    }

    fn handle_trade(
        &self,
        action: SubscriptionDataAction,
        trade_data: Vec<BitmexTradePayload>,
    ) -> Result<()> {
        if action != SubscriptionDataAction::Partial && action != SubscriptionDataAction::Insert {
            bail!("Unsupported trade action: {action}")
        }

        for record in trade_data {
            (self.handle_trade_callback)(
                self.get_unified_currency_pair(&record.symbol)?,
                Trade {
                    trade_id: record.trade_id.into(),
                    price: record.price,
                    quantity: record.size,
                    side: record.side,
                    transaction_time: record.timestamp,
                },
            );
        }

        Ok(())
    }

    fn handle_execution(
        &self,
        action: SubscriptionDataAction,
        execution_data: Vec<BitmexOrderExecutionPayload>,
    ) -> Result<()> {
        if action == SubscriptionDataAction::Partial {
            // We're not interested in execution snapshot
            return Ok(());
        }

        for execution in execution_data {
            match execution {
                BitmexOrderExecutionPayload::New(data) => {
                    // No need to handle order as created when close position received
                    // Order may have several instructions separated by spaces
                    if !data.instruction.contains("Close") {
                        (self.order_created_callback)(
                            data.client_order_id,
                            data.exchange_order_id,
                            EventSourceType::WebSocket,
                        );
                    }
                }
                BitmexOrderExecutionPayload::Canceled(data) => (self.order_cancelled_callback)(
                    data.client_order_id,
                    data.exchange_order_id,
                    EventSourceType::WebSocket,
                ),
                BitmexOrderExecutionPayload::Rejected(_) => (), // Nothing to do cause it's been already handled during create_order() response handling
                BitmexOrderExecutionPayload::Filled(variant)
                | BitmexOrderExecutionPayload::PartiallyFilled(variant) => match variant {
                    BitmexOrderFill::Trade(data) => {
                        let order_data = SpecialOrderData {
                            currency_pair: self.get_unified_currency_pair(&data.symbol)?,
                            order_side: data.side,
                            order_amount: data.amount,
                        };
                        let client_order_id = if !data.client_order_id.is_empty() {
                            Some(data.client_order_id)
                        } else {
                            None
                        };

                        let fill_event = FillEvent {
                            source_type: EventSourceType::WebSocket,
                            trade_id: Some(data.trade_id.into()),
                            client_order_id,
                            exchange_order_id: data.exchange_order_id,
                            fill_price: data.fill_price,
                            fill_amount: FillAmount::Incremental {
                                fill_amount: data.fill_amount,
                                total_filled_amount: Some(data.total_filled_amount),
                            },
                            order_role: Some(Bitmex::get_order_role_by_commission_amount(
                                data.commission_amount,
                            )),
                            commission_currency_code: Some(data.currency.into()),
                            commission_rate: Some(data.commission_rate),
                            commission_amount: Some(data.commission_amount),
                            fill_type: Self::get_order_fill_type(&data.details)?,
                            special_order_data: Some(order_data),
                            fill_date: Some(data.timestamp),
                        };

                        (self.handle_order_filled_callback)(fill_event);
                    }
                    BitmexOrderFill::Funding(_) => (),
                },
            }
        }

        Ok(())
    }

    pub(crate) fn get_order_fill_type(text: &str) -> Result<OrderFillType> {
        if text == "Liquidation" {
            Ok(OrderFillType::Liquidation)
        } else if text == "Funding" {
            Ok(OrderFillType::Funding)
        } else if text.contains("Submitted via API") || text.contains("Submission from") {
            Ok(OrderFillType::UserTrade)
        } else if text.contains("Position Close") {
            Ok(OrderFillType::ClosePosition)
        } else {
            bail!("Unknown order fill type {text}")
        }
    }

    fn on_auth_success(&self) -> Result<()> {
        let traded_currencies = self.traded_specific_currencies.lock();
        let subscriptions = Self::subscribe_to_websocket_events(
            // Note that OrderBookL2_25 get only top 25 levels
            vec![
                SubscriptionType::OrderBookL2_25,
                SubscriptionType::Trade,
                SubscriptionType::Execution,
            ],
            traded_currencies.deref(),
        );

        (self.websocket_message_callback)(WebSocketRole::Main, subscriptions)
    }

    fn subscribe_to_websocket_events(
        subscriptions: Vec<SubscriptionType>,
        currency_pairs: &[SpecificCurrencyPair],
    ) -> String {
        let mut request = Request {
            operation: SubscriptionOperationType::Subscribe,
            args: Vec::with_capacity(subscriptions.len() * currency_pairs.len()),
        };
        for subscription in subscriptions {
            for currency_pair in currency_pairs {
                request
                    .args
                    .push(format!("{}:{currency_pair}", subscription.as_str()));
            }
        }

        serde_json::to_string(&request).expect("Failed to serialize subscription message")
    }
}

#[derive(Deserialize, Debug)]
#[serde(bound(deserialize = "'de: 'a"))]
#[serde(untagged)]
enum WebsocketMessage<'a> {
    SubscriptionResult(SubscriptionResult),
    Payload(BitmexPayloadData<'a>),
    Info(InformationMessage<'a>),
    Unknown(Value),
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct InformationMessage<'a> {
    info: &'a str,
    version: &'a str,
    timestamp: &'a str,
}

#[derive(Deserialize, Debug)]
struct SubscriptionResult {
    success: bool,
    request: Value,
}

#[derive(Serialize)]
struct Request<T> {
    #[serde(rename = "op")]
    operation: SubscriptionOperationType,
    args: Vec<T>,
}

// Enum is for all possible subscription types but just some of them are used for now
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum SubscriptionType {
    // Public types
    Announcement,        // Site announcements
    Chat,                // Trollbox chat
    Connected,           // Statistics of connected users/bots
    Funding,     // Updates of swap funding rates. Sent every funding interval (usually 8hrs)
    Instrument,  // Instrument updates including turnover and bid/ask
    Insurance,   // Daily Insurance Fund updates
    Liquidation, // Liquidation orders as they're entered into the book
    OrderBookL2, // Full level 2 orderBook
    OrderBookL2_25, // 25 top levels of full level 2 orderBook
    OrderBook10, // Top 10 levels using traditional full book push
    PublicNotifications, // System-wide notifications (used for short-lived messages)
    Quote,       // Top level of the book
    QuoteBin1m,  // 1-minute quote bins
    QuoteBin5m,  // 5-minute quote bins
    QuoteBin1h,  // 1-hour quote bins
    QuoteBin1d,  // 1-day quote bins
    Settlement,  // Settlements
    Trade,       // Live trades
    TradeBin1m,  // 1-minute trade bins
    TradeBin5m,  // 5-minute trade bins
    TradeBin1h,  // 1-hour trade bins
    TradeBin1d,  // 1-day trade bins
    // Private types
    Affiliate,            // Affiliate status, such as total referred users & payout %
    Execution,            // Individual executions; can be multiple per order
    Order,                // Live updates on your orders
    Margin,               // Updates on your current account balance and margin requirements
    Position,             // Updates on your positions
    PrivateNotifications, // Individual notifications - currently not used
    Transact,             // Deposit/Withdrawal updates
    Wallet,               // Bitcoin address balance data, including total deposits & withdrawals
}

impl SubscriptionType {
    fn as_str(&self) -> &str {
        match self {
            Self::Announcement => "announcement",
            Self::Chat => "chat",
            Self::Connected => "connected",
            Self::Funding => "funding",
            Self::Instrument => "instrument",
            Self::Insurance => "insurance",
            Self::Liquidation => "liquidation",
            Self::OrderBookL2 => "orderBookL2",
            Self::OrderBookL2_25 => "orderBookL2_25",
            Self::OrderBook10 => "orderBook10",
            Self::PublicNotifications => "publicNotifications",
            Self::Quote => "quote",
            Self::QuoteBin1m => "quoteBin1m",
            Self::QuoteBin5m => "quoteBin5m",
            Self::QuoteBin1h => "quoteBin1h",
            Self::QuoteBin1d => "quoteBin1d",
            Self::Settlement => "settlement",
            Self::Trade => "trade",
            Self::TradeBin1m => "tradeBin1m",
            Self::TradeBin5m => "tradeBin5m",
            Self::TradeBin1h => "tradeBin1h",
            Self::TradeBin1d => "tradeBin1d",
            Self::Affiliate => "affiliate",
            Self::Execution => "execution",
            Self::Order => "order",
            Self::Margin => "margin",
            Self::Position => "position",
            Self::PrivateNotifications => "privateNotifications",
            Self::Transact => "transact",
            Self::Wallet => "wallet",
        }
    }
}

impl Display for SubscriptionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Debug for SubscriptionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// Unsubscribe is not used for now
#[allow(dead_code)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum SubscriptionOperationType {
    Subscribe,
    Unsubscribe,
    AuthKeyExpires,
}

impl SubscriptionOperationType {
    fn as_str(&self) -> &str {
        match self {
            Self::Subscribe => "subscribe",
            Self::Unsubscribe => "unsubscribe",
            Self::AuthKeyExpires => "authKeyExpires",
        }
    }
}

// Possible types of received data
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum SubscriptionDataAction {
    Partial, // Data snapshot, received just after subscription
    Insert,  // New data record
    Update,  // Update active data. For example an order
    Delete,  // Delete active data
}

impl SubscriptionDataAction {
    fn as_str(&self) -> &str {
        match self {
            Self::Partial => "partial",
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

impl Display for SubscriptionDataAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Debug for SubscriptionDataAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Deserialize, Debug)]
#[serde(bound(deserialize = "'de: 'a"))]
#[serde(rename_all = "camelCase")]
#[serde(tag = "table")]
enum BitmexPayloadData<'a> {
    OrderBookL2_25(BitmexOrderBookPayload),
    OrderBookL2(BitmexOrderBookPayload),
    Trade {
        action: SubscriptionDataAction,
        data: Vec<BitmexTradePayload>,
    },
    Execution {
        action: SubscriptionDataAction,
        data: Vec<BitmexOrderExecutionPayload<'a>>,
    },
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "action")]
enum BitmexOrderBookPayload {
    Update { data: Vec<BitmexOrderBookUpdate> },
    Insert { data: Vec<BitmexOrderBookInsert> },
    Delete { data: Vec<BitmexOrderBookDelete> },
    Partial { data: Vec<BitmexOrderBookInsert> },
}

#[derive(Deserialize, Debug)]
#[serde(bound(deserialize = "'de: 'a"))]
#[serde(tag = "ordStatus")]
enum BitmexOrderExecutionPayload<'a> {
    New(BitmexOrderStatus<'a>),
    Filled(BitmexOrderFill<'a>),
    PartiallyFilled(BitmexOrderFill<'a>),
    Canceled(BitmexOrderStatus<'a>),
    Rejected(BitmexOrderStatus<'a>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize, Debug)]
#[serde(bound(deserialize = "'de: 'a"))]
#[serde(tag = "execType")]
pub(crate) enum BitmexOrderFill<'a> {
    Trade(BitmexOrderFillTrade<'a>),
    Funding(BitmexOrderFillDummy),
}
