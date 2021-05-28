use crate::core::{
    exchanges::common::Amount, exchanges::common::ExchangeAccountId,
    exchanges::events::AllowedEventSourceType, orders::fill::EventSourceType,
    orders::order::ClientOrderId, orders::order::ExchangeOrderId, orders::order::OrderEventType,
    orders::order::OrderStatus, orders::pool::OrderRef,
};
use anyhow::{bail, Result};
use chrono::Utc;
use log::{error, info, warn};

use super::exchange::Exchange;

impl Exchange {
    pub(crate) fn handle_cancel_order_succeeded(
        &self,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
        filled_amount: Option<Amount>,
        source_type: EventSourceType,
    ) -> Result<()> {
        let args_to_log = (
            self.exchange_account_id.clone(),
            exchange_order_id.clone(),
            self.features.allowed_cancel_event_source_type,
            source_type,
        );

        if Self::should_ignore_event(self.features.allowed_cancel_event_source_type, source_type) {
            info!("Ignoring fill {:?}", args_to_log);
            return Ok(());
        }

        if exchange_order_id.as_str().is_empty() {
            Self::log_cancel_handling_error_and_propagate(
                "Received HandleOrderFilled with an empty exchangeOrderId",
                &args_to_log,
            )?;
        }

        match self.orders.cache_by_exchange_id.get(&exchange_order_id) {
            None => {
                // TODO BufferedCanceledOrderManager.add_order(exchange_order_id, self.exchange_account_id)
                // TODO All other code connected BufferedCaceledOrderManager
                Ok(())
            }
            Some(order_ref) => self.try_update_local_order(
                &order_ref,
                filled_amount,
                source_type,
                client_order_id,
                exchange_order_id,
            ),
        }
    }

    fn order_already_closed(
        &self,
        status: OrderStatus,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
    ) -> bool {
        let arg_to_log = match status {
            OrderStatus::Canceled => "Canceled",
            OrderStatus::Completed => "Completed",
            _ => return false,
        };

        warn!(
            "CancelOrderSucceeded received for {} order {} {:?} {}",
            arg_to_log, client_order_id, exchange_order_id, self.exchange_account_id
        );

        true
    }

    fn try_update_local_order(
        &self,
        order_ref: &OrderRef,
        filled_amount: Option<Amount>,
        source_type: EventSourceType,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
    ) -> Result<()> {
        if self.order_already_closed(order_ref.status(), client_order_id, exchange_order_id) {
            return Ok(());
        }

        if source_type == EventSourceType::RestFallback {
            // TODO some metrics
        }

        let mut is_canceling_from_wait_cancel_order = false;
        order_ref.fn_mut(|order| {
            order.internal_props.filled_amount_after_cancellation = filled_amount;
            order.set_status(OrderStatus::Canceled, Utc::now());
            order.internal_props.cancellation_event_source_type = Some(source_type);
            is_canceling_from_wait_cancel_order =
                order.internal_props.is_canceling_from_wait_cancel_order;
        });

        // Here we cover the situation with MakerOnly orders
        // As soon as we created an order, it was automatically canceled
        // Usually we raise CancelOrderSucceeded in WaitCancelOrder after a check for fills via fallback
        // but in this particular case the cancellation is triggered by exchange itself, so WaitCancelOrder was never called
        if !is_canceling_from_wait_cancel_order {
            info!("Adding CancelOrderSucceeded event from handle_cancel_order_succeeded() {} {:?} on {}",
                client_order_id,
                exchange_order_id,
                self.exchange_account_id);

            // Sometimes we start WaitCancelOrder at about the same time when as get an "order was refused/canceled" notification from an exchange (i. e. MakerOnly),
            // and we can Add CancelOrderSucceeded event here (outside WaitCancelOrder) and later from WaitCancelOrder as
            // when we check order.WasFinished in the beginning on WaitCancelOrder, the status is not set to Canceled yet
            // To avoid this situation we set CanceledNotFromWaitCancelOrder to true and then don't raise an event in WaitCancelOrder for the 2nd time
            order_ref.fn_mut(|order| {
                order.internal_props.canceled_not_from_wait_cancel_order = true;
            });

            self.add_event_on_order_change(order_ref, OrderEventType::CancelOrderSucceeded)?;
        }

        info!(
            "Order was successfully cancelled {} {:?} on {}",
            client_order_id, exchange_order_id, self.exchange_account_id
        );

        // TODO DataRecorder.save(order_ref)

        Ok(())
    }

    fn log_cancel_handling_error_and_propagate(
        template: &str,
        args_to_log: &(
            ExchangeAccountId,
            ExchangeOrderId,
            AllowedEventSourceType,
            EventSourceType,
        ),
    ) -> Result<()> {
        let error_msg = format!("{} {:?}", template, args_to_log);

        error!("{}", error_msg);
        bail!("{}", error_msg)
    }
}

#[cfg(test)]
mod test {
    use anyhow::Context;
    use rstest::rstest;
    use rust_decimal_macros::dec;

    use super::*;
    use crate::core::{
        exchanges::common::CurrencyPair, exchanges::general::test_helper, orders::order::OrderRole,
        orders::order::OrderSide,
    };

    #[test]
    fn empty_exchange_order_id() {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = dec!(1);
        let source_type = EventSourceType::Rest;

        let maybe_error = exchange.handle_cancel_order_succeeded(
            &client_order_id,
            &exchange_order_id,
            Some(filled_amount),
            source_type,
        );

        match maybe_error {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!(
                    "Received HandleOrderFilled with an empty exchangeOrderId",
                    &error.to_string()[..56]
                );
            }
        }
    }

    #[rstest]
    #[case(OrderStatus::Completed, true)]
    #[case(OrderStatus::Canceled, true)]
    #[case(OrderStatus::Creating, false)]
    fn order_already_closed(#[case] status: OrderStatus, #[case] expected: bool) {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let exchange_order_id = ExchangeOrderId::new("".into());

        let already_closed =
            exchange.order_already_closed(status, &client_order_id, &exchange_order_id);

        assert_eq!(already_closed, expected);
    }

    #[test]
    fn return_if_order_already_closed() -> Result<()> {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            &exchange.exchange_account_id.clone(),
            &currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
        );
        order_ref.fn_mut(|order| order.set_status(OrderStatus::Completed, Utc::now()));

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        let updating_result = exchange.try_update_local_order(
            &order_ref,
            filled_amount,
            source_type,
            &client_order_id,
            &exchange_order_id,
        );

        assert!(updating_result.is_ok());

        Ok(())
    }

    #[test]
    fn order_filled_amount_cancellation_updated() -> Result<()> {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            &exchange.exchange_account_id.clone(),
            &currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
        );

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        exchange.try_update_local_order(
            &order_ref,
            filled_amount,
            source_type,
            &client_order_id,
            &exchange_order_id,
        )?;

        let changed_amount = order_ref.internal_props().filled_amount_after_cancellation;
        let expected = filled_amount;
        assert_eq!(changed_amount, expected);

        Ok(())
    }

    #[test]
    fn order_status_updated() -> Result<()> {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            &exchange.exchange_account_id.clone(),
            &currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
        );

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        exchange.try_update_local_order(
            &order_ref,
            filled_amount,
            source_type,
            &client_order_id,
            &exchange_order_id,
        )?;

        let order_status = order_ref.status();
        assert_eq!(order_status, OrderStatus::Canceled);

        let order_event_source_type = order_ref
            .internal_props()
            .cancellation_event_source_type
            .expect("in test");
        assert_eq!(order_event_source_type, source_type);

        Ok(())
    }

    #[test]
    fn canceled_not_from_wait_cancel_order() -> Result<()> {
        let (exchange, event_receiver) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_currency_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            &exchange.exchange_account_id.clone(),
            &currency_pair.clone(),
            fill_price,
            order_amount,
            order_side,
        );

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        exchange.try_update_local_order(
            &order_ref,
            filled_amount,
            source_type,
            &client_order_id,
            &exchange_order_id,
        )?;

        let canceled_not_from_wait_cancel_order = order_ref
            .internal_props()
            .canceled_not_from_wait_cancel_order;
        assert_eq!(canceled_not_from_wait_cancel_order, true);

        let gotten_id = event_receiver
            .recv()
            .context("Event was not received")?
            .order
            .client_order_id();
        assert_eq!(gotten_id, client_order_id);
        Ok(())
    }
}
