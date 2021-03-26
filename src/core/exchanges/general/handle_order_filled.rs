use super::exchange::Exchange;
use crate::core::{
    exchanges::common::Amount, exchanges::common::ExchangeAccountId,
    exchanges::events::AllowedEventSourceType, orders::fill::EventSourceType,
    orders::fill::OrderFillType, orders::order::ClientOrderId, orders::order::ExchangeOrderId,
    orders::order::OrderSide,
};
use anyhow::{bail, Result};
use log::{error, info, warn};

type ArgsToLog = (
    ExchangeAccountId,
    String,
    Option<ClientOrderId>,
    ExchangeOrderId,
    AllowedEventSourceType,
    EventSourceType,
);

pub struct FillEventData {
    source_type: EventSourceType,
    trade_id: String,
    client_order_id: Option<ClientOrderId>,
    exchange_order_id: ExchangeOrderId,
    fill_type: OrderFillType,
    // FIXME Different type? Option<CurrencyPair> maybe?
    trade_currency_pair: String,
    order_side: Option<OrderSide>,
    order_amount: Option<Amount>,
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
            if event_data.fill_type == OrderFillType::Liquidation {
                Self::log_fill_handling_error_and_propagate(
                    "Currency pair should be set for liquidation trade",
                    &args_to_log,
                )?;
            }

            // FIXME What about order_side == OrderSide::Unknown?
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
                    // FIXME there are not "should" in C#
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
                    // FIXME continue from here
                    //let order_instance = create_order_instance();
                    //event_data.client_order_id = Some(order_instance.client_order_id);
                    //self.handle_create_order_succeeded(
                    //    &self.exchange_account_id,
                    //    order_instance.client_order_id,
                    //    &event_data.exchange_order_id,
                    //    &event_data.source_type,
                    //);
                }
            }
        }

        Ok(())
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
