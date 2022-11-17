use crate::exchanges::general::handlers::should_ignore_event;
use crate::{exchanges::general::exchange::Exchange, math::ConvertPercentToRate};
use chrono::Utc;
use function_name::named;
use mmb_domain::events::{
    AllowedEventSourceType, EventSourceType, MetricsEventInfoBase, MetricsEventType, TradeId,
};
use mmb_domain::exchanges::commission::Percent;
use mmb_domain::exchanges::symbol::{Round, Symbol};
use mmb_domain::market::{CurrencyCode, CurrencyPair, ExchangeAccountId};
use mmb_domain::order::event::OrderEventType;
use mmb_domain::order::fill::{OrderFill, OrderFillType};
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::{Amount, OrderOptions, Price};
use mmb_domain::order::snapshot::{ClientOrderFillId, OrderRole};
use mmb_domain::order::snapshot::{
    ClientOrderId, ExchangeOrderId, OrderSide, OrderSnapshot, OrderStatus,
};
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use uuid::Uuid;

type ArgsToLog = (
    ExchangeAccountId,
    Option<TradeId>,
    Option<ClientOrderId>,
    ExchangeOrderId,
    AllowedEventSourceType,
    EventSourceType,
);

#[derive(Debug, Clone, Copy)]
pub enum FillAmount {
    Incremental {
        // Volume of order fill for current event
        fill_amount: Amount,
        // Summary volume of all executed order fills
        total_filled_amount: Option<Amount>,
    },
    Total {
        // Summary volume of all executed order fills
        total_filled_amount: Amount,
    },
}

impl FillAmount {
    pub fn total_filled_amount(&self) -> Option<Amount> {
        match self {
            FillAmount::Incremental {
                total_filled_amount: amount,
                ..
            } => *amount,
            FillAmount::Total {
                total_filled_amount,
            } => Some(*total_filled_amount),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpecialOrderData {
    // For ClosePosition order currency pair can be empty string
    pub currency_pair: CurrencyPair,
    pub order_side: OrderSide,
    pub order_amount: Amount,
}

#[derive(Debug, Clone)]
pub struct FillEvent {
    pub source_type: EventSourceType,
    pub trade_id: Option<TradeId>,
    pub client_order_id: Option<ClientOrderId>,
    pub exchange_order_id: ExchangeOrderId,
    pub fill_price: Price,
    pub fill_amount: FillAmount,
    pub order_role: Option<OrderRole>,
    pub commission_currency_code: Option<CurrencyCode>,
    pub commission_rate: Option<Percent>,
    pub commission_amount: Option<Amount>,
    pub fill_type: OrderFillType,
    pub special_order_data: Option<SpecialOrderData>,
    pub fill_date: Option<DateTime>,
}

impl Exchange {
    #[named]
    pub fn handle_order_filled(&self, fill_event: &mut FillEvent) {
        log::trace!(concat!("started ", function_name!(), " {:?}"), fill_event);

        let args_to_log = (
            self.exchange_account_id,
            fill_event.trade_id.clone(),
            fill_event.client_order_id.clone(),
            fill_event.exchange_order_id.clone(),
            self.features.allowed_fill_event_source_type,
            fill_event.source_type,
        );

        if should_ignore_event(
            self.features.allowed_fill_event_source_type,
            fill_event.source_type,
        ) {
            log::info!("Ignoring fill {args_to_log:?}");
            return;
        }

        if fill_event.exchange_order_id.is_empty() {
            panic!("Received HandleOrderFilled with an empty exchangeOrderId {args_to_log:?}",);
        }

        self.add_special_order_if_need(fill_event, &args_to_log);

        match self
            .orders
            .cache_by_exchange_id
            .get(&fill_event.exchange_order_id)
        {
            None => {
                log::info!("Received a fill for not existing order {args_to_log:?}",);

                self.buffered_fills_manager
                    .lock()
                    .add_fill(self.exchange_account_id, fill_event);

                if let Some(client_order_id) = &fill_event.client_order_id {
                    self.raise_order_created(
                        client_order_id,
                        &fill_event.exchange_order_id,
                        fill_event.source_type,
                    );
                }
            }
            Some(order_ref) => self.create_and_add_order_fill(fill_event, &order_ref),
        }
    }

    fn was_trade_already_received(
        trade_id: &Option<TradeId>,
        order_fills: &[OrderFill],
        order_ref: &OrderRef,
    ) -> bool {
        let current_trade_id = match trade_id {
            None => return false,
            Some(trade_id) => trade_id,
        };

        if order_fills.iter().any(|fill| {
            fill.trade_id()
                .map(|fill_trade_id| fill_trade_id == current_trade_id)
                .unwrap_or(false)
        }) {
            log::info!(
                "Trade with {current_trade_id} was received already for order {order_ref:?}"
            );

            return true;
        }

        false
    }

    fn diff_fill_after_non_diff(
        fill_event: &FillEvent,
        order_fills: &[OrderFill],
        order_ref: &OrderRef,
    ) -> bool {
        if matches!(fill_event.fill_amount, FillAmount::Incremental { .. })
            && order_fills.iter().any(|fill| !fill.is_incremental_fill())
        {
            // Most likely we received a trade update (diff), then received a non-diff fill via fallback and then again received a diff trade update
            // It happens when WebSocket is glitchy and we miss update and the problem is we have no idea how to handle diff updates
            // after applying a non-diff one as there's no TradeId, so we have to ignore all the diff updates afterwards
            // relying only on fallbacks
            log::warn!("Unable to process a diff fill after a non-diff one {order_ref:?}");

            return true;
        }

        false
    }

    fn filled_amount_not_less_event_fill(
        fill_event: &FillEvent,
        order_filled_amount: Amount,
        order_ref: &OrderRef,
    ) -> bool {
        match fill_event.fill_amount {
            FillAmount::Total {
                total_filled_amount,
            } if order_filled_amount >= total_filled_amount => {
                log::warn!(
                    "order.filled_amount is {order_filled_amount} >= received fill {total_filled_amount}, so non-diff fill for {} {:?} should be ignored",
                    order_ref.client_order_id(),
                    order_ref.exchange_order_id(),
                );

                true
            }
            _ => false,
        }
    }

    fn should_miss_fill(
        fill_event: &FillEvent,
        order_filled_amount: Amount,
        last_fill_amount: Amount,
        order_ref: &OrderRef,
    ) -> bool {
        if let Some(total_filled_amount) = fill_event.fill_amount.total_filled_amount() {
            if order_filled_amount + last_fill_amount != total_filled_amount {
                log::warn!("Fill was missed because {order_filled_amount} != {total_filled_amount} for {order_ref:?}");
                return true;
            }
        }

        false
    }

    fn get_last_fill_data(
        fill_event: &mut FillEvent,
        symbol: &Symbol,
        order_fills: &[OrderFill],
        order_filled_amount: Amount,
        order_ref: &OrderRef,
    ) -> Option<(Price, Amount, Decimal)> {
        fn calc_last_fill(
            fill_event: &FillEvent,
            last_fill_amount: Amount,
            symbol: &Symbol,
        ) -> (Price, Amount, Decimal) {
            let last_fill_price = fill_event.fill_price;
            let last_fill_cost = if !symbol.is_derivative() {
                last_fill_amount * last_fill_price
            } else {
                last_fill_amount / last_fill_price
            };
            (last_fill_price, last_fill_amount, last_fill_cost)
        }

        let (last_fill_price, last_fill_amount, last_fill_cost) = match fill_event.fill_amount {
            FillAmount::Total {
                total_filled_amount,
            } if !order_fills.is_empty() => {
                let (_, last_fill_amount, last_fill_cost) =
                    calc_last_fill(fill_event, total_filled_amount, symbol);

                let cost_diff = Self::calculate_cost_diff(order_fills, order_ref, last_fill_cost)?;
                let (price, amount, cost) = Self::calculate_last_fill_data(
                    last_fill_amount,
                    order_filled_amount,
                    symbol,
                    cost_diff,
                );

                Self::set_commission_amount(fill_event, order_fills);

                (price, amount, cost)
            }
            FillAmount::Total {
                total_filled_amount: fill_amount,
            }
            | FillAmount::Incremental { fill_amount, .. } => {
                calc_last_fill(fill_event, fill_amount, symbol)
            }
        };

        if last_fill_amount.is_zero() {
            log::warn!(
                "last_fill_amount was received for 0 for {}, {:?}",
                order_ref.client_order_id(),
                order_ref.exchange_order_id()
            );

            return None;
        }

        Some((last_fill_price, last_fill_amount, last_fill_cost))
    }

    fn calculate_cost_diff(
        order_fills: &[OrderFill],
        order_ref: &OrderRef,
        last_fill_cost: Decimal,
    ) -> Option<Decimal> {
        // Diff should be calculated only if it is not the first fill
        let total_filled_cost: Decimal = order_fills.iter().map(|fill| fill.cost()).sum();
        let cost_diff = last_fill_cost - total_filled_cost;
        if cost_diff <= dec!(0) {
            log::warn!("cost_diff is {cost_diff} which is <= 0 for {order_ref:?}");
            return None;
        }

        Some(cost_diff)
    }

    fn calculate_last_fill_data(
        last_fill_amount: Amount,
        order_filled_amount: Amount,
        symbol: &Symbol,
        cost_diff: Price,
    ) -> (Price, Amount, Price) {
        let amount_diff = last_fill_amount - order_filled_amount;
        let res_fill_price = if !symbol.is_derivative() {
            cost_diff / amount_diff
        } else {
            amount_diff / cost_diff
        };
        let last_fill_price = symbol.price_round(res_fill_price, Round::ToNearest);

        let last_fill_amount = amount_diff;
        let last_fill_cost = cost_diff;

        (last_fill_price, last_fill_amount, last_fill_cost)
    }

    fn set_commission_amount(fill_event: &mut FillEvent, order_fills: &[OrderFill]) {
        if let Some(commission_amount) = fill_event.commission_amount {
            let current_commission: Decimal = order_fills
                .iter()
                .map(|fill| fill.commission_amount())
                .sum();
            fill_event.commission_amount = Some(commission_amount - current_commission);
        }
    }

    fn panic_if_wrong_status_or_cancelled(order_ref: &OrderRef, fill_event: &FillEvent) -> bool {
        let (status, was_cancellation_event_raised) =
            order_ref.fn_ref(|o| (o.status(), o.internal_props.was_cancellation_event_raised));

        if matches!(status, OrderStatus::FailedToCreate | OrderStatus::Completed) {
            panic!(
                "Fill was received for a {status:?} {was_cancellation_event_raised} {fill_event:?}"
            );
        }

        if was_cancellation_event_raised {
            log::warn!(
                "Fill was received for a {status:?} {was_cancellation_event_raised} {fill_event:?}"
            );
        }
        was_cancellation_event_raised
    }

    fn get_order_role(fill_event: &FillEvent, order_ref: &OrderRef) -> OrderRole {
        match fill_event.order_role {
            Some(order_role) => order_role,
            None => {
                if fill_event.commission_amount.is_none()
                    && fill_event.commission_rate.is_none()
                    && order_ref.role().is_none()
                {
                    panic!("Fill has neither commission nor commission rate");
                }

                order_ref.role().expect("Unable to determine order_role")
            }
        }
    }

    fn get_commission_amount(
        fill_event_commission_amount: Option<Amount>,
        fill_event_commission_rate: Option<Decimal>,
        expected_commission_rate: Percent,
        last_fill_amount: Amount,
        last_fill_price: Price,
        commission_currency_code: CurrencyCode,
        symbol: &Symbol,
    ) -> Amount {
        match fill_event_commission_amount {
            Some(commission_amount) => commission_amount,
            None => {
                let commission_rate = match fill_event_commission_rate {
                    Some(commission_rate) => commission_rate,
                    None => expected_commission_rate,
                };

                let last_fill_amount_in_currency_code = symbol
                    .convert_amount_from_amount_currency_code(
                        commission_currency_code,
                        last_fill_amount,
                        last_fill_price,
                    );
                last_fill_amount_in_currency_code * commission_rate
            }
        }
    }

    fn set_commission_rate(&self, fill_event: &mut FillEvent, order_role: OrderRole) -> Decimal {
        let commission = self.commission.get_commission(order_role).fee;
        let expected_commission_rate = commission.percent_to_rate();

        if fill_event.commission_amount.is_none() && fill_event.commission_rate.is_none() {
            fill_event.commission_rate = Some(expected_commission_rate);
        }

        expected_commission_rate
    }

    fn update_commission_for_bnb_case(
        &self,
        commission_currency_code: CurrencyCode,
        symbol: &Symbol,
        commission_amount: Amount,
        converted_commission_amount: &mut Amount,
        converted_commission_currency_code: &mut CurrencyCode,
    ) {
        if commission_currency_code != symbol.base_currency_code()
            && commission_currency_code != symbol.quote_currency_code()
        {
            let mut currency_pair =
                CurrencyPair::from_codes(commission_currency_code, symbol.quote_currency_code());
            match self.order_book_top.get(&currency_pair) {
                Some(top_prices) => {
                    let bid = top_prices
                        .bid
                        .as_ref()
                        .expect("There are no top bid in order book");
                    let price_bnb_quote = bid.price;
                    *converted_commission_amount = commission_amount * price_bnb_quote;
                    *converted_commission_currency_code = symbol.quote_currency_code();
                }
                None => {
                    currency_pair = CurrencyPair::from_codes(
                        symbol.quote_currency_code(),
                        commission_currency_code,
                    );

                    match self.order_book_top.get(&currency_pair) {
                        Some(top_prices) => {
                            let ask = top_prices
                                .ask
                                .as_ref()
                                .expect("There are no top ask in order book");
                            let price_quote_bnb = ask.price;
                            *converted_commission_amount = commission_amount / price_quote_bnb;
                            *converted_commission_currency_code = symbol.quote_currency_code();
                        }
                        None => log::error!(
                            "Top bids and asks for {} and currency pair {currency_pair:?} do not exist",
                            self.exchange_account_id,
                        ),
                    }
                }
            }
        }
    }

    fn panic_if_fill_amounts_conformity(&self, order_filled_amount: Amount, order: &OrderRef) {
        let amount = order.amount();
        if order_filled_amount > amount {
            panic!(
                "filled_amount {order_filled_amount} > order.amount {amount} for {} {:?} on {}",
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id,
            )
        }
    }

    fn send_order_filled_event(&self, order_ref: &OrderRef) {
        let cloned_order = Arc::new(order_ref.deep_clone());
        self.add_event_on_order_change(order_ref, OrderEventType::OrderFilled { cloned_order })
            .expect("Unable to send event, probably receiver is dropped already");
    }

    fn react_if_order_completed(&self, order_filled_amount: Amount, order_ref: &OrderRef) {
        if order_filled_amount == order_ref.amount() {
            order_ref.fn_mut(|order| {
                order.set_status(OrderStatus::Completed, Utc::now());
            });

            let cloned_order = Arc::new(order_ref.deep_clone());
            self.add_event_on_order_change(
                order_ref,
                OrderEventType::OrderCompleted { cloned_order },
            )
            .expect("Unable to send event, probably receiver is dropped already");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_fill(
        &self,
        trade_id: &Option<TradeId>,
        is_diff: bool,
        fill_type: OrderFillType,
        symbol: &Symbol,
        order_ref: &OrderRef,
        converted_commission_currency_code: CurrencyCode,
        last_fill_amount: Amount,
        last_fill_price: Price,
        last_fill_cost: Decimal,
        expected_commission_rate: Percent,
        commission_amount: Amount,
        order_role: OrderRole,
        commission_currency_code: CurrencyCode,
        converted_commission_amount: Amount,
    ) {
        let last_fill_amount_in_converted_commission_currency_code = symbol
            .convert_amount_from_amount_currency_code(
                converted_commission_currency_code,
                last_fill_amount,
                last_fill_price,
            );
        let expected_converted_commission_amount =
            last_fill_amount_in_converted_commission_currency_code * expected_commission_rate;

        let referral_reward = self.commission.get_commission(order_role).referral_reward;
        let referral_reward_amount = commission_amount * referral_reward.percent_to_rate();

        let rounded_fill_price = symbol.price_round(last_fill_price, Round::ToNearest);

        let client_order_id = order_ref.client_order_id();
        let side = order_ref.side();
        let exchange_order_id = order_ref.exchange_order_id();

        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Some(ClientOrderFillId::unique_id()),
            Utc::now(),
            fill_type,
            trade_id.clone(),
            rounded_fill_price,
            last_fill_amount,
            last_fill_cost,
            order_role.into(),
            commission_currency_code,
            commission_amount,
            referral_reward_amount,
            converted_commission_currency_code,
            converted_commission_amount,
            expected_converted_commission_amount,
            is_diff,
            None,
            Some(side),
        );

        log::info!(
            "Adding a fill {} {trade_id:?} {client_order_id} {exchange_order_id:?} {order_fill:?}",
            self.exchange_account_id
        );

        order_ref.fn_mut(move |order| order.add_fill(order_fill));
    }

    fn create_and_add_order_fill(&self, fill_event: &mut FillEvent, order_ref: &OrderRef) {
        let (order_fills, order_filled_amount) = order_ref.get_fills();

        if Self::was_trade_already_received(&fill_event.trade_id, &order_fills, order_ref) {
            return;
        }

        if Self::diff_fill_after_non_diff(fill_event, &order_fills, order_ref) {
            return;
        }

        if Self::filled_amount_not_less_event_fill(fill_event, order_filled_amount, order_ref) {
            return;
        }

        let symbol = self
            .get_symbol(order_ref.currency_pair())
            .expect("Unable Unable to get symbol");
        let (last_fill_price, last_fill_amount, last_fill_cost) = match Self::get_last_fill_data(
            fill_event,
            &symbol,
            &order_fills,
            order_filled_amount,
            order_ref,
        ) {
            Some(last_fill_data) => last_fill_data,
            None => return,
        };

        if Self::should_miss_fill(fill_event, order_filled_amount, last_fill_amount, order_ref) {
            return;
        }

        if Self::panic_if_wrong_status_or_cancelled(order_ref, fill_event) {
            return;
        }

        log::info!("Received fill {fill_event:?} {last_fill_price} {last_fill_amount}");

        let commission_currency_code = fill_event
            .commission_currency_code
            .unwrap_or_else(|| symbol.get_commission_currency_code(order_ref.side()));

        let order_role = Self::get_order_role(fill_event, order_ref);

        let expected_commission_rate = self.set_commission_rate(fill_event, order_role);

        let commission_amount = Self::get_commission_amount(
            fill_event.commission_amount,
            fill_event.commission_rate,
            expected_commission_rate,
            last_fill_amount,
            last_fill_price,
            commission_currency_code,
            &symbol,
        );

        let mut converted_commission_currency_code = commission_currency_code;
        let mut converted_commission_amount = commission_amount;

        self.update_commission_for_bnb_case(
            commission_currency_code,
            &symbol,
            commission_amount,
            &mut converted_commission_amount,
            &mut converted_commission_currency_code,
        );

        self.add_fill(
            &fill_event.trade_id,
            matches!(fill_event.fill_amount, FillAmount::Incremental { .. }),
            fill_event.fill_type,
            &symbol,
            order_ref,
            converted_commission_currency_code,
            last_fill_amount,
            last_fill_price,
            last_fill_cost,
            expected_commission_rate,
            commission_amount,
            order_role,
            commission_currency_code,
            converted_commission_amount,
        );

        // This order fields updated, so let's use actual values
        let order_filled_amount = order_ref.filled_amount();

        self.panic_if_fill_amounts_conformity(order_filled_amount, order_ref);

        self.send_order_filled_event(order_ref);

        if fill_event.source_type == EventSourceType::RestFallback {
            // TODO some metrics
        }

        self.react_if_order_completed(order_filled_amount, order_ref);

        let (order_init_time, order_finished_time) =
            order_ref.fn_ref(|snapshot| (snapshot.props.init_time, snapshot.props.finished_time));
        if let Some(order_finished_time) = order_finished_time {
            let metrics_event_info = MetricsEventInfoBase::new(
                order_init_time.timestamp_millis(),
                order_finished_time.timestamp_millis(),
                MetricsEventType::OrderFromCreateToFill,
            );
            self.save_metrics(&metrics_event_info, 0);
        }

        self.event_recorder
            .save(&mut order_ref.deep_clone())
            .expect("Failure save order");
    }

    fn add_special_order_if_need(&self, fill_event: &mut FillEvent, args_to_log: &ArgsToLog) {
        if !(fill_event.fill_type.is_special()) {
            return;
        }

        let special = fill_event
            .special_order_data
            .as_ref()
            .unwrap_or_else(|| {
                panic!("Special order data should be set for liquidation trade {args_to_log:?}");
            })
            .clone();

        if fill_event.fill_type == OrderFillType::Liquidation
            && special.currency_pair.as_str().is_empty()
        {
            panic!("Currency pair should be set for liquidation trade {args_to_log:?}");
        }

        if fill_event.client_order_id.is_some() {
            panic!("Client order id cannot be set for liquidation or close position trade {args_to_log:?}");
        }

        match self
            .orders
            .cache_by_exchange_id
            .get(&fill_event.exchange_order_id)
        {
            Some(order_ref) => fill_event.client_order_id = Some(order_ref.client_order_id()),
            None => {
                let order_options = match fill_event.fill_type == OrderFillType::ClosePosition {
                    true => OrderOptions::close_position(fill_event.fill_price),
                    false => OrderOptions::liquidation(fill_event.fill_price),
                };

                // Liquidation and ClosePosition are always Takers
                let order =
                    self.create_special_order_in_pool(special, order_options, OrderRole::Taker);

                fill_event.client_order_id = Some(order.client_order_id());
                self.handle_create_order_succeeded(
                    self.exchange_account_id,
                    &order.client_order_id(),
                    &fill_event.exchange_order_id,
                    fill_event.source_type,
                )
                .expect("Error handle create order succeeded");
            }
        }
    }

    // Create special order (Liquidation or ClosePosition) in pool
    fn create_special_order_in_pool(
        &self,
        special: SpecialOrderData,
        options: OrderOptions,
        order_role: OrderRole,
    ) -> OrderRef {
        let order_instance = OrderSnapshot::with_params(
            ClientOrderId::unique_id(),
            options,
            Some(order_role),
            self.exchange_account_id,
            special.currency_pair,
            special.order_amount,
            special.order_side,
            None,
            "Unknown order from handle_order_filled()",
        );

        self.orders.add_snapshot_initial(&order_instance)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        exchanges::general::exchange::OrderBookTop, exchanges::general::exchange::PriceLevel,
        exchanges::general::test_helper, exchanges::general::test_helper::create_order_ref,
        exchanges::general::test_helper::get_test_exchange,
    };
    use anyhow::{Context, Result};
    use chrono::Utc;
    use mmb_domain::market::CurrencyCode;
    use mmb_domain::order::fill::OrderFill;
    use mmb_domain::order::pool::OrdersPool;
    use mmb_domain::order::snapshot::{
        OrderFillRole, OrderFills, OrderHeader, OrderSimpleProps, OrderStatusHistory,
        SystemInternalOrderProps,
    };
    use mmb_domain::order::snapshot::{OrderType, UserOrder};
    use serde_json::json;
    use uuid::Uuid;

    fn trade_id_from_str(str: &str) -> TradeId {
        json!(str).into()
    }

    mod liquidation {
        use super::*;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[should_panic(expected = "Special order data should be set for liquidation trade")]
        async fn empty_special_order_data() {
            let mut fill_event = FillEvent {
                source_type: EventSourceType::WebSocket,
                trade_id: Some(trade_id_from_str("empty")),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: FillAmount::Total {
                    total_filled_amount: dec!(0),
                },
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                special_order_data: None,
                fill_date: None,
            };

            let (exchange, _) = get_test_exchange(false);
            exchange.handle_order_filled(&mut fill_event);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[should_panic(
            expected = "Client order id cannot be set for liquidation or close position trade"
        )]
        async fn not_empty_client_order_id() {
            let mut fill_event = FillEvent {
                source_type: EventSourceType::WebSocket,
                trade_id: Some(trade_id_from_str("empty")),
                client_order_id: Some(ClientOrderId::unique_id()),
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: FillAmount::Total {
                    total_filled_amount: dec!(0),
                },
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                special_order_data: Some(SpecialOrderData {
                    currency_pair: CurrencyPair::from_codes("te".into(), "st".into()),
                    order_side: OrderSide::Buy,
                    order_amount: dec!(7),
                }),
                fill_date: None,
            };

            let (exchange, _) = get_test_exchange(false);
            exchange.handle_order_filled(&mut fill_event);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn should_add_order() {
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let order_amount = dec!(12);
            let order_role = None;
            let fill_price = dec!(0.2);
            let fill_amount = FillAmount::Total {
                total_filled_amount: dec!(5),
            };

            let mut fill_event = FillEvent {
                source_type: EventSourceType::WebSocket,
                trade_id: Some(trade_id_from_str("empty")),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price,
                fill_amount,
                order_role,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                special_order_data: Some(SpecialOrderData {
                    currency_pair,
                    order_side,
                    order_amount,
                }),
                fill_date: None,
            };

            let (exchange, _event_received) = get_test_exchange(false);
            exchange.handle_order_filled(&mut fill_event);

            let order = exchange
                .orders
                .cache_by_client_id
                .iter()
                .next()
                .expect("order should be added already");
            assert_eq!(order.order_type(), OrderType::Liquidation);
            assert_eq!(order.exchange_account_id(), exchange.exchange_account_id);
            assert_eq!(order.currency_pair(), currency_pair);
            assert_eq!(order.side(), order_side);
            assert_eq!(order.amount(), order_amount);
            assert_eq!(order.price(), fill_price);
            assert_eq!(order.role(), Some(OrderRole::Taker));

            let (fills, filled_amount) = order.get_fills();
            assert_eq!(Some(filled_amount), fill_amount.total_filled_amount());
            assert_eq!(fills.get(0).expect("in test").price(), fill_price);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[should_panic(expected = "Received HandleOrderFilled with an empty exchangeOrderId")]
        async fn empty_exchange_order_id() {
            let mut fill_event = FillEvent {
                source_type: EventSourceType::WebSocket,
                trade_id: Some(trade_id_from_str("empty")),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("".into()),
                fill_price: dec!(0),
                fill_amount: FillAmount::Total {
                    total_filled_amount: dec!(0),
                },
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                special_order_data: Some(SpecialOrderData {
                    currency_pair: CurrencyPair::from_codes("te".into(), "st".into()),
                    order_side: OrderSide::Buy,
                    order_amount: dec!(0),
                }),
                fill_date: None,
            };

            let (exchange, _event_receiver) = get_test_exchange(false);
            exchange.handle_order_filled(&mut fill_event);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ignore_if_trade_was_already_received() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let order_amount = dec!(1);
        let trade_id = trade_id_from_str("test_trade_id");
        let total_filled_amount = dec!(0.2);
        let fill_amount = FillAmount::Total {
            total_filled_amount,
        };

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id.clone()),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair: CurrencyPair::from_codes("te".into(), "st".into()),
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            None,
            Utc::now(),
            OrderFillType::Liquidation,
            Some(trade_id),
            order_price,
            total_filled_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, total_filled_amount);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ignore_diff_fill_after_non_diff() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let incremental_fill_amount = dec!(0.2);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };

        let order_amount = dec!(1);
        let trade_id = trade_id_from_str("test_trade_id");

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair: CurrencyPair::from_codes("te".into(), "st".into()),
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            None,
            Utc::now(),
            OrderFillType::Liquidation,
            Some(trade_id_from_str("different_trade_id")),
            order_price,
            incremental_fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, incremental_fill_amount);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ignore_filled_amount_not_less_event_fill() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let total_filled_amount = dec!(0.2);
        let fill_amount = FillAmount::Total {
            total_filled_amount,
        };
        let order_amount = dec!(1);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair: CurrencyPair::from_codes("te".into(), "st".into()),
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            None,
            Utc::now(),
            OrderFillType::Liquidation,
            Some(trade_id_from_str("different_trade_id")),
            order_price,
            total_filled_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, total_filled_amount);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ignore_diff_fill_if_filled_amount_is_zero() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let incremental_fill_amount = dec!(0);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(1);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.2),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            None,
            Utc::now(),
            OrderFillType::Liquidation,
            Some(trade_id_from_str("different_trade_id")),
            order_price,
            incremental_fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test"),
            dec!(0),
            dec!(0),
            true,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, dec!(0));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "Fill was received for a FailedToCreate false")]
    async fn error_if_order_status_is_failed_to_create() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(1);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(1);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.2),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.set_status(OrderStatus::FailedToCreate, Utc::now());

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "Fill was received for a Completed false")]
    async fn error_if_order_status_is_completed() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(1);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(1);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.2),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.set_status(OrderStatus::Completed, Utc::now());

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn do_not_add_fill_if_cancellation_event_was_raised() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(1);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(1);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let fill_price = dec!(0.2);

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price,
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.internal_props.was_cancellation_event_raised = true;

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        assert!(order_ref.get_fills().0.is_empty());
    }

    // TODO Can be improved via testing only calculate_cost_diff_function
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_cost_diff_on_buy_side() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let total_filled_amount = dec!(5);
        let fill_amount = FillAmount::Total {
            total_filled_amount,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::with_user_order(
            client_order_id,
            exchange.exchange_account_id,
            currency_pair,
            OrderSide::Buy,
            order_amount,
            UserOrder::limit(order_price),
            None,
            None,
            "FromTest".to_owned(),
        );
        let props = OrderSimpleProps::new(
            Utc::now(),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
            None,
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);
        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let mut first_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.2),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut first_fill_event);

        let mut second_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("another_trade_id")),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.3),
            fill_amount: FillAmount::Total {
                total_filled_amount: dec!(10),
            },
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut second_fill_event);

        let order_ref = exchange
            .orders
            .cache_by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");
        let (fills, _filled_amount) = order_ref.get_fills();

        assert_eq!(fills.len(), 2);
        let first_fill = &fills[0];
        assert_eq!(first_fill.price(), dec!(0.2));
        assert_eq!(first_fill.amount(), dec!(5));
        assert_eq!(first_fill.commission_amount(), dec!(0.01));
        let second_fill = &fills[1];
        assert_eq!(second_fill.price(), dec!(0.4));
        assert_eq!(second_fill.amount(), dec!(5));
        assert_eq!(second_fill.commission_amount(), dec!(0.02));
    }

    // TODO Can be improved via testing only calculate_cost_diff_function
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_cost_diff_on_sell_side() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let total_filled_amount = dec!(5);
        let fill_amount = FillAmount::Total {
            total_filled_amount,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::with_user_order(
            client_order_id,
            exchange.exchange_account_id,
            currency_pair,
            OrderSide::Sell,
            order_amount,
            UserOrder::limit(order_price),
            None,
            None,
            "FromTest".to_owned(),
        );
        let props = OrderSimpleProps::new(
            Utc::now(),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
            None,
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let mut first_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.2),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut first_fill_event);

        let mut second_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("another_trade_id")),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.3),
            fill_amount: FillAmount::Total {
                total_filled_amount: dec!(10),
            },
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut second_fill_event);

        let order_ref = exchange
            .orders
            .cache_by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");
        let (fills, _filled_amount) = order_ref.get_fills();

        assert_eq!(fills.len(), 2);
        let first_fill = &fills[0];
        assert_eq!(first_fill.price(), dec!(0.2));
        assert_eq!(first_fill.amount(), dec!(5));
        assert_eq!(first_fill.commission_amount(), dec!(0.01));
        let second_fill = &fills[1];
        assert_eq!(second_fill.price(), dec!(0.4));
        assert_eq!(second_fill.amount(), dec!(5));
        assert_eq!(second_fill.commission_amount(), dec!(0.02));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_cost_diff_on_buy_side_derivative() {
        let (exchange, _event_receiver) = get_test_exchange(true);

        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let total_filled_amount = dec!(5);
        let fill_amount = FillAmount::Total {
            total_filled_amount,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::with_user_order(
            client_order_id,
            exchange.exchange_account_id,
            currency_pair,
            OrderSide::Buy,
            order_amount,
            UserOrder::limit(order_price),
            None,
            None,
            "FromTest".to_owned(),
        );
        let props = OrderSimpleProps::new(
            Utc::now(),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
            None,
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);
        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let mut first_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(2000),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut first_fill_event);

        let mut second_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("another_trade_id")),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(3000),
            fill_amount: FillAmount::Total {
                total_filled_amount: dec!(10),
            },
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut second_fill_event);

        let order_ref = exchange
            .orders
            .cache_by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");

        let (fills, filled_amount) = order_ref.get_fills();

        assert_eq!(filled_amount, dec!(10));
        assert_eq!(fills.len(), 2);

        let first_fill = &fills[0];
        assert_eq!(first_fill.price(), dec!(2000));
        assert_eq!(first_fill.amount(), dec!(5));
        assert_eq!(first_fill.commission_amount(), dec!(0.01));

        let second_fill = &fills[1];
        assert_eq!(second_fill.price(), dec!(6000));
        assert_eq!(second_fill.amount(), dec!(5));
        assert_eq!(second_fill.commission_amount(), dec!(0.02));
    }

    // TODO Why do we need tests like this?
    // Nothing depends on order.side as I can see
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_cost_diff_on_sell_side_derivative() {
        let (exchange, _event_receiver) = get_test_exchange(true);

        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let total_filled_amount = dec!(5);
        let fill_amount = FillAmount::Total {
            total_filled_amount,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::with_user_order(
            client_order_id,
            exchange.exchange_account_id,
            currency_pair,
            OrderSide::Sell,
            order_amount,
            UserOrder::limit(order_price),
            None,
            None,
            "FromTest".to_owned(),
        );
        let props = OrderSimpleProps::new(
            Utc::now(),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
            None,
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);
        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let mut first_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(2000),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut first_fill_event);

        let mut second_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("another_trade_id")),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(3000),
            fill_amount: FillAmount::Total {
                total_filled_amount: dec!(10),
            },
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut second_fill_event);

        let order_ref = exchange
            .orders
            .cache_by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");

        let (fills, filled_amount) = order_ref.get_fills();

        assert_eq!(filled_amount, dec!(10));
        assert_eq!(fills.len(), 2);

        let first_fill = &fills[0];
        assert_eq!(first_fill.price(), dec!(2000));
        assert_eq!(first_fill.amount(), dec!(5));
        assert_eq!(first_fill.commission_amount(), dec!(0.01));

        let second_fill = &fills[1];
        assert_eq!(second_fill.price(), dec!(6000));
        assert_eq!(second_fill.amount(), dec!(5));
        assert_eq!(second_fill.commission_amount(), dec!(0.02));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ignore_non_diff_fill_with_second_cost_lesser() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let total_filled_amount = dec!(5);
        let fill_amount = FillAmount::Total {
            total_filled_amount,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::with_user_order(
            client_order_id,
            exchange.exchange_account_id,
            currency_pair,
            OrderSide::Sell,
            order_amount,
            UserOrder::limit(order_price),
            None,
            None,
            "FromTest".to_owned(),
        );
        let props = OrderSimpleProps::new(
            Utc::now(),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
            None,
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);
        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let mut first_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut first_fill_event);

        let mut second_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("another_trade_id")),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.3),
            fill_amount: FillAmount::Total {
                total_filled_amount: dec!(10),
            },
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.handle_order_filled(&mut second_fill_event);

        let order_ref = exchange
            .orders
            .cache_by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");

        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ignore_fill_if_total_filled_amount_is_incorrect() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: Some(dec!(9)),
        };
        let order_amount = dec!(1);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (fills, _) = order_ref.get_fills();
        assert!(fills.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn take_roll_from_fill_if_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: Some(OrderRole::Taker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_event.fill_price),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        let right_value = dec!(0.2) / dec!(100) * dec!(5);
        assert_eq!(fill.commission_amount(), right_value);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn take_roll_from_order_if_not_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        let right_value = dec!(0.1) / dec!(100) * dec!(5);
        assert_eq!(fill.commission_amount(), right_value);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "Fill has neither commission nor commission rate")]
    async fn error_if_unable_to_get_role() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            None,
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        Exchange::get_order_role(&fill_event, &order_ref);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn use_commission_currency_code_from_fill_event() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let commission_currency_code = CurrencyCode::new("BTC");

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: Some(commission_currency_code),
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        assert_eq!(
            fill.converted_commission_currency_code(),
            commission_currency_code
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn commission_currency_code_from_base_currency_code() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let base_currency_code = CurrencyCode::new("PHB");

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        assert_eq!(
            fill.converted_commission_currency_code(),
            base_currency_code
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn commission_currency_code_from_quote_currency_code() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Sell;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let quote_currency_code = exchange
            .symbols
            .iter()
            .next()
            .expect("in test")
            .value()
            .quote_currency_code;

        let fill = &fills[0];
        assert_eq!(
            fill.converted_commission_currency_code(),
            quote_currency_code
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn use_commission_amount_if_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Sell;
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let commission_amount = dec!(0.001);

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(commission_amount),
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let first_fill = &fills[0];
        assert_eq!(first_fill.commission_amount(), commission_amount);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn use_commission_rate_if_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Sell;
        let fill_price = dec!(0.8);
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));
        let commission_rate = dec!(0.3) / dec!(100);

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price,
            fill_amount,
            order_role: None,
            commission_currency_code: Some("BTC".into()),
            commission_rate: Some(commission_rate),
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let first_fill = &fills[0];
        let result_value = commission_rate * fill_price * incremental_fill_amount;
        assert_eq!(first_fill.commission_amount(), result_value);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_commission_rate_if_not_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Sell;
        let fill_price = dec!(0.8);
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price,
            fill_amount,
            order_role: None,
            commission_currency_code: Some("BTC".into()),
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let first_fill = &fills[0];
        let result_value = dec!(0.1) / dec!(100) * fill_price * incremental_fill_amount;
        assert_eq!(first_fill.commission_amount(), result_value);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_commission_amount() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.8);
        let incremental_fill_amount = dec!(5);
        let fill_amount = FillAmount::Incremental {
            fill_amount: incremental_fill_amount,
            total_filled_amount: None,
        };
        let order_amount = dec!(12);
        let trade_id = Some(trade_id_from_str("test_trade_id"));

        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id,
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price,
            fill_amount,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(dec!(0.2)),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let first_fill = &fills[0];
        let result_value = dec!(0.1) / dec!(100) * incremental_fill_amount;
        assert_eq!(first_fill.commission_amount(), result_value);
    }

    mod get_commission_amount {
        use super::*;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn from_fill_event() -> Result<()> {
            let (exchange, _event_receiver) = get_test_exchange(true);

            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());

            let commission_rate = dec!(0.001);
            let expected_commission_rate = dec!(0.001);
            let last_fill_amount = dec!(5);
            let last_fill_price = dec!(0.8);
            let commission_currency_code = CurrencyCode::new("PHB");
            let symbol = exchange.get_symbol(currency_pair)?;
            let fill_event_commission_amount = dec!(6.3);

            let commission_amount = Exchange::get_commission_amount(
                Some(fill_event_commission_amount),
                Some(commission_rate),
                expected_commission_rate,
                last_fill_amount,
                last_fill_price,
                commission_currency_code,
                &symbol,
            );

            let right_value = fill_event_commission_amount;
            assert_eq!(commission_amount, right_value);

            Ok(())
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn via_commission_rate() -> Result<()> {
            let (exchange, _event_receiver) = get_test_exchange(true);

            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());

            let commission_rate = dec!(0.001);
            let expected_commission_rate = dec!(0.001);
            let last_fill_amount = dec!(5);
            let last_fill_price = dec!(0.8);
            let commission_currency_code = CurrencyCode::new("PHB");
            let symbol = exchange.get_symbol(currency_pair)?;
            let commission_amount = Exchange::get_commission_amount(
                None,
                Some(commission_rate),
                expected_commission_rate,
                last_fill_amount,
                last_fill_price,
                commission_currency_code,
                &symbol,
            );

            let right_value = dec!(0.1) / dec!(100) * dec!(5) / dec!(0.8);
            assert_eq!(commission_amount, right_value);

            Ok(())
        }
    }

    mod add_fill {
        use super::*;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn expected_commission_amount_equal_commission_amount() -> Result<()> {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let order_amount = dec!(12);
            let order_role = OrderRole::Maker;
            let fill_price = dec!(0.8);

            let order_ref = create_order_ref(
                &client_order_id,
                Some(order_role),
                exchange.exchange_account_id,
                currency_pair,
                fill_price,
                order_amount,
                order_side,
            );

            let trade_id = Some(trade_id_from_str("test trade_id"));
            let is_diff = true;
            let symbol = exchange.get_symbol(currency_pair)?;
            let converted_commission_currency_code =
                symbol.get_commission_currency_code(order_side);
            let last_fill_amount = dec!(5);
            let last_fill_price = dec!(0.8);
            let last_fill_cost = dec!(4.0);
            let expected_commission_rate = dec!(0.001);
            let commission_currency_code = CurrencyCode::new("PHB");
            let converted_commission_amount = dec!(0.005);
            let commission_amount = dec!(0.1) / dec!(100) * dec!(5);

            exchange.add_fill(
                &trade_id,
                is_diff,
                OrderFillType::Liquidation,
                &symbol,
                &order_ref,
                converted_commission_currency_code,
                last_fill_amount,
                last_fill_price,
                last_fill_cost,
                expected_commission_rate,
                commission_amount,
                order_role,
                commission_currency_code,
                converted_commission_amount,
            );

            let fill = order_ref.get_fills().0.last().cloned().expect("in test");

            assert_eq!(fill.commission_amount(), commission_amount);
            assert_eq!(
                fill.expected_converted_commission_amount(),
                commission_amount
            );

            Ok(())
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn expected_commission_amount_not_equal_wrong_commission_amount() -> Result<()> {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let order_amount = dec!(12);
            let fill_price = dec!(0.8);
            let order_role = OrderRole::Maker;

            let order_ref = create_order_ref(
                &client_order_id,
                Some(order_role),
                exchange.exchange_account_id,
                currency_pair,
                fill_price,
                order_amount,
                order_side,
            );

            let trade_id = Some(trade_id_from_str("test trade_id"));
            let is_diff = true;
            let symbol = exchange.get_symbol(currency_pair)?;
            let converted_commission_currency_code =
                symbol.get_commission_currency_code(order_side);
            let last_fill_amount = dec!(5);
            let last_fill_price = dec!(0.8);
            let last_fill_cost = dec!(4.0);
            let expected_commission_rate = dec!(0.001);
            let commission_currency_code = CurrencyCode::new("PHB");
            let converted_commission_amount = dec!(0.005);
            let commission_amount = dec!(1000);

            exchange.add_fill(
                &trade_id,
                is_diff,
                OrderFillType::Liquidation,
                &symbol,
                &order_ref,
                converted_commission_currency_code,
                last_fill_amount,
                last_fill_price,
                last_fill_cost,
                expected_commission_rate,
                commission_amount,
                order_role,
                commission_currency_code,
                converted_commission_amount,
            );

            let fill = order_ref.get_fills().0.last().cloned().expect("in test");

            assert_eq!(fill.commission_amount(), commission_amount);
            let right_value = dec!(0.1) / dec!(100) * dec!(5);
            assert_eq!(fill.expected_converted_commission_amount(), right_value);

            Ok(())
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn check_referral_reward_amount() -> Result<()> {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let order_role = OrderRole::Maker;
            let order_amount = dec!(12);
            let fill_price = dec!(0.8);

            let order_ref = create_order_ref(
                &client_order_id,
                Some(order_role),
                exchange.exchange_account_id,
                currency_pair,
                fill_price,
                order_amount,
                order_side,
            );

            let trade_id = Some(trade_id_from_str("test trade_id"));
            let is_diff = true;
            let symbol = exchange.get_symbol(currency_pair)?;
            let converted_commission_currency_code =
                symbol.get_commission_currency_code(order_side);
            let last_fill_amount = dec!(5);
            let last_fill_price = dec!(0.8);
            let last_fill_cost = dec!(4.0);
            let expected_commission_rate = dec!(0.001);
            let commission_amount = dec!(0.005);
            let commission_currency_code = CurrencyCode::new("PHB");
            let converted_commission_amount = dec!(0.005);

            exchange.add_fill(
                &trade_id,
                is_diff,
                OrderFillType::Liquidation,
                &symbol,
                &order_ref,
                converted_commission_currency_code,
                last_fill_amount,
                last_fill_price,
                last_fill_cost,
                expected_commission_rate,
                commission_amount,
                order_role,
                commission_currency_code,
                converted_commission_amount,
            );

            let fill = order_ref.get_fills().0.last().cloned().expect("in test");

            let right_value = dec!(5) * dec!(0.1) / dec!(100) * dec!(0.4);
            assert_eq!(fill.referral_reward_amount(), right_value);

            Ok(())
        }
    }

    mod check_fill_amounts_conformity {
        use super::*;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[should_panic(expected = "filled_amount 13 > order.amount 12 fo")]
        async fn too_big_filled_amount() {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let fill_price = dec!(0.8);
            let order_amount = dec!(12);

            let order_ref = create_order_ref(
                &client_order_id,
                Some(OrderRole::Maker),
                exchange.exchange_account_id,
                currency_pair,
                fill_price,
                order_amount,
                order_side,
            );

            let fill_amount = dec!(13);
            exchange.panic_if_fill_amounts_conformity(fill_amount, &order_ref);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn proper_filled_amount() {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let fill_price = dec!(0.8);
            let order_amount = dec!(12);

            let order_ref = create_order_ref(
                &client_order_id,
                Some(OrderRole::Maker),
                exchange.exchange_account_id,
                currency_pair,
                fill_price,
                order_amount,
                order_side,
            );

            let fill_amount = dec!(10);
            exchange.panic_if_fill_amounts_conformity(fill_amount, &order_ref);
        }
    }

    mod react_if_order_completed {
        use super::*;
        use mmb_domain::events::ExchangeEvent;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn order_completed_if_filled_completely() -> Result<()> {
            let (exchange, mut event_receiver) = get_test_exchange(false);
            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let fill_price = dec!(0.2);
            let order_amount = dec!(12);
            let order_ref = create_order_ref(
                &client_order_id,
                Some(OrderRole::Maker),
                exchange.exchange_account_id,
                currency_pair,
                fill_price,
                order_amount,
                order_side,
            );
            let order_filled_amount = order_amount;
            exchange.react_if_order_completed(order_filled_amount, &order_ref);
            let order_status = order_ref.status();

            assert_eq!(order_status, OrderStatus::Completed);

            let event = match event_receiver
                .try_recv()
                .context("Event was not received")?
            {
                ExchangeEvent::OrderEvent(v) => v,
                _ => panic!("Should be OrderEvent"),
            };
            let gotten_id = event.order.client_order_id();
            assert_eq!(gotten_id, client_order_id);
            Ok(())
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn order_not_filled() {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_side = OrderSide::Buy;
            let fill_price = dec!(0.2);
            let order_amount = dec!(12);

            let order_ref = create_order_ref(
                &client_order_id,
                Some(OrderRole::Maker),
                exchange.exchange_account_id,
                currency_pair,
                fill_price,
                order_amount,
                order_side,
            );

            let order_filled_amount = dec!(10);
            exchange.react_if_order_completed(order_filled_amount, &order_ref);

            let order_status = order_ref.status();

            assert_ne!(order_status, OrderStatus::Completed);
        }
    }

    mod update_commission_for_bnb_case {
        use super::*;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn using_top_bid() {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let commission_currency_code = CurrencyCode::new("BNB");
            let symbol = exchange
                .symbols
                .iter()
                .next()
                .expect("in test")
                .value()
                .clone();
            let commission_amount = dec!(15);
            let mut converted_commission_amount = dec!(4.5);
            let mut converted_commission_currency_code = CurrencyCode::new("BTC");

            let currency_pair =
                CurrencyPair::from_codes(commission_currency_code, symbol.quote_currency_code);
            let order_book_top = OrderBookTop {
                ask: None,
                bid: Some(PriceLevel {
                    price: dec!(0.3),
                    amount: dec!(0.1),
                }),
            };
            exchange
                .order_book_top
                .insert(currency_pair, order_book_top);

            exchange.update_commission_for_bnb_case(
                commission_currency_code,
                &symbol,
                commission_amount,
                &mut converted_commission_amount,
                &mut converted_commission_currency_code,
            );

            let right_amount = dec!(4.5);
            assert_eq!(converted_commission_amount, right_amount);

            let right_currency_code = CurrencyCode::new("BTC");
            assert_eq!(converted_commission_currency_code, right_currency_code);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn using_top_ask() {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let commission_currency_code = CurrencyCode::new("BNB");
            let symbol = exchange
                .symbols
                .iter()
                .next()
                .expect("in test")
                .value()
                .clone();
            let commission_amount = dec!(15);
            let mut converted_commission_amount = dec!(4.5);
            let mut converted_commission_currency_code = CurrencyCode::new("BTC");

            let currency_pair = CurrencyPair::from_codes("BTC".into(), commission_currency_code);
            let order_book_top = OrderBookTop {
                ask: Some(PriceLevel {
                    price: dec!(0.3),
                    amount: dec!(0.1),
                }),
                bid: None,
            };
            exchange
                .order_book_top
                .insert(currency_pair, order_book_top);

            exchange.update_commission_for_bnb_case(
                commission_currency_code,
                &symbol,
                commission_amount,
                &mut converted_commission_amount,
                &mut converted_commission_currency_code,
            );

            let right_amount = dec!(50);
            assert_eq!(converted_commission_amount, right_amount);

            let right_currency_code = CurrencyCode::new("BTC");
            assert_eq!(converted_commission_currency_code, right_currency_code);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn fatal_error() {
            let (exchange, _event_receiver) = get_test_exchange(false);

            let commission_currency_code = CurrencyCode::new("BNB");
            let symbol = exchange
                .symbols
                .iter()
                .next()
                .expect("in test")
                .value()
                .clone();
            let commission_amount = dec!(15);
            let mut converted_commission_amount = dec!(3);
            let mut converted_commission_currency_code = CurrencyCode::new("BTC");

            exchange.update_commission_for_bnb_case(
                commission_currency_code,
                &symbol,
                commission_amount,
                &mut converted_commission_amount,
                &mut converted_commission_currency_code,
            );

            let right_amount = dec!(3);
            assert_eq!(converted_commission_amount, right_amount);

            let right_currency_code = CurrencyCode::new("BTC");
            assert_eq!(converted_commission_currency_code, right_currency_code);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn filled_amount_from_zero_to_completed() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.8);
        let order_amount = dec!(12);
        let exchange_order_id = ExchangeOrderId::new("some_exchange_order_id".into());
        let client_account_id = ClientOrderId::unique_id();

        let order = OrderSnapshot::with_params(
            client_order_id,
            OrderOptions::liquidation(fill_price),
            Some(OrderRole::Maker),
            exchange.exchange_account_id,
            currency_pair,
            order_amount,
            order_side,
            None,
            "FromTest",
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(&order);

        let fill_amount = FillAmount::Incremental {
            fill_amount: dec!(5),
            total_filled_amount: None,
        };
        let mut fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("first_trade_id")),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_order_id.clone(),
            fill_price,
            fill_amount,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.create_and_add_order_fill(&mut fill_event, &order_ref);

        let (_, filled_amount) = order_ref.get_fills();

        let current_right_filled_amount = dec!(5);
        assert_eq!(filled_amount, current_right_filled_amount);

        let fill_amount = FillAmount::Incremental {
            fill_amount: dec!(2),
            total_filled_amount: None,
        };
        let mut second_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("second_trade_id")),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_order_id.clone(),
            fill_price,
            fill_amount,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.create_and_add_order_fill(&mut second_fill_event, &order_ref);

        let (_, filled_amount) = order_ref.get_fills();

        let right_filled_amount = dec!(7);
        assert_eq!(filled_amount, right_filled_amount);

        let fill_amount = FillAmount::Incremental {
            fill_amount: dec!(5),
            total_filled_amount: None,
        };
        let mut second_fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id_from_str("third_trade_id")),
            client_order_id: Some(client_account_id),
            exchange_order_id,
            fill_price,
            fill_amount,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            special_order_data: Some(SpecialOrderData {
                currency_pair,
                order_side: OrderSide::Buy,
                order_amount: dec!(0),
            }),
            fill_date: None,
        };

        exchange.create_and_add_order_fill(&mut second_fill_event, &order_ref);

        let (_, filled_amount) = order_ref.get_fills();

        let right_filled_amount = dec!(12);
        assert_eq!(filled_amount, right_filled_amount);

        let order_status = order_ref.status();
        assert_eq!(order_status, OrderStatus::Completed);
    }
}
