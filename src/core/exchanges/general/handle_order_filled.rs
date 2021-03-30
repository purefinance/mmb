use std::sync::Arc;

use super::exchange::Exchange;
use crate::core::{
    exchanges::common::Amount, exchanges::common::CurrencyPair,
    exchanges::common::ExchangeAccountId, exchanges::common::Price,
    exchanges::events::AllowedEventSourceType, orders::fill::EventSourceType,
    orders::fill::OrderFillType, orders::order::ClientOrderId, orders::order::ExchangeOrderId,
    orders::order::OrderRole, orders::order::OrderSide, orders::order::OrderSnapshot,
    orders::order::OrderStatus, orders::order::OrderType, orders::pool::OrderRef,
};
use anyhow::{bail, Result};
use log::{error, info, warn};
use parking_lot::RwLock;
use rust_decimal::prelude::Zero;
use rust_decimal_macros::dec;

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
    pub commission_currency_code: Option<String>,
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

        self.check_based_on_fill_type(&mut event_data, &args_to_log)?;

        if event_data.exchange_order_id.as_str().is_empty() {
            Self::log_fill_handling_error_and_propagate(
                "Received HandleOrderFilled with an empty exchangeOrderId",
                &args_to_log,
            )?;
        }

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
            Some(order) => {
                self.local_order_exist(&mut event_data, &*order)?;
            }
        }

        //FIXME handle it in the end
        Ok(())
    }

    fn local_order_exist(&self, event_data: &mut FillEventData, order: &OrderRef) -> Result<()> {
        let (order_fills, order_filled_amound) = order.get_fills();

        if !event_data.trade_id.is_empty()
            && order_fills.iter().any(|fill| {
                std::str::from_utf8(fill.id().as_bytes()).expect("Unable to convert Uuid to &str")
                    == event_data.trade_id
            })
        {
            info!(
                "Trade with {} was received already for order {:?}",
                event_data.trade_id, order
            );

            return Ok(());
        }

        if event_data.is_diff && order_fills.iter().any(|fill| !fill.is_diff()) {
            // Most likely we received a trade update (diff), then received a non-diff fill via fallback and then again received a diff trade update
            // It happens when WebSocket is glitchy and we miss update and the problem is we have no idea how to handle diff updates
            // after applying a non-diff one as there's no TradeId, so we have to ignore all the diff updates afterwards
            // relying only on fallbacks
            warn!(
                "Unable to process a diff fill after a non-diff one {:?}",
                order
            );

            return Ok(());
        }

        if event_data.is_diff && order_filled_amound >= event_data.fill_amount {
            warn!(
                        "order.filled_amount is {} >= received fill {}, so non-diff fill for {} {:?} should be ignored",
                        order_filled_amound,
                        event_data.fill_amount,
                        order.client_order_id(),
                        order.exchange_order_id(),
                    );

            return Ok(());
        }

        let last_fill_amount = event_data.fill_amount;
        let last_fill_price = event_data.fill_price;
        // TODO FIXME implement part connected with symbol

        if !event_data.is_diff && order_fills.len() > 0 {
            // Diff should be calculated only if it is not the first fill
            let mut total_filled_cost = dec!(0);
            order_fills
                .iter()
                .for_each(|fill| total_filled_cost += fill.cost());
            // TODO FIXME implement part connected with symbol
        }

        if last_fill_amount.is_zero() {
            warn!(
                "last_fill_amount was received for 0 for {}, {:?}",
                order.client_order_id(),
                order.exchange_order_id()
            );

            return Ok(());
        }

        if let Some(total_filled_amount) = event_data.total_filled_amount {
            if order_filled_amound + last_fill_amount != total_filled_amount {
                warn!(
                    "Fill was missed because {} != {} for {:?}",
                    order_filled_amound, total_filled_amount, order
                );

                return Ok(());
            }
        }

        if order.status() == OrderStatus::FailedToCreate
            || order.status() == OrderStatus::Completed
            || order.was_cancellation_event_raised()
        {
            let error_msg = format!(
                "Fill was received for a {:?} {} {:?}",
                order.status(),
                order.was_cancellation_event_raised(),
                event_data
            );

            error!("{}", error_msg);
            bail!("{}", error_msg)
        }

        info!("Received fill {:?}", event_data);

        if event_data.commission_currency_code.is_none() {
            // TODO event_data.commission_currency_code = symbol.get_commision_currency_code(order.side());
        }

        if event_data.order_role.is_none() {
            if event_data.commission_amount.is_none()
                && event_data.commission_rate.is_none()
                && order.role().is_none()
            {
                let error_msg = format!(
                    "Fill has neither commission nor comission rate. Order role in order was set too",
                );

                error!("{}", error_msg);
                bail!("{}", error_msg)
            }

            event_data.order_role = order.role();
        }

        // FIXME What is this?
        let some_magical_number = dec!(0.01);
        let expected_commission_rate =
            self.commission.get_commission(event_data.order_role)?.fee * some_magical_number;
        if event_data.commission_amount.is_none() && event_data.commission_rate.is_none() {
            event_data.commission_rate = Some(expected_commission_rate);
        }

        if event_data.commission_amount.is_none() {
            // TODO let last_fill_amount_in_cuurency_code = ...
            // TODO commission_amount = last_fill_amount_in_currency_code * commission_rate;
        }

        let converted_commission_currency_code = event_data.commission_currency_code.clone();
        let converted_commission_amount = event_data.commission_amount;

        // TODO if all about symbol's data

        // FIXME handle it in the end
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
                    let order_instance = self.create_order_instance(event_data);

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

    fn create_order_instance(&self, event_data: &FillEventData) -> OrderSnapshot {
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
            None,
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
    }
}
