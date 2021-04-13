use std::sync::Arc;

use super::{
    currency_pair_metadata::CurrencyPairMetadata, currency_pair_metadata::Round, exchange::Exchange,
};
use crate::core::{
    exchanges::common::Amount, exchanges::common::CurrencyCode, exchanges::common::CurrencyPair,
    exchanges::common::ExchangeAccountId, exchanges::common::Price,
    exchanges::events::AllowedEventSourceType, orders::fill::EventSourceType,
    orders::fill::OrderFill, orders::fill::OrderFillType, orders::order::ClientOrderId,
    orders::order::ExchangeOrderId, orders::order::OrderEventType, orders::order::OrderRole,
    orders::order::OrderSide, orders::order::OrderSnapshot, orders::order::OrderStatus,
    orders::order::OrderType, orders::pool::OrderRef,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use log::{error, info, warn};
use parking_lot::RwLock;
use rust_decimal::prelude::Zero;
use rust_decimal_macros::dec;
use uuid::Uuid;

type ArgsToLog = (
    ExchangeAccountId,
    String,
    Option<ClientOrderId>,
    ExchangeOrderId,
    AllowedEventSourceType,
    EventSourceType,
);

#[derive(Debug, Clone)]
pub struct FillEventData {
    pub source_type: EventSourceType,
    pub trade_id: String,
    pub client_order_id: Option<ClientOrderId>,
    pub exchange_order_id: ExchangeOrderId,
    pub fill_price: Price,
    pub fill_amount: Amount,
    pub is_diff: bool,
    pub total_filled_amount: Option<Amount>,
    pub order_role: Option<OrderRole>,
    pub commission_currency_code: Option<CurrencyCode>,
    pub commission_rate: Option<Amount>,
    pub commission_amount: Option<Amount>,
    pub fill_type: OrderFillType,
    pub trade_currency_pair: Option<CurrencyPair>,
    pub order_side: Option<OrderSide>,
    pub order_amount: Option<Amount>,
}

impl Exchange {
    pub fn handle_order_filled(&self, mut event_data: FillEventData) -> Result<()> {
        let args_to_log = (
            self.exchange_account_id.clone(),
            event_data.trade_id.clone(),
            event_data.client_order_id.clone(),
            event_data.exchange_order_id.clone(),
            self.features.allowed_fill_event_source_type,
            event_data.source_type,
        );

        if Self::should_ignore_event(
            self.features.allowed_fill_event_source_type,
            event_data.source_type,
        ) {
            info!("Ignoring fill {:?}", args_to_log);
            return Ok(());
        }

        if event_data.exchange_order_id.as_str().is_empty() {
            Self::log_fill_handling_error_and_propagate(
                "Received HandleOrderFilled with an empty exchangeOrderId",
                &args_to_log,
            )?;
        }

        self.check_based_on_fill_type(&mut event_data, &args_to_log)?;

        match self
            .orders
            .by_exchange_id
            .get(&event_data.exchange_order_id)
        {
            None => {
                info!("Received a fill for not existing order {:?}", &args_to_log);
                // TODO BufferedFillsManager.add_fill()

                if let Some(client_order_id) = event_data.client_order_id {
                    self.raise_order_created(
                        client_order_id,
                        event_data.exchange_order_id,
                        event_data.source_type,
                    );
                }

                return Ok(());
            }
            Some(order) => self.local_order_exist(&mut event_data, &*order),
        }
    }

    fn was_trade_already_received(
        trade_id: &str,
        order_fills: &Vec<OrderFill>,
        order_ref: &OrderRef,
    ) -> bool {
        if !trade_id.is_empty()
            && order_fills.iter().any(|fill| {
                if let Some(fill_trade_id) = fill.trade_id() {
                    return fill_trade_id == &trade_id;
                }

                false
            })
        {
            info!(
                "Trade with {} was received already for order {:?}",
                trade_id, order_ref
            );

            return true;
        }

        false
    }

    fn diff_fill_after_non_diff(
        event_data: &FillEventData,
        order_fills: &Vec<OrderFill>,
        order_ref: &OrderRef,
    ) -> bool {
        if event_data.is_diff && order_fills.iter().any(|fill| !fill.is_diff()) {
            // Most likely we received a trade update (diff), then received a non-diff fill via fallback and then again received a diff trade update
            // It happens when WebSocket is glitchy and we miss update and the problem is we have no idea how to handle diff updates
            // after applying a non-diff one as there's no TradeId, so we have to ignore all the diff updates afterwards
            // relying only on fallbacks
            warn!(
                "Unable to process a diff fill after a non-diff one {:?}",
                order_ref
            );

            return true;
        }

        false
    }

    fn filled_amount_not_less_event_fill(
        event_data: &FillEventData,
        order_filled_amount: Amount,
        order_ref: &OrderRef,
    ) -> bool {
        if !event_data.is_diff && order_filled_amount >= event_data.fill_amount {
            warn!(
                "order.filled_amount is {} >= received fill {}, so non-diff fill for {} {:?} should be ignored",
                order_filled_amount,
                event_data.fill_amount,
                order_ref.client_order_id(),
                order_ref.exchange_order_id(),
            );

            return true;
        }

        false
    }

    fn should_miss_fill(
        event_data: &FillEventData,
        order_filled_amount: Amount,
        last_fill_amount: Amount,
        order_ref: &OrderRef,
    ) -> bool {
        if let Some(total_filled_amount) = event_data.total_filled_amount {
            if order_filled_amount + last_fill_amount != total_filled_amount {
                warn!(
                    "Fill was missed because {} != {} for {:?}",
                    order_filled_amount, total_filled_amount, order_ref
                );

                return true;
            }
        }

        false
    }

    fn get_last_fill_data(
        mut event_data: &mut FillEventData,
        currency_pair_metadata: &CurrencyPairMetadata,
        order_fills: &Vec<OrderFill>,
        order_filled_amount: Amount,
        order_ref: &OrderRef,
    ) -> Result<Option<(Price, Amount, Price)>> {
        let mut last_fill_amount = event_data.fill_amount;
        let mut last_fill_price = event_data.fill_price;
        let mut last_fill_cost = if !currency_pair_metadata.is_derivative() {
            last_fill_amount * last_fill_price
        } else {
            last_fill_amount / last_fill_price
        };

        if !event_data.is_diff && order_fills.len() > 0 {
            match Self::calculate_cost_diff(&order_fills, &*order_ref, last_fill_cost) {
                None => return Ok(None),
                Some(cost_diff) => {
                    let (price, amount, cost) = Self::calculate_last_fill_data(
                        last_fill_amount,
                        &order_fills,
                        order_filled_amount,
                        &currency_pair_metadata,
                        cost_diff,
                        &mut event_data,
                    )?;
                    last_fill_price = price;
                    last_fill_amount = amount;
                    last_fill_cost = cost
                }
            };
        }

        if last_fill_amount.is_zero() {
            warn!(
                "last_fill_amount was received for 0 for {}, {:?}",
                order_ref.client_order_id(),
                order_ref.exchange_order_id()
            );

            return Ok(None);
        }

        Ok(Some((last_fill_price, last_fill_amount, last_fill_cost)))
    }

    fn calculate_cost_diff(
        order_fills: &Vec<OrderFill>,
        order_ref: &OrderRef,
        last_fill_cost: Price,
    ) -> Option<Price> {
        // Diff should be calculated only if it is not the first fill
        let mut total_filled_cost = dec!(0);
        order_fills
            .iter()
            .for_each(|fill| total_filled_cost += fill.cost());
        let cost_diff = last_fill_cost - total_filled_cost;
        if cost_diff <= dec!(0) {
            warn!(
                "cost_diff is {} which is <= 0 for {:?}",
                cost_diff, order_ref
            );

            return None;
        }

        Some(cost_diff)
    }

    fn calculate_last_fill_data(
        last_fill_amount: Amount,
        order_fills: &Vec<OrderFill>,
        order_filled_amount: Amount,
        currency_pair_metadata: &CurrencyPairMetadata,
        cost_diff: Price,
        event_data: &mut FillEventData,
    ) -> Result<(Price, Amount, Price)> {
        let amount_diff = last_fill_amount - order_filled_amount;
        let res_fill_price = if !currency_pair_metadata.is_derivative() {
            cost_diff / amount_diff
        } else {
            amount_diff / cost_diff
        };
        let last_fill_price =
            currency_pair_metadata.price_round(res_fill_price, Round::ToNearest)?;

        let last_fill_amount = amount_diff;
        let last_fill_cost = cost_diff;

        if let Some(commission_amount) = event_data.commission_amount {
            let mut current_commission = dec!(0);
            order_fills
                .iter()
                .for_each(|fill| current_commission += fill.commission_amount());
            event_data.commission_amount = Some(commission_amount - current_commission);
        }

        Ok((last_fill_price, last_fill_amount, last_fill_cost))
    }

    fn wrong_status_or_cancelled(order_ref: &OrderRef, event_data: &FillEventData) -> Result<()> {
        if order_ref.status() == OrderStatus::FailedToCreate
            || order_ref.status() == OrderStatus::Completed
            || order_ref.was_cancellation_event_raised()
        {
            let error_msg = format!(
                "Fill was received for a {:?} {} {:?}",
                order_ref.status(),
                order_ref.was_cancellation_event_raised(),
                event_data
            );

            error!("{}", error_msg);
            bail!("{}", error_msg)
        }

        Ok(())
    }

    fn get_order_role(event_data: &FillEventData, order_ref: &OrderRef) -> Result<OrderRole> {
        match &event_data.order_role {
            Some(order_role) => Ok(order_role.clone()),
            None => {
                if event_data.commission_amount.is_none()
                    && event_data.commission_rate.is_none()
                    && order_ref.role().is_none()
                {
                    let error_msg = format!("Fill has neither commission nor commission rate",);

                    error!("{}", error_msg);
                    bail!("{}", error_msg)
                }

                match order_ref.role() {
                    Some(role) => Ok(role),
                    None => {
                        let error_msg = format!("Unable to determine order_role");

                        error!("{}", error_msg);
                        bail!("{}", error_msg)
                    }
                }
            }
        }
    }

    fn get_commission_amount(
        event_data: &FillEventData,
        expected_commission_rate: Amount,
        last_fill_amount: Amount,
        last_fill_price: Price,
        commission_currency_code: &CurrencyCode,
        currency_pair_metadata: &CurrencyPairMetadata,
    ) -> Result<Amount> {
        match event_data.commission_amount {
            Some(commission_amount) => Ok(commission_amount.clone()),
            None => {
                let commission_rate = match event_data.commission_rate {
                    Some(commission_rate) => commission_rate.clone(),
                    None => expected_commission_rate,
                };

                let last_fill_amount_in_currency_code = currency_pair_metadata
                    .convert_amount_from_amount_currency_code(
                        commission_currency_code.clone(),
                        last_fill_amount,
                        last_fill_price,
                    )?;
                Ok(last_fill_amount_in_currency_code * commission_rate)
            }
        }
    }

    fn try_set_commission_rate(event_data: &mut FillEventData, expected_commission_rate: Price) {
        if event_data.commission_amount.is_none() && event_data.commission_rate.is_none() {
            event_data.commission_rate = Some(expected_commission_rate);
        }
    }

    // FIXME unexpected? What's the better name?
    fn calculate_commission_data_for_unexpected_currency_code(
        &self,
        commission_currency_code: &CurrencyCode,
        currency_pair_metadata: &CurrencyPairMetadata,
        commission_amount: Amount,
        converted_commission_amount: &mut Amount,
        converted_commission_currency_code: &mut CurrencyCode,
    ) -> Result<()> {
        if commission_currency_code != &currency_pair_metadata.base_currency_code
            && commission_currency_code != &currency_pair_metadata.quote_currency_code
        {
            let mut currency_pair = CurrencyPair::from_currency_codes(
                commission_currency_code.clone(),
                currency_pair_metadata.quote_currency_code.clone(),
            );
            //match self.top_prices.get(&currency_pair) {
            match self.order_book_top.get(&currency_pair) {
                Some(top_prices) => {
                    let bid = top_prices
                        .bid
                        .as_ref()
                        .context("There are no top bid in order book")?;
                    let price_bnb_quote = bid.price;
                    *converted_commission_amount = commission_amount * price_bnb_quote;
                    *converted_commission_currency_code =
                        currency_pair_metadata.quote_currency_code.clone();
                }
                None => {
                    currency_pair = CurrencyPair::from_currency_codes(
                        currency_pair_metadata.quote_currency_code.clone(),
                        commission_currency_code.clone(),
                    );

                    match self.order_book_top.get(&currency_pair) {
                        Some(top_prices) => {
                            let ask = top_prices
                                .ask
                                .as_ref()
                                .context("There are no top ask in order book")?;
                            let price_quote_bnb = ask.price;
                            *converted_commission_amount = commission_amount / price_quote_bnb;
                            *converted_commission_currency_code =
                                currency_pair_metadata.quote_currency_code.clone();
                        }
                        None => error!(
                            "Top bids and asks for {} and currency pair {:?} do not exist",
                            self.exchange_account_id, currency_pair
                        ),
                    }
                }
            }
        }

        Ok(())
    }

    fn set_average_order_fill_price(
        fills: &Vec<OrderFill>,
        currency_pair_metadata: &CurrencyPairMetadata,
        order_filled_amount: Amount,
        order_ref: &OrderRef,
    ) -> Result<()> {
        let mut order_fills_cost_sum = dec!(0);
        fills
            .iter()
            .for_each(|fill| order_fills_cost_sum += fill.cost());
        let average_fill_price = if !currency_pair_metadata.is_derivative() {
            order_fills_cost_sum / order_filled_amount
        } else {
            order_filled_amount / order_fills_cost_sum
        };

        let order_average_fill_price =
            currency_pair_metadata.price_round(average_fill_price, Round::ToNearest)?;
        order_ref.fn_mut(|order| {
            order.internal_props.average_fill_price = order_average_fill_price;
        });

        Ok(())
    }

    fn check_fill_amounts_comformity(
        &self,
        order_filled_amount: Amount,
        order_ref: &OrderRef,
    ) -> Result<()> {
        if order_filled_amount > order_ref.amount() {
            let error_msg = format!(
                "filled_amount {} > order.amount {} for {} {} {:?}",
                order_filled_amount,
                order_ref.amount(),
                self.exchange_account_id,
                order_ref.client_order_id(),
                order_ref.exchange_order_id(),
            );

            error!("{}", error_msg);
            bail!("{}", error_msg)
        }

        Ok(())
    }

    fn send_order_filled_event(
        &self,
        event_data: &FillEventData,
        order_ref: &OrderRef,
        order_fill: &OrderFill,
    ) -> Result<()> {
        self.add_event_on_order_change(order_ref, OrderEventType::OrderFilled)
            .context("Unable to send event, probably receiver is dead already")?;

        info!(
            "Added a fill {} {} {} {:?} {:?}",
            self.exchange_account_id,
            event_data.trade_id,
            order_ref.client_order_id(),
            order_ref.exchange_order_id(),
            order_fill
        );

        Ok(())
    }

    fn react_if_order_completed(
        &self,
        order_filled_amount: Amount,
        order_ref: &OrderRef,
    ) -> Result<()> {
        if order_filled_amount == order_ref.amount() {
            order_ref.fn_mut(|order| {
                order.set_status(OrderStatus::Completed, Utc::now());
            });
            self.add_event_on_order_change(order_ref, OrderEventType::OrderCompleted)
                .context("Unable to send event, probably receiver is dead already")?;
        }

        Ok(())
    }

    fn add_fill(
        &self,
        event_data: &FillEventData,
        currency_pair_metadata: &CurrencyPairMetadata,
        order_ref: &OrderRef,
        converted_commission_currency_code: &CurrencyCode,
        last_fill_amount: Amount,
        last_fill_price: Price,
        last_fill_cost: Price,
        expected_commission_rate: Price,
        commission_amount: Amount,
        order_role: OrderRole,
        commission_currency_code: &CurrencyCode,
        converted_commission_amount: Amount,
    ) -> Result<OrderFill> {
        let last_fill_amount_in_converted_commission_currency_code = currency_pair_metadata
            .convert_amount_from_amount_currency_code(
                converted_commission_currency_code.clone(),
                last_fill_amount,
                last_fill_price,
            )?;
        let expected_converted_commission_amount =
            last_fill_amount_in_converted_commission_currency_code * expected_commission_rate;

        let proportion_multiplier = dec!(0.01);
        let referral_reward_amount = commission_amount
            * self
                .commission
                .get_commission(Some(order_role))?
                .referral_reward
            * proportion_multiplier;

        let rounded_fill_price =
            currency_pair_metadata.price_round(last_fill_price, Round::ToNearest)?;
        let order_fill = OrderFill::new(
            // FIXME what to do with it? Does it even use in C#?
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some(event_data.trade_id.clone()),
            rounded_fill_price,
            last_fill_amount,
            last_fill_cost,
            order_role.into(),
            commission_currency_code.clone(),
            commission_amount,
            referral_reward_amount,
            converted_commission_currency_code.clone(),
            converted_commission_amount,
            expected_converted_commission_amount,
            event_data.is_diff,
            None,
            None,
        );
        order_ref.fn_mut(|order| order.add_fill(order_fill.clone()));

        Ok(order_fill)
    }

    fn local_order_exist(
        &self,
        mut event_data: &mut FillEventData,
        order_ref: &OrderRef,
    ) -> Result<()> {
        let (order_fills, order_filled_amount) = order_ref.get_fills();

        if Self::was_trade_already_received(&event_data.trade_id, &order_fills, &order_ref) {
            return Ok(());
        }

        if Self::diff_fill_after_non_diff(&event_data, &order_fills, &order_ref) {
            return Ok(());
        }

        if Self::filled_amount_not_less_event_fill(&event_data, order_filled_amount, &order_ref) {
            return Ok(());
        }

        let currency_pair_metadata = self.get_currency_pair_metadata(&order_ref.currency_pair())?;
        let (last_fill_price, last_fill_amount, last_fill_cost) = match Self::get_last_fill_data(
            &mut event_data,
            &currency_pair_metadata,
            &order_fills,
            order_filled_amount,
            order_ref,
        )? {
            Some(last_fill_data) => last_fill_data,
            None => return Ok(()),
        };

        if Self::should_miss_fill(
            &event_data,
            order_filled_amount,
            last_fill_amount,
            &order_ref,
        ) {
            return Ok(());
        }

        Self::wrong_status_or_cancelled(&*order_ref, &event_data)?;

        info!("Received fill {:?}", event_data);

        let commission_currency_code = match &event_data.commission_currency_code {
            Some(commission_currency_code) => commission_currency_code.clone(),
            None => currency_pair_metadata.get_commision_currency_code(order_ref.side()),
        };

        let order_role = Self::get_order_role(event_data, order_ref)?;

        let proportion_multiplier = dec!(0.01);
        let expected_commission_rate =
            self.commission.get_commission(Some(order_role))?.fee * proportion_multiplier;

        Self::try_set_commission_rate(&mut event_data, expected_commission_rate);

        let commission_amount = Self::get_commission_amount(
            &event_data,
            expected_commission_rate,
            last_fill_amount,
            last_fill_price,
            &commission_currency_code,
            &currency_pair_metadata,
        )?;

        let mut converted_commission_currency_code = commission_currency_code.clone();
        let mut converted_commission_amount = commission_amount;

        self.calculate_commission_data_for_unexpected_currency_code(
            &commission_currency_code,
            &currency_pair_metadata,
            commission_amount,
            &mut converted_commission_amount,
            &mut converted_commission_currency_code,
        )?;

        let order_fill = self.add_fill(
            &event_data,
            &currency_pair_metadata,
            &order_ref,
            &converted_commission_currency_code,
            last_fill_amount,
            last_fill_price,
            last_fill_cost,
            expected_commission_rate,
            commission_amount,
            order_role,
            &commission_currency_code,
            converted_commission_amount,
        )?;

        // This order fields updated, so let's use actual values
        let (order_fills, order_filled_amount) = order_ref.get_fills();

        Self::set_average_order_fill_price(
            &order_fills,
            &currency_pair_metadata,
            order_filled_amount,
            order_ref,
        )?;

        self.check_fill_amounts_comformity(order_filled_amount, &order_ref)?;

        self.send_order_filled_event(&event_data, &order_ref, &order_fill)?;

        if event_data.source_type == EventSourceType::RestFallback {
            // TODO some metrics
        }

        self.react_if_order_completed(order_filled_amount, &order_ref)?;

        // TODO DataRecorder.save(order)

        Ok(())
    }

    fn check_based_on_fill_type(
        &self,
        event_data: &mut FillEventData,
        args_to_log: &ArgsToLog,
    ) -> Result<()> {
        if event_data.fill_type == OrderFillType::Liquidation
            || event_data.fill_type == OrderFillType::ClosePosition
        {
            if event_data.fill_type == OrderFillType::Liquidation
                && event_data.trade_currency_pair.is_none()
            {
                Self::log_fill_handling_error_and_propagate(
                    "Currency pair should be set for liquidation trade",
                    &args_to_log,
                )?;
            }

            if event_data.order_side.is_none() {
                Self::log_fill_handling_error_and_propagate(
                    "Side should be set for liquidatioin or close position trade",
                    &args_to_log,
                )?;
            }

            if event_data.client_order_id.is_some() {
                Self::log_fill_handling_error_and_propagate(
                    "Client order id cannot be set for liquidation or close position trade",
                    &args_to_log,
                )?;
            }

            if event_data.order_amount.is_none() {
                Self::log_fill_handling_error_and_propagate(
                    "Order amount should be set for liquidation or close position trade",
                    &args_to_log,
                )?;
            }

            match self
                .orders
                .by_exchange_id
                .get(&event_data.exchange_order_id)
            {
                Some(order) => {
                    event_data.client_order_id = Some(order.client_order_id());
                }
                None => {
                    // Liquidation and ClosePosition are always Takers
                    let order_instance = self.create_order_instance(event_data, OrderRole::Taker);

                    event_data.client_order_id =
                        Some(order_instance.header.client_order_id.clone());
                    self.handle_create_order_succeeded(
                        &self.exchange_account_id,
                        &order_instance.header.client_order_id,
                        &event_data.exchange_order_id,
                        &event_data.source_type,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn create_order_instance(
        &self,
        event_data: &FillEventData,
        order_role: OrderRole,
    ) -> OrderSnapshot {
        let currency_pair = event_data
            .trade_currency_pair
            .clone()
            .expect("Impossible situation: currency pair are checked above already");
        let order_amount = event_data
            .order_amount
            .clone()
            .expect("Impossible situation: amount are checked above already");
        let order_side = event_data
            .order_side
            .clone()
            .expect("Impossible situation: order_side are checked above already");

        let client_order_id = ClientOrderId::unique_id();

        let order_instance = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(order_role),
            self.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        self.orders
            .add_snapshot_initial(Arc::new(RwLock::new(order_instance.clone())));

        order_instance
    }

    fn log_fill_handling_error_and_propagate(
        template: &str,
        args_to_log: &(
            ExchangeAccountId,
            String,
            Option<ClientOrderId>,
            ExchangeOrderId,
            AllowedEventSourceType,
            EventSourceType,
        ),
    ) -> Result<()> {
        let error_msg = format!("{} {:?}", template, args_to_log);

        error!("{}", error_msg);
        bail!("{}", error_msg)
    }

    fn should_ignore_event(
        allowed_event_source_type: AllowedEventSourceType,
        source_type: EventSourceType,
    ) -> bool {
        if allowed_event_source_type == AllowedEventSourceType::FallbackOnly
            && source_type != EventSourceType::RestFallback
        {
            return true;
        }

        if allowed_event_source_type == AllowedEventSourceType::NonFallback
            && source_type != EventSourceType::Rest
            && source_type != EventSourceType::WebSocket
        {
            return true;
        }

        return false;
    }
}

#[cfg(test)]
mod test {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::core::{
        exchanges::binance::binance::Binance, exchanges::common::CurrencyCode,
        exchanges::general::commission::Commission,
        exchanges::general::commission::CommissionForType,
        exchanges::general::currency_pair_metadata::PrecisionType,
        exchanges::general::exchange::OrderBookTop, exchanges::general::exchange::PriceLevel,
        exchanges::general::features::ExchangeFeatures,
        exchanges::general::features::OpenOrdersType, orders::event::OrderEvent,
        orders::fill::OrderFill, orders::order::OrderExecutionType, orders::order::OrderFillRole,
        orders::order::OrderFills, orders::order::OrderHeader, orders::order::OrderSimpleProps,
        orders::order::OrderStatusHistory, orders::order::SystemInternalOrderProps,
        orders::pool::OrdersPool, settings,
    };
    use std::sync::mpsc::{channel, Receiver};

    fn get_test_exchange(is_derivative: bool) -> (Arc<Exchange>, Receiver<OrderEvent>) {
        let exchange_account_id = ExchangeAccountId::new("local_exchange_account_id".into(), 0);
        let settings = settings::ExchangeSettings::new(
            exchange_account_id.clone(),
            "test_api_key".into(),
            "test_secret_key".into(),
            false,
        );

        let binance = Binance::new(settings, "Binance0".parse().expect("in test"));
        let refferal_reward = dec!(40);
        let commission = Commission::new(
            CommissionForType::new(dec!(0.1), refferal_reward),
            CommissionForType::new(dec!(0.2), refferal_reward),
        );

        let (tx, rx) = channel();
        let exchange = Exchange::new(
            exchange_account_id,
            "host".into(),
            vec![],
            vec![],
            Box::new(binance),
            ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                false,
                true,
                AllowedEventSourceType::default(),
            ),
            tx,
            commission,
        );
        let base_currency_code = "PHB";
        let quote_currency_code = "BTC";
        let amount_currency_code = if is_derivative {
            quote_currency_code.clone()
        } else {
            base_currency_code.clone()
        };

        let specific_currency_pair = "PHBBTC";
        let price_precision = 0;
        let amount_precision = 0;
        let price_tick = dec!(0.1);
        let symbol = CurrencyPairMetadata::new(
            false,
            is_derivative,
            base_currency_code.into(),
            base_currency_code.into(),
            quote_currency_code.into(),
            quote_currency_code.into(),
            specific_currency_pair.into(),
            None,
            None,
            price_precision,
            PrecisionType::ByFraction,
            Some(price_tick),
            amount_currency_code.into(),
            None,
            None,
            amount_precision,
            PrecisionType::ByFraction,
            None,
            None,
            None,
        );
        exchange.symbols.lock().push(Arc::new(symbol));

        (exchange, rx)
    }

    mod liquidation {
        use super::*;

        #[test]
        fn empty_currency_pair() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: None,
                order_side: None,
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange(false);
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Currency pair should be set for liquidation trade",
                        &error.to_string()[..49]
                    );
                }
            }
        }

        #[test]
        fn empty_order_side() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: None,
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange(false);
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Side should be set for liquidatioin or close position trade",
                        &error.to_string()[..59]
                    );
                }
            }
        }

        #[test]
        fn not_empty_client_order_id() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: Some(ClientOrderId::unique_id()),
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: Some(OrderSide::Buy),
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange(false);
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Client order id cannot be set for liquidation or close position trade",
                        &error.to_string()[..69]
                    );
                }
            }
        }

        #[test]
        fn not_empty_order_amount() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: Some(OrderSide::Buy),
                order_amount: None,
            };

            let (exchange, _) = get_test_exchange(false);
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Order amount should be set for liquidation or close position trade",
                        &error.to_string()[..66]
                    );
                }
            }
        }

        #[test]
        fn should_add_order() {
            let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
            let order_side = OrderSide::Buy;
            let order_amount = dec!(12);
            let order_role = None;
            let fill_price = dec!(0.2);
            let fill_amount = dec!(5);

            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("test".into()),
                fill_price,
                fill_amount,
                is_diff: false,
                total_filled_amount: None,
                order_role,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(currency_pair.clone()),
                order_side: Some(order_side),
                order_amount: Some(order_amount),
            };

            let (exchange, _event_received) = get_test_exchange(false);
            match exchange.handle_order_filled(event_data) {
                Ok(_) => {
                    let order = exchange
                        .orders
                        .by_client_id
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
                    assert_eq!(filled_amount, fill_amount);
                    assert_eq!(fills.iter().next().expect("in test").price(), fill_price);
                }
                Err(_) => assert!(false),
            }
        }

        #[test]
        fn empty_exchange_order_id() {
            let event_data = FillEventData {
                source_type: EventSourceType::WebSocket,
                trade_id: String::new(),
                client_order_id: None,
                exchange_order_id: ExchangeOrderId::new("".into()),
                fill_price: dec!(0),
                fill_amount: dec!(0),
                is_diff: false,
                total_filled_amount: None,
                order_role: None,
                commission_currency_code: None,
                commission_rate: None,
                commission_amount: None,
                fill_type: OrderFillType::Liquidation,
                trade_currency_pair: Some(CurrencyPair::from_currency_codes(
                    "te".into(),
                    "st".into(),
                )),
                order_side: Some(OrderSide::Buy),
                order_amount: Some(dec!(0)),
            };

            let (exchange, _event_receiver) = get_test_exchange(false);
            match exchange.handle_order_filled(event_data) {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Received HandleOrderFilled with an empty exchangeOrderId",
                        &error.to_string()[..56]
                    );
                }
            }
        }
    }

    #[test]
    fn ignore_if_trade_was_already_received() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();
        let fill_amount = dec!(0.2);

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(CurrencyPair::from_currency_codes("te".into(), "st".into())),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some(trade_id),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, fill_amount);
    }

    #[test]
    fn ignore_diff_fill_after_non_diff() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let fill_amount = dec!(0.2);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(CurrencyPair::from_currency_codes("te".into(), "st".into())),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some("different_trade_id".to_owned()),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, fill_amount);
    }

    #[test]
    fn ignore_filled_amount_not_less_event_fill() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("te".into(), "st".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let fill_amount = dec!(0.2);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(CurrencyPair::from_currency_codes("te".into(), "st".into())),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some("different_trade_id".to_owned()),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, fill_amount);
    }

    #[test]
    fn ignore_diff_fill_if_filled_amount_is_zero() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let order_price = dec!(1);
        let fill_amount = dec!(0);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.2),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let cost = dec!(0);
        let order_fill = OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::Liquidation,
            Some("different_trade_id".to_owned()),
            order_price,
            fill_amount,
            cost,
            OrderFillRole::Taker,
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            CurrencyCode::new("test".into()),
            dec!(0),
            dec!(0),
            true,
            None,
            None,
        );
        order.add_fill(order_fill);
        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange
            .local_order_exist(&mut event_data, &*order_ref)
            .expect("in test");

        let (_, order_filled_amount) = order_ref.get_fills();
        assert_eq!(order_filled_amount, dec!(0));
    }

    #[test]
    fn error_if_order_status_is_failed_to_create() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.2),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.set_status(OrderStatus::FailedToCreate, Utc::now());

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!(
                    "Fill was received for a FailedToCreate false",
                    &error.to_string()[..44]
                );
            }
        }
    }

    #[test]
    fn error_if_order_status_is_completed() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.2),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.set_status(OrderStatus::Completed, Utc::now());

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!(
                    "Fill was received for a Completed false",
                    &error.to_string()[..39]
                );
            }
        }
    }

    #[test]
    fn error_if_cancellation_event_was_raised() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(1);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();
        let fill_price = dec!(0.2);

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price,
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.internal_props.cancellation_event_was_raised = true;

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                // TODO has to be Created!
                // Does it mean order status had to be changed somewhere?
                assert_eq!(
                    "Fill was received for a Creating true",
                    &error.to_string()[..37]
                );
            }
        }
    }

    // TODO Can be improved via testing only calculate_cost_diff_function
    #[test]
    fn calculate_cost_diff_on_buy_side() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::new(
            client_order_id.clone(),
            Utc::now(),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            OrderType::Limit,
            OrderSide::Buy,
            order_amount,
            OrderExecutionType::None,
            None,
            None,
            None,
        );
        let props = OrderSimpleProps::new(
            Some(order_price),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            Default::default(),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            Arc::new(header),
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
        );

        exchange
            .orders
            .try_add_snapshot_by_exchange_id(Arc::new(RwLock::new(order)));

        let first_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.2),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(first_event_data)
            .expect("in test");

        let second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "another_trade_id".to_owned(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.3),
            fill_amount: dec!(10),
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(second_event_data)
            .expect("in test");

        let order_ref = exchange
            .orders
            .by_exchange_id
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
    #[test]
    fn calculate_cost_diff_on_sell_side() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::new(
            client_order_id.clone(),
            Utc::now(),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            OrderType::Limit,
            OrderSide::Sell,
            order_amount,
            OrderExecutionType::None,
            None,
            None,
            None,
        );
        let props = OrderSimpleProps::new(
            Some(order_price),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            Default::default(),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            Arc::new(header),
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
        );

        exchange
            .orders
            .try_add_snapshot_by_exchange_id(Arc::new(RwLock::new(order)));

        let first_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.2),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(first_event_data)
            .expect("in test");

        let second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "another_trade_id".to_owned(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.3),
            fill_amount: dec!(10),
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(second_event_data)
            .expect("in test");

        let order_ref = exchange
            .orders
            .by_exchange_id
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

    #[test]
    fn calculate_cost_diff_on_buy_side_derivative() {
        let (exchange, _event_receiver) = get_test_exchange(true);

        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::new(
            client_order_id.clone(),
            Utc::now(),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            OrderType::Limit,
            OrderSide::Buy,
            order_amount,
            OrderExecutionType::None,
            None,
            None,
            None,
        );
        let props = OrderSimpleProps::new(
            Some(order_price),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            Default::default(),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            Arc::new(header),
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
        );

        exchange
            .orders
            .try_add_snapshot_by_exchange_id(Arc::new(RwLock::new(order)));

        let first_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(2000),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(first_event_data)
            .expect("in test");

        let second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "another_trade_id".to_owned(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(3000),
            fill_amount: dec!(10),
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(second_event_data)
            .expect("in test");

        let order_ref = exchange
            .orders
            .by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");

        let (fills, filled_amount) = order_ref.get_fills();

        assert_eq!(filled_amount, dec!(10));
        assert_eq!(order_ref.internal_props().average_fill_price, dec!(3000));
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

    // FIXME Why do we need tests like this?
    // Nothing depends on order.side as I can see
    #[test]
    fn calculate_cost_diff_on_sell_side_derivative() {
        let (exchange, _event_receiver) = get_test_exchange(true);

        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::new(
            client_order_id.clone(),
            Utc::now(),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            OrderType::Limit,
            OrderSide::Sell,
            order_amount,
            OrderExecutionType::None,
            None,
            None,
            None,
        );
        let props = OrderSimpleProps::new(
            Some(order_price),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            Default::default(),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            Arc::new(header),
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
        );

        exchange
            .orders
            .try_add_snapshot_by_exchange_id(Arc::new(RwLock::new(order)));

        let first_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(2000),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(first_event_data)
            .expect("in test");

        let second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "another_trade_id".to_owned(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(3000),
            fill_amount: dec!(10),
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(second_event_data)
            .expect("in test");

        let order_ref = exchange
            .orders
            .by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");

        let (fills, filled_amount) = order_ref.get_fills();

        assert_eq!(filled_amount, dec!(10));
        assert_eq!(order_ref.internal_props().average_fill_price, dec!(3000));
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

    #[test]
    fn ignore_non_diff_fill_with_second_cost_lesser() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let client_order_id = ClientOrderId::unique_id();
        let order_side = OrderSide::Buy;
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;
        let exchange_order_id: ExchangeOrderId = "some_order_id".into();

        // Add order manually for setting custom order.amount
        let header = OrderHeader::new(
            client_order_id.clone(),
            Utc::now(),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            OrderType::Limit,
            OrderSide::Sell,
            order_amount,
            OrderExecutionType::None,
            None,
            None,
            None,
        );
        let props = OrderSimpleProps::new(
            Some(order_price),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            Default::default(),
            Default::default(),
            None,
        );
        let order = OrderSnapshot::new(
            Arc::new(header),
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
        );

        exchange
            .orders
            .try_add_snapshot_by_exchange_id(Arc::new(RwLock::new(order)));

        let first_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.01)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(first_event_data)
            .expect("in test");

        let second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "another_trade_id".to_owned(),
            client_order_id: None,
            exchange_order_id: exchange_order_id.clone(),
            fill_price: dec!(0.3),
            fill_amount: dec!(10),
            is_diff: false,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(dec!(0.03)),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        exchange
            .handle_order_filled(second_event_data)
            .expect("in test");

        let order_ref = exchange
            .orders
            .by_exchange_id
            .get(&exchange_order_id)
            .expect("in test");

        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);
    }

    #[test]
    fn ignore_fill_if_total_filled_amount_is_incorrect() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(1);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: Some(dec!(9)),
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert!(fills.is_empty());
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn take_roll_from_fill_if_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Taker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                let right_value = dec!(0.2) / dec!(100) * dec!(5);
                assert_eq!(fill.commission_amount(), right_value);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn take_roll_from_order_if_not_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                let right_value = dec!(0.1) / dec!(100) * dec!(5);
                assert_eq!(fill.commission_amount(), right_value);
            }
            Err(_) => {
                assert!(false);
            }
        }
    }

    #[test]
    fn error_if_unable_to_get_role() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            None,
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match Exchange::get_order_role(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!(
                    "Fill has neither commission nor commission rate",
                    &error.to_string()[..47]
                );
            }
        }
    }

    #[test]
    fn use_commission_currency_code_from_event_data() -> Result<()> {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let commission_currency_code = CurrencyCode::new("BTC".into());

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: Some(commission_currency_code.clone()),
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange.local_order_exist(&mut event_data, &*order_ref)?;
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let fill = &fills[0];
        assert_eq!(
            fill.converted_commission_currency_code(),
            &commission_currency_code
        );

        Ok(())
    }

    #[test]
    fn commission_currency_code_from_base_currency_code() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let base_currency_code = CurrencyCode::new("PHB".into());

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                assert_eq!(
                    fill.converted_commission_currency_code(),
                    &base_currency_code
                );
            }
            Err(_) => {
                assert!(false);
            }
        }
    }

    #[test]
    fn commission_currency_code_from_quote_currency_code() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Sell;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let quote_currency_code = exchange
                    .symbols
                    .lock()
                    .first()
                    .expect("")
                    .quote_currency_code
                    .clone();

                let fill = &fills[0];
                assert_eq!(
                    fill.converted_commission_currency_code(),
                    &quote_currency_code
                );
            }
            Err(_) => {
                assert!(false);
            }
        }
    }

    #[test]
    fn use_commission_amount_if_specified() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Sell;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let commission_amount = dec!(0.001);

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(commission_amount),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let first_fill = &fills[0];
                assert_eq!(first_fill.commission_amount(), commission_amount);
            }
            Err(_) => {
                assert!(false);
            }
        }
    }

    #[test]
    fn use_commission_rate_if_specified() -> Result<()> {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Sell;
        let fill_price = dec!(0.8);
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let commission_rate = dec!(0.3) / dec!(100);

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: fill_price.clone(),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: Some("BTC".into()),
            commission_rate: Some(commission_rate),
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange.local_order_exist(&mut event_data, &*order_ref)?;
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let first_fill = &fills[0];
        let result_value = commission_rate * fill_price * fill_amount;
        assert_eq!(first_fill.commission_amount(), result_value);

        Ok(())
    }

    #[test]
    fn calculate_commission_rate_if_not_specified() -> Result<()> {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Sell;
        let fill_price = dec!(0.8);
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: fill_price.clone(),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: Some("BTC".into()),
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange.local_order_exist(&mut event_data, &*order_ref)?;
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let first_fill = &fills[0];
        let result_value = dec!(0.1) / dec!(100) * fill_price * fill_amount;
        assert_eq!(first_fill.commission_amount(), result_value);

        Ok(())
    }

    #[test]
    fn calculate_commission_amount() -> Result<()> {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.8);
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("".into()),
            fill_price: fill_price.clone(),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: None,
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let mut order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            dec!(0.2),
            order_amount,
            order_side,
            None,
        );
        order.fills.filled_amount = dec!(3);

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        exchange.local_order_exist(&mut event_data, &*order_ref)?;
        let (fills, _) = order_ref.get_fills();
        assert_eq!(fills.len(), 1);

        let first_fill = &fills[0];
        let result_value = dec!(0.1) / dec!(100) * fill_amount;
        assert_eq!(first_fill.commission_amount(), result_value);

        Ok(())
    }

    #[test]
    fn get_commission_amount_via_rate() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("some_exchange_order_id".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(OrderSide::Buy),
            order_amount: Some(dec!(0)),
        };

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        exchange
            .orders
            .try_add_snapshot_by_exchange_id(Arc::new(RwLock::new(order.clone())));

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                let right_value = dec!(0.1) / dec!(100) * dec!(5);
                assert_eq!(fill.commission_amount(), right_value);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn get_commission_amount_via_rate_for_sell() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Sell;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("some_exchange_order_id".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        exchange
            .orders
            .try_add_snapshot_by_exchange_id(Arc::new(RwLock::new(order.clone())));

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                let right_value = dec!(0.1) / dec!(100) * dec!(5) * dec!(0.8);
                assert_eq!(fill.commission_amount(), right_value);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn get_commission_amount_via_rate_for_buy() {
        let (exchange, _event_receiver) = get_test_exchange(true);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("some_exchange_order_id".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                let right_value = dec!(0.1) / dec!(100) * dec!(5) / dec!(0.8);
                assert_eq!(fill.commission_amount(), right_value);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn expected_commission_amount_equal_commission_amount() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let commission_amount = dec!(0.1) / dec!(100) * dec!(5);

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("some_exchange_order_id".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(commission_amount),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                assert_eq!(fill.commission_amount(), commission_amount);
                assert_eq!(
                    fill.expected_converted_commission_amount(),
                    commission_amount
                );
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn expected_commission_amount_not_equal_wrong_commission_amount() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();
        let commission_amount = dec!(1000);

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("some_exchange_order_id".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: Some(commission_amount),
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                assert_eq!(fill.commission_amount(), commission_amount);
                let right_value = dec!(0.1) / dec!(100) * dec!(5);
                assert_eq!(fill.expected_converted_commission_amount(), right_value);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn refferal_reward_percentage_from_commissions() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_amount = dec!(5);
        let order_amount = dec!(12);
        let trade_id = "test_trade_id".to_owned();

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: trade_id.clone(),
            client_order_id: None,
            exchange_order_id: ExchangeOrderId::new("some_exchange_order_id".into()),
            fill_price: dec!(0.8),
            fill_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair,
            event_data.fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let fill = &fills[0];
                let right_value = dec!(5) * dec!(0.1) / dec!(100) * dec!(0.4);
                assert_eq!(fill.referral_reward_amount(), right_value);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn filled_amount_from_zero_to_completed() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.8);
        let order_amount = dec!(12);
        let exchange_account_id = ExchangeOrderId::new("some_echange_order_id".into());
        let client_account_id = ClientOrderId::unique_id();

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "first_trend_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price,
            fill_amount: dec!(5),
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (_, filled_amount) = order_ref.get_fills();

                let current_right_filled_amount = dec!(5);
                assert_eq!(filled_amount, current_right_filled_amount);
            }
            Err(_) => assert!(false),
        }

        let mut second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "second_trade_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price,
            fill_amount: dec!(2),
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut second_event_data, &*order_ref) {
            Ok(_) => {
                let (_, filled_amount) = order_ref.get_fills();

                let right_filled_amount = dec!(7);
                assert_eq!(filled_amount, right_filled_amount);
            }
            Err(_) => assert!(false),
        }

        let mut second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "third_trade_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price,
            fill_amount: dec!(5),
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut second_event_data, &*order_ref) {
            Ok(_) => {
                let (_, filled_amount) = order_ref.get_fills();

                let right_filled_amount = dec!(12);
                assert_eq!(filled_amount, right_filled_amount);

                let order_status = order_ref.status();
                assert_eq!(order_status, OrderStatus::Completed);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn too_big_filled_amount() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.8);
        let order_amount = dec!(12);
        let exchange_account_id = ExchangeOrderId::new("some_echange_order_id".into());
        let client_account_id = ClientOrderId::unique_id();

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "first_trend_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price,
            fill_amount: dec!(13),
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!(
                    "filled_amount 13 > order.amount 12 for",
                    &error.to_string()[..38]
                );
            }
        }
    }

    fn average_fill_price() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.8);
        let order_amount = dec!(12);
        let exchange_account_id = ExchangeOrderId::new("some_echange_order_id".into());
        let client_account_id = ClientOrderId::unique_id();

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "first_trend_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price: dec!(0.2),
            fill_amount: dec!(5),
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let right_average_fill_price = dec!(0.2);
                let calculated = order_ref.internal_props().average_fill_price;
                assert_eq!(calculated, right_average_fill_price);
            }
            Err(_) => assert!(false),
        }

        let mut second_event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "second_trade_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price: dec!(0.4),
            fill_amount: dec!(5),
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut second_event_data, &*order_ref) {
            Ok(_) => {
                let right_average_fill_price = dec!(0.3);
                let calculated = order_ref.internal_props().average_fill_price;
                assert_eq!(calculated, right_average_fill_price);
            }
            Err(_) => assert!(false),
        }
    }

    #[test]
    fn last_fill_receive_time() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.2);
        let order_amount = dec!(12);
        let exchange_account_id = ExchangeOrderId::new("some_echange_order_id".into());
        let client_account_id = ClientOrderId::unique_id();

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "first_trend_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price,
            fill_amount: dec!(5),
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let (fills, _filled_amount) = order_ref.get_fills();
                assert_eq!(fills.len(), 1);

                let first_fill = &fills[0];
                // FIXME Is that LastFillDateTime from C#?
                let receive_time = first_fill.receive_time().timestamp_millis();
                let current_time = Utc::now().timestamp_millis();
                assert_eq!(current_time, receive_time);
            }
            Err(_) => {
                assert!(false);
            }
        }
    }

    #[test]
    fn order_completed_if_filled_completely() {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let order_side = OrderSide::Buy;
        let fill_price = dec!(0.2);
        let order_amount = dec!(12);
        let exchange_account_id = ExchangeOrderId::new("some_echange_order_id".into());
        let client_account_id = ClientOrderId::unique_id();

        let order = OrderSnapshot::with_params(
            client_order_id.clone(),
            OrderType::Liquidation,
            Some(OrderRole::Maker),
            exchange.exchange_account_id.clone(),
            currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
            None,
        );

        let order_pool = OrdersPool::new();
        order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        let order_ref = order_pool
            .by_client_id
            .get(&client_order_id)
            .expect("in test");

        let mut event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: "first_trend_id".into(),
            client_order_id: Some(client_account_id.clone()),
            exchange_order_id: exchange_account_id.clone(),
            fill_price,
            fill_amount: order_amount,
            is_diff: true,
            total_filled_amount: None,
            order_role: Some(OrderRole::Maker),
            commission_currency_code: None,
            commission_rate: None,
            commission_amount: None,
            fill_type: OrderFillType::Liquidation,
            trade_currency_pair: Some(currency_pair.clone()),
            order_side: Some(order_side),
            order_amount: Some(dec!(0)),
        };

        match exchange.local_order_exist(&mut event_data, &*order_ref) {
            Ok(_) => {
                let order_status = order_ref.status();
                assert_eq!(order_status, OrderStatus::Completed);
            }
            Err(_) => {
                assert!(false);
            }
        }
    }

    #[test]
    fn converted_commission_amount_to_quote_when_bnb_case() -> Result<()> {
        let (exchange, _event_receiver) = get_test_exchange(false);

        let commission_currency_code = CurrencyCode::new("BNB".into());
        let currency_pair_metadata = exchange.symbols.lock()[0].clone();
        let commission_amount = dec!(15);
        let mut converted_commission_amount = dec!(4.5);
        let mut converted_commission_currency_code = CurrencyCode::new("BTC".into());

        let currency_pair = CurrencyPair::from_currency_codes(
            commission_currency_code.clone(),
            currency_pair_metadata.quote_currency_code.clone(),
        );
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

        exchange.calculate_commission_data_for_unexpected_currency_code(
            &commission_currency_code,
            &currency_pair_metadata,
            commission_amount,
            &mut converted_commission_amount,
            &mut converted_commission_currency_code,
        )?;

        let right_amount = dec!(4.5);
        assert_eq!(converted_commission_amount, right_amount);

        let right_currency_code = CurrencyCode::new("BTC".into());
        assert_eq!(converted_commission_currency_code, right_currency_code);

        Ok(())
    }
}
