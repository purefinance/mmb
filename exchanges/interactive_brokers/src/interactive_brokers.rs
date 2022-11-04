use crate::channels::channel_type::ChannelType;
use crate::channels::make_channels;
use crate::channels::receivers::ChannelReceivers;
use crate::channels::senders::ChannelSenders;
use crate::contract;
use crate::event_listener_fields::EventListenerFields;
use crate::handlers::Handlers;
use crate::mutexes::Mutexes;
use crate::order_side::OrderSide as IbOrderSide;
use crate::order_status::OrderStatus as IbOrderStatus;
use anyhow::{anyhow, Context};
use chrono::{NaiveDateTime, Utc};
use function_name::named;
use ibtwsapi::core::client::EClient;
use ibtwsapi::core::contract::Contract;
use ibtwsapi::core::errors::IBKRApiLibError;
use ibtwsapi::core::messages::ServerRspMsg;
use ibtwsapi::core::order::Order;
use ibtwsapi::examples::order_samples;
use mmb_core::exchanges::general::exchange::{Exchange, RequestResult};
use mmb_core::exchanges::general::handlers::handle_order_filled::{
    FillAmount, FillEvent, SpecialOrderData,
};
use mmb_core::exchanges::general::order::cancel::CancelOrderResult;
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::traits::ExchangeError;
use mmb_domain::events::{ExchangeBalance, TradeId};
use mmb_domain::exchanges::symbol::Symbol;
use mmb_domain::market::{CurrencyCode, CurrencyPair, ExchangeErrorType};
use mmb_domain::order::fill::{EventSourceType, OrderFillType};
use mmb_domain::order::snapshot::{
    ClientOrderId, ExchangeOrderId, OrderInfo, OrderRole, OrderSide as MmbOrderSide,
    OrderStatus as MmbOrderStatus,
};
use mmb_domain::position::{ActivePosition, ActivePositionId, DerivativePosition};
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, MutexGuard, RwLock};

pub struct InteractiveBrokers {
    // `Mutex` is required here, because `EClient::evt_chan` doesn't implement `Sync`
    client: Arc<Mutex<EClient>>,

    next_order_id: AtomicI32,

    // Interior mutability is required here, because method that uses this field, takes `&self`
    symbols: RwLock<HashMap<CurrencyPair, Arc<Symbol>>>,

    ch_rx: ChannelReceivers,

    req_id_seed: AtomicI32,

    pub mutexes: Mutexes,

    pub event_listener_fields: RwLock<Option<EventListenerFields>>,
}

impl InteractiveBrokers {
    pub fn new() -> Self {
        let client = Arc::new(Mutex::new(EClient::new()));
        let (channel_senders, ch_rx) = make_channels();

        let event_listener_fields = EventListenerFields {
            client: client.clone(),
            channel_senders,
            handlers: Handlers::empty(),
        };

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to calc duration since UNIX start time")
            .as_secs() as i32;

        InteractiveBrokers {
            client,
            next_order_id: AtomicI32::new(seed),
            symbols: RwLock::new(HashMap::new()),
            ch_rx,
            req_id_seed: AtomicI32::new(seed),
            mutexes: Mutexes::default(),
            event_listener_fields: RwLock::new(Some(event_listener_fields)),
        }
    }

    pub async fn set_symbols(&self, exchange: Arc<Exchange>) {
        let symbols = exchange
            .symbols
            .iter()
            .map(|v| {
                let (k, v) = v.pair();
                (*k, v.clone())
            })
            .collect();

        *self.symbols.write().await = symbols;
    }

    /// Can be called only once.
    #[named]
    pub async fn take_event_listener_fields(&self) -> EventListenerFields {
        let f_n = function_name!();

        self.event_listener_fields
            .write()
            .await
            .take()
            .unwrap_or_else(|| panic!("fn {f_n}: `event_listener_fields` is `None`."))
    }

    pub async fn response_listener(
        client: Arc<Mutex<EClient>>,
        channel_senders: ChannelSenders,
        handlers: Handlers,
    ) -> anyhow::Result<()> {
        loop {
            let msg = client.lock().await.get_event()?;

            if let Some(msg) = msg {
                channel_senders.send(msg.clone());

                Self::handle(&handlers, msg).await?;
            }
        }
    }

    async fn handle(handlers: &Handlers, msg: ServerRspMsg) -> anyhow::Result<()> {
        match &msg {
            ServerRspMsg::OpenOrder { order_state, .. } => {
                if IbOrderStatus::Filled == IbOrderStatus::from_str(&order_state.status)? {
                    let order_filled_callback = handlers.order_filled_callback.as_ref();

                    let fill_event = Self::parse_fill_event_from_open_order_msg(msg)?;

                    (order_filled_callback)(fill_event);
                } else {
                    // No need to handle. Ignore it.
                }
            }
            _ => {
                // No need to handle. Ignore it.
            }
        }

        Ok(())
    }

    pub async fn create_order_request(
        &self,
        currency_pair: &CurrencyPair,
        side: MmbOrderSide,
        price: Decimal,
        amount: Decimal,
    ) -> anyhow::Result<ExchangeOrderId> {
        let next_id = self.next_order_id();
        let contract = self
            .make_contract(currency_pair)
            .await
            .context("Make contract error.")?;
        let order = self
            .make_order(side, price, amount)
            .context("Make order error.")?;

        self.get_client()
            .await
            .place_order(next_id, &contract, &order)
            .context("Place order error.")?;

        Ok(next_id.into())
    }

    /// TODO: Maybe use here `TwsError`?
    #[named]
    pub async fn create_order_response(
        &self,
        exchange_order_id: ExchangeOrderId,
    ) -> anyhow::Result<ExchangeOrderId> {
        let f_n = function_name!();

        let expected_order_id: i32 = exchange_order_id
            .as_str()
            .parse()
            .expect("Error parsing client_order_id.");

        loop {
            let msg = self.ch_rx.recv(ChannelType::CreateOrder).await;

            break match &msg {
                ServerRspMsg::ErrMsg { req_id, .. } => {
                    if req_id == &expected_order_id {
                        // TODO: Maybe use here `TwsError`?
                        Err(anyhow!(msg))
                    } else {
                        // Message for someone else, but not for me. Ignore it.

                        continue;
                    }
                }
                ServerRspMsg::OpenOrder { order_id, .. } => {
                    if order_id == &expected_order_id {
                        Ok(order_id.into())
                    } else {
                        // Message for someone else, but not for me. Ignore it.

                        continue;
                    }
                }
                _ => unreachable!("fn {f_n}: received unsupported message: {:?}", msg),
            };
        }
    }

    pub async fn create_order_inner(
        &self,
        currency_pair: &CurrencyPair,
        side: MmbOrderSide,
        price: Decimal,
        amount: Decimal,
    ) -> anyhow::Result<ExchangeOrderId> {
        let exchange_order_id = self
            .create_order_request(currency_pair, side, price, amount)
            .await?;

        self.create_order_response(exchange_order_id).await
    }

    async fn cancel_order_request(&self, exchange_order_id: &ExchangeOrderId) -> CancelOrderResult {
        let order_id = exchange_order_id
            .as_str()
            .parse()
            .expect("Error parsing `exchange_order_id`.");

        match self.get_client().await.cancel_order(order_id) {
            Ok(_) => CancelOrderResult::succeed(order_id.into(), EventSourceType::Rest, None),
            Err(error) => CancelOrderResult::failed(Self::cast_error(error), EventSourceType::Rest),
        }
    }

    /// TODO: Maybe use here `TwsError`?
    #[named]
    async fn cancel_order_response(&self, client_order_id: &ClientOrderId) -> CancelOrderResult {
        let f_n = function_name!();

        let expected_order_id: i32 = client_order_id
            .as_str()
            .parse()
            .expect("Error parsing client_order_id.");

        loop {
            let msg = self.ch_rx.recv(ChannelType::CancelOrder).await;

            break match msg {
                ServerRspMsg::ErrMsg {
                    req_id,
                    error_code,
                    error_str,
                } => {
                    if req_id == expected_order_id {
                        // TODO: Maybe use here `TwsError`?
                        CancelOrderResult::failed(
                            ExchangeError::new(
                                ExchangeErrorType::Unknown,
                                error_str,
                                Some(error_code as i64),
                            ),
                            EventSourceType::Rest,
                        )
                    } else {
                        // Message for someone else, but not for me. Ignore it.

                        continue;
                    }
                }
                ServerRspMsg::OrderStatus {
                    order_id,
                    status,
                    filled,
                    ..
                } => {
                    let status =
                        IbOrderStatus::from_str(&status).expect("IbOrderStatus parse error.");
                    let is_expected_order_id = order_id == expected_order_id;
                    let is_expected_status = status == IbOrderStatus::Cancelled;

                    if is_expected_order_id && is_expected_status {
                        let filled = Decimal::from_f64_retain(filled).ok_or_else(|| {
                            format!(
                                "fn {f_n}: order filled amount: Decimal::from_f64_retain error.",
                            )
                        });

                        match filled {
                            Ok(filled) => CancelOrderResult::succeed(
                                order_id.into(),
                                EventSourceType::Rest,
                                Some(filled),
                            ),
                            Err(e) => CancelOrderResult::failed(
                                ExchangeError::parsing(e),
                                EventSourceType::Rest,
                            ),
                        }
                    } else {
                        // Not desired `status` or not desired `order_id`. Ignore it.

                        continue;
                    }
                }
                _ => unreachable!("fn {f_n}: received unsupported message: {:?}", msg),
            };
        }
    }

    pub async fn cancel_order_inner(&self, order_id: &str) -> CancelOrderResult {
        let request_result = self
            .cancel_order_request(&ExchangeOrderId::from(order_id))
            .await;

        match request_result.outcome {
            RequestResult::Success(_) => {
                self.cancel_order_response(&ClientOrderId::from(order_id))
                    .await
            }
            RequestResult::Error(_) => request_result,
        }
    }

    pub async fn get_open_orders_request(&self) -> anyhow::Result<()> {
        self.get_client().await.req_all_open_orders()?;

        Ok(())
    }

    #[named]
    pub async fn get_open_orders_response(&self) -> anyhow::Result<Vec<OrderInfo>> {
        let f_n = function_name!();

        let mut orders = Vec::new();

        loop {
            let msg = self.ch_rx.recv(ChannelType::GetOpenOrders).await;

            match &msg {
                ServerRspMsg::ErrMsg { .. } => continue,
                ServerRspMsg::OpenOrder { .. } => {
                    let order = Self::parse_order_info_from_open_order_msg(msg)?;

                    orders.push(order);
                }
                ServerRspMsg::OpenOrderEnd => break,
                _ => unreachable!("fn {f_n}: received unsupported message: {:?}", msg),
            };
        }

        Ok(orders)
    }

    pub async fn get_open_orders_inner(&self) -> anyhow::Result<Vec<OrderInfo>> {
        // There we need `Mutex` that locks the entire function,
        // because methods that return `Vec` cannot be called simultaneously
        let _guard = self.mutexes.get_open_orders.lock().await;

        self.get_open_orders_request().await?;

        self.get_open_orders_response().await
    }

    pub async fn get_my_trades_request(&self) -> Result<(), IBKRApiLibError> {
        self.get_client().await.req_completed_orders(false)?;

        Ok(())
    }

    #[named]
    pub async fn get_my_trades_response(
        &self,
        symbol: &Symbol,
        min_datetime: Option<DateTime>,
    ) -> anyhow::Result<Vec<OrderTrade>> {
        let f_n = function_name!();

        let mut trades = Vec::new();

        loop {
            let msg = self.ch_rx.recv(ChannelType::GetMyTrades).await;

            match &msg {
                ServerRspMsg::ErrMsg { .. } => continue,
                ServerRspMsg::CompletedOrder { .. } => {
                    if Self::check_if_completed_order_fits(&msg, symbol, min_datetime)? {
                        let trade = Self::parse_order_trade_from_completed_order_msg(msg)?;

                        trades.push(trade);
                    }
                }
                ServerRspMsg::CompletedOrdersEnd => break,
                _ => unreachable!("fn {f_n}: received unsupported message: {:?}", msg),
            };
        }

        Ok(trades)
    }

    pub async fn get_balance_request(&self) -> anyhow::Result<()> {
        let req_id = self.req_id_seed.fetch_add(1, Ordering::Relaxed);

        self.get_client()
            .await
            .req_account_summary(req_id, "All", "AllTags")?;

        Ok(())
    }

    #[named]
    pub async fn get_balance_response(&self) -> anyhow::Result<Vec<ExchangeBalance>> {
        let f_n = function_name!();

        let mut balances = Vec::new();

        loop {
            let msg = self.ch_rx.recv(ChannelType::GetBalance).await;

            match &msg {
                ServerRspMsg::ErrMsg { .. } => continue,
                ServerRspMsg::AccountSummary { .. } => {
                    let balance = Self::parse_balance_from_account_summary_msg(msg)?;

                    balances.push(balance);
                }
                ServerRspMsg::AccountSummaryEnd { .. } => break,
                _ => unreachable!("fn {f_n}: received unsupported message: {:?}", msg),
            };
        }

        Ok(balances)
    }

    pub async fn get_balance_inner(&self) -> anyhow::Result<Vec<ExchangeBalance>> {
        // There we need `Mutex` that locks the entire function,
        // because methods that return `Vec` cannot be called simultaneously
        let _guard = self.mutexes.get_balance.lock().await;

        self.get_balance_request().await?;

        self.get_balance_response().await
    }

    pub async fn get_positions_request(&self) -> anyhow::Result<()> {
        self.get_client().await.req_positions()?;

        Ok(())
    }

    #[named]
    pub async fn get_positions_response(&self) -> anyhow::Result<Vec<ActivePosition>> {
        let f_n = function_name!();

        let mut positions = Vec::new();

        loop {
            let msg = self.ch_rx.recv(ChannelType::GetPositions).await;

            match &msg {
                ServerRspMsg::ErrMsg { .. } => continue,
                ServerRspMsg::PositionData { .. } => {
                    let position = Self::parse_position_from_position_data_msg(msg)?;

                    positions.push(position);
                }
                ServerRspMsg::PositionEnd => break,
                _ => unreachable!("fn {f_n}: received unsupported message: {:?}", msg),
            };
        }

        Ok(positions)
    }

    pub async fn get_positions_inner(&self) -> anyhow::Result<Vec<ActivePosition>> {
        // There we need `Mutex` that locks the entire function,
        // because methods that return `Vec` cannot be called simultaneously
        let _guard = self.mutexes.get_positions.lock().await;

        self.get_positions_request().await?;

        self.get_positions_response().await
    }

    pub async fn get_client(&self) -> MutexGuard<EClient> {
        self.client.lock().await
    }

    #[named]
    fn check_if_completed_order_fits(
        msg: &ServerRspMsg,
        symbol: &Symbol,
        min_datetime: Option<DateTime>,
    ) -> anyhow::Result<bool> {
        let f_n = function_name!();

        if let ServerRspMsg::OpenOrder {
            contract,
            order_state,
            ..
        } = msg
        {
            let datetime_fits = {
                let order_datetime = Self::parse_datetime(&order_state.completed_time)?;

                min_datetime
                    .map(|min_datetime| order_datetime >= min_datetime)
                    .unwrap_or(true)
            };

            let symbol_fits = {
                let order_symbol = &contract.symbol;
                let order_currency = &contract.currency;
                let required_symbol = symbol.base_currency_code.as_str();
                let required_currency = symbol.quote_currency_code.as_str();

                order_symbol == required_symbol && order_currency == required_currency
            };

            Ok(datetime_fits && symbol_fits)
        } else {
            unreachable!("fn {f_n}: received unsupported message: {:?}", msg);
        }
    }

    #[named]
    fn parse_order_info_from_open_order_msg(msg: ServerRspMsg) -> anyhow::Result<OrderInfo> {
        let f_n = function_name!();

        if let ServerRspMsg::OpenOrder {
            order_id,
            contract,
            order,
            order_state,
        } = msg
        {
            let order_side: MmbOrderSide = IbOrderSide::from_str(&order.action)
                .map_err(|e| anyhow!(e))?
                .try_into()
                .map_err(|e: String| anyhow!(e))?;
            let base = CurrencyCode::from(contract.symbol.as_str());
            let quote = CurrencyCode::from(contract.currency.as_str());
            let order_status: MmbOrderStatus = IbOrderStatus::from_str(&order_state.status)
                .map_err(|_| anyhow!("fn {f_n}: Parse order status error."))?
                .try_into()
                .map_err(|e: String| anyhow!(e))?;
            // TODO: Check if right value used
            let price = Decimal::from_f64_retain(order.lmt_price).context(anyhow!(
                "fn {f_n}: Order price: Decimal::from_f64_retain error.",
            ))?;
            let amount = Decimal::from_f64_retain(order.total_quantity).context(anyhow!(
                "fn {f_n}: Order amount: Decimal::from_f64_retain error.",
            ))?;
            let filled_amount = Decimal::from_f64_retain(order.filled_quantity).context(
                anyhow!("fn {f_n}: Order filled amount: Decimal::from_f64_retain error."),
            )?;
            let commission_amount = Decimal::from_f64_retain(order_state.commission).context(
                anyhow!("fn {f_n}: Order commission amount: Decimal::from_f64_retain error."),
            )?;

            Ok(OrderInfo::new(
                CurrencyPair::from_codes(base, quote),
                order_id.into(),
                order_id.into(),
                order_side,
                order_status,
                price,
                amount,
                price,
                filled_amount,
                Some(order_state.commission_currency),
                None,
                Some(commission_amount),
            ))
        } else {
            unreachable!("fn {f_n}: received unsupported message: {:?}", msg);
        }
    }

    /// TODO: Figure out where we can get `OrderTrade::trade_id`
    /// TODO: Figure out where we can get `OrderTrade::order_role`
    /// TODO: Check if `OrderTrade::fill_type` is right
    #[named]
    fn parse_order_trade_from_completed_order_msg(msg: ServerRspMsg) -> anyhow::Result<OrderTrade> {
        let f_n = function_name!();

        if let ServerRspMsg::CompletedOrder {
            order, order_state, ..
        } = msg
        {
            let order_id = order.order_id;
            // TODO: Check if right value used
            let price = Decimal::from_f64_retain(order.lmt_price).context(anyhow!(
                "fn {f_n}: Order price: Decimal::from_f64_retain error.",
            ))?;
            let amount = Decimal::from_f64_retain(order.total_quantity).context(anyhow!(
                "fn {f_n}: Order amount: Decimal::from_f64_retain error.",
            ))?;
            let commission_amount = Decimal::from_f64_retain(order_state.commission).context(
                anyhow!("fn {f_n}: Order commission amount: Decimal::from_f64_retain error."),
            )?;

            Ok(OrderTrade::new(
                order_id.into(),
                TradeId::String(order_id.to_string().into_boxed_str()),
                Self::parse_datetime(&order_state.completed_time)?,
                price,
                amount,
                OrderRole::Maker,
                CurrencyCode::from(order_state.commission_currency.as_str()),
                None,
                Some(commission_amount),
                OrderFillType::UserTrade,
            ))
        } else {
            unreachable!("fn {f_n}: received unsupported message: {:?}", msg);
        }
    }

    #[named]
    fn parse_balance_from_account_summary_msg(
        msg: ServerRspMsg,
    ) -> anyhow::Result<ExchangeBalance> {
        let f_n = function_name!();

        if let ServerRspMsg::AccountSummary {
            value, currency, ..
        } = msg
        {
            let balance = Decimal::from_str(&value).with_context(|| {
                format!("fn {f_n}: account balance: Decimal::from_f64_retain error.")
            })?;

            Ok(ExchangeBalance {
                currency_code: CurrencyCode::from(currency.as_str()),
                balance,
            })
        } else {
            unreachable!("fn {f_n}: received unsupported message: {:?}", msg);
        }
    }

    /// TODO: Implement
    /// TODO: Check if `average_entry_price`, `liquidation_price` and `leverage` is right
    /// TODO: Check if `ActivePositionId` (`ActivePosition::id`) is right
    #[named]
    fn parse_position_from_position_data_msg(msg: ServerRspMsg) -> anyhow::Result<ActivePosition> {
        let f_n = function_name!();

        if let ServerRspMsg::PositionData {
            contract,
            position,
            avg_cost,
            ..
        } = msg
        {
            let position = Decimal::from_f64_retain(position)
                .with_context(|| format!("fn {f_n}: position: Decimal::from_f64_retain error."))?;
            let avg_cost = Decimal::from_f64_retain(avg_cost).with_context(|| {
                format!("fn {f_n}: Position avg_cost: Decimal::from_f64_retain error.")
            })?;
            let leverage = Decimal::from_f64_retain(1.0).with_context(|| {
                format!("fn {f_n}: Position leverage: Decimal::from_f64_retain error.")
            })?;
            let base = CurrencyCode::from(contract.symbol.as_str());
            let quote = CurrencyCode::from(contract.currency.as_str());

            let derivative = DerivativePosition {
                currency_pair: CurrencyPair::from_codes(base, quote),
                position,
                average_entry_price: avg_cost,
                liquidation_price: avg_cost,
                leverage,
            };

            // We don't receive `timestamp` from exchange
            let mut active_position = ActivePosition::new(derivative, Utc::now());
            // TODO: Check if it is right
            active_position.id = ActivePositionId::from(contract.con_id.to_string().as_str());

            Ok(active_position)
        } else {
            unreachable!("fn {f_n}: received unsupported message: {:?}", msg);
        }
    }

    /// TODO: Fill `FillEvent::trade_id` from `ServerRspMsg::ExecutionData::execution::exec_id`
    /// TODO: Fill `FillEvent::fill_date` from `ServerRspMsg::ExecutionData::execution::time`
    /// TODO: Figure out where we can get `FillEvent::order_role`
    /// TODO: Check if `FillEvent::fill_type` is right
    #[named]
    fn parse_fill_event_from_open_order_msg(msg: ServerRspMsg) -> anyhow::Result<FillEvent> {
        let f_n = function_name!();

        if let ServerRspMsg::OpenOrder {
            order_id,
            contract,
            order,
            order_state,
        } = msg
        {
            let fill_price = Decimal::from_f64_retain(order.lmt_price).with_context(|| {
                format!("fn {f_n}: order fill price: Decimal::from_f64_retain error.",)
            })?;
            let total_quantity =
                Decimal::from_f64_retain(order.total_quantity).with_context(|| {
                    format!("fn {f_n}: order total filled amount: Decimal::from_f64_retain error.",)
                })?;

            let fill_amount = {
                let filled_quantity = Decimal::from_f64_retain(order.filled_quantity)
                    .with_context(|| {
                        format!("fn {f_n}: order filled amount: Decimal::from_f64_retain error.",)
                    })?;

                FillAmount::Incremental {
                    fill_amount: filled_quantity,
                    total_filled_amount: Some(total_quantity),
                }
            };

            let commission_currency_code =
                CurrencyCode::from(order_state.commission_currency.as_str());

            let commission_amount =
                Decimal::from_f64_retain(order_state.commission).with_context(|| {
                    format!("fn {f_n}: order commission amount: Decimal::from_f64_retain error.",)
                })?;

            let special_order_data = {
                let base = CurrencyCode::from(contract.symbol.as_str());
                let quote = CurrencyCode::from(contract.currency.as_str());
                let order_side: MmbOrderSide = IbOrderSide::from_str(&order.action)
                    .map_err(|e| anyhow!(e))?
                    .try_into()
                    .map_err(|e: String| anyhow!(e))?;

                SpecialOrderData {
                    currency_pair: CurrencyPair::from_codes(base, quote),
                    order_side,
                    order_amount: total_quantity,
                }
            };

            Ok(FillEvent {
                source_type: EventSourceType::Rest,
                trade_id: None,
                client_order_id: Some(order_id.into()),
                exchange_order_id: order_id.into(),
                fill_price,
                fill_amount,
                order_role: None,
                commission_currency_code: Some(commission_currency_code),
                commission_rate: None,
                commission_amount: Some(commission_amount),
                fill_type: OrderFillType::UserTrade,
                special_order_data: Some(special_order_data),
                fill_date: None,
            })
        } else {
            unreachable!("fn {f_n}: received unsupported message: {:?}", msg);
        }
    }

    /// TODO: Check if `DateTime` parsing is right
    fn parse_datetime(datetime: &str) -> anyhow::Result<DateTime> {
        // Format here: `20220919-16:13:16 GET`

        // let datetime = ChronoDateTime::parse_from_str(&datetime, "%Y%m%d-%H:%M:%S")?;
        let datetime = NaiveDateTime::parse_from_str(datetime, "%Y%m%d-%H:%M:%S")?;

        // TODO: Check if it is right
        Ok(DateTime::from_local(datetime, Utc))
    }

    fn next_order_id(&self) -> i32 {
        let order_id = self.next_order_id.load(Ordering::SeqCst);

        self.next_order_id.fetch_add(1, Ordering::SeqCst);

        order_id
    }

    #[named]
    async fn make_contract(&self, currency_pair: &CurrencyPair) -> anyhow::Result<Contract> {
        let f_n = function_name!();

        let symbols = self.symbols.read().await;

        let symbol = symbols
            .get(currency_pair)
            .ok_or_else(|| anyhow!("fn {f_n}: Error: currency pair not found: {currency_pair}."))?;

        Ok(contract::usstock(symbol))
    }

    #[named]
    fn make_order(
        &self,
        side: MmbOrderSide,
        price: Decimal,
        amount: Decimal,
    ) -> anyhow::Result<Order> {
        let f_n = function_name!();

        let side: IbOrderSide = side.into();

        let price = price
            .try_into()
            .context(anyhow!("fn {f_n}: Error converting order `price` to f64."))?;
        let amount = amount
            .try_into()
            .context(anyhow!("fn {f_n}: Error converting order `amount` to f64."))?;

        Ok(order_samples::limit_order(&side.to_string(), amount, price))
    }

    pub fn cast_error(error: IBKRApiLibError) -> ExchangeError {
        let error_msg = error.to_string();

        let error_type = match error {
            IBKRApiLibError::Io(_) => ExchangeErrorType::SendError,
            IBKRApiLibError::ParseFloat(_) => ExchangeErrorType::ParsingError,
            IBKRApiLibError::ParseInt(_) => ExchangeErrorType::ParsingError,
            IBKRApiLibError::RecvError(_) => ExchangeErrorType::SendError,
            IBKRApiLibError::TryRecvError(_) => ExchangeErrorType::SendError,
            IBKRApiLibError::RecvTimeoutError(_) => ExchangeErrorType::SendError,
            IBKRApiLibError::ApiError(_) => ExchangeErrorType::SendError,
        };

        ExchangeError::new(error_type, error_msg, None)
    }
}
