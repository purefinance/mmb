use chrono::Utc;
use mmb_utils::infrastructure::WithExpect;

use crate::{
    exchanges::common::Amount,
    exchanges::general::exchange::Exchange,
    orders::{
        event::OrderEventType, fill::EventSourceType, order::ClientOrderId, order::ExchangeOrderId,
        order::OrderStatus, pool::OrderRef,
    },
};

impl Exchange {
    pub(crate) fn handle_cancel_order_succeeded(
        &self,
        client_order_id: Option<&ClientOrderId>,
        exchange_order_id: &ExchangeOrderId,
        filled_amount: Option<Amount>,
        source_type: EventSourceType,
    ) {
        let args_to_log = (
            self.exchange_account_id,
            exchange_order_id.clone(),
            self.features.allowed_cancel_event_source_type,
            source_type,
        );

        if Self::should_ignore_event(self.features.allowed_cancel_event_source_type, source_type) {
            log::info!("Ignoring fill {:?}", args_to_log);
            return;
        }

        if exchange_order_id.is_empty() {
            panic!(
                "Received HandleOrderFilled with an empty exchangeOrderId {:?}",
                &args_to_log
            );
        }

        match self.orders.cache_by_exchange_id.get(exchange_order_id) {
            None => {
                self.buffered_canceled_orders_manager
                    .lock()
                    .add_order(self.exchange_account_id, exchange_order_id.clone());

                match client_order_id {
                    Some(client_order_id) =>
                        self.raise_order_created(client_order_id, exchange_order_id, source_type),
                    None =>
                        log::error!("cancel_order_succeeded was received for an order which is not in the system {} {:?}",
                            self.exchange_account_id,
                            exchange_order_id),
                }
            }
            Some(order_ref) => {
                self.update_local_order(&order_ref, filled_amount, source_type, exchange_order_id)
            }
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

        log::warn!(
            "CancelOrderSucceeded received for {} order {} {:?} {}",
            arg_to_log,
            client_order_id,
            exchange_order_id,
            self.exchange_account_id
        );

        true
    }

    fn update_local_order(
        &self,
        order_ref: &OrderRef,
        filled_amount: Option<Amount>,
        source_type: EventSourceType,
        exchange_order_id: &ExchangeOrderId,
    ) {
        let client_order_id = order_ref.client_order_id();

        if self.order_already_closed(order_ref.status(), &client_order_id, exchange_order_id) {
            return;
        }

        if source_type == EventSourceType::RestFallback {
            // TODO some metrics
        }

        let is_canceling_from_wait_cancel_order = order_ref.fn_mut(|order| {
            order.internal_props.filled_amount_after_cancellation = filled_amount;
            order.set_status(OrderStatus::Canceled, Utc::now());
            order.internal_props.cancellation_event_source_type = Some(source_type);
            order.internal_props.is_canceling_from_wait_cancel_order
        });

        // Here we cover the situation with MakerOnly orders
        // As soon as we created an order, it was automatically canceled
        // Usually we raise CancelOrderSucceeded in WaitCancelOrder after a check for fills via fallback
        // but in this particular case the cancellation is triggered by exchange itself, so WaitCancelOrder was never called
        if !is_canceling_from_wait_cancel_order {
            log::info!("Adding CancelOrderSucceeded event from handle_cancel_order_succeeded() {:?} {:?} on {}",
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

            self.add_event_on_order_change(order_ref, OrderEventType::CancelOrderSucceeded)
                .with_expect(|| format!("Failed to add event CancelOrderSucceeded on order change {client_order_id}"));
        }

        log::info!(
            "Order was successfully cancelled {:?} {:?} on {}",
            client_order_id,
            exchange_order_id,
            self.exchange_account_id
        );

        // TODO DataRecorder.save(order_ref)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::exchanges::events::ExchangeEvent;
    use crate::{
        exchanges::common::CurrencyPair, exchanges::general::test_helper, orders::order::OrderRole,
        orders::order::OrderSide,
    };
    use rstest::rstest;
    use rust_decimal_macros::dec;

    #[test]
    #[should_panic(expected = "Received HandleOrderFilled with an empty exchangeOrderId")]
    fn empty_exchange_order_id() {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = dec!(1);
        let source_type = EventSourceType::Rest;

        exchange.handle_cancel_order_succeeded(
            Some(&client_order_id),
            &exchange_order_id,
            Some(filled_amount),
            source_type,
        );
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
    fn return_if_order_already_closed() {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            exchange.exchange_account_id,
            currency_pair,
            fill_price,
            order_amount,
            order_side,
        );
        order_ref.fn_mut(|order| order.set_status(OrderStatus::Completed, Utc::now()));

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        exchange.update_local_order(&order_ref, filled_amount, source_type, &exchange_order_id);
    }

    #[test]
    fn order_filled_amount_cancellation_updated() {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            exchange.exchange_account_id,
            currency_pair,
            fill_price,
            order_amount,
            order_side,
        );

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        exchange.update_local_order(&order_ref, filled_amount, source_type, &exchange_order_id);

        let changed_amount =
            order_ref.fn_ref(|x| x.internal_props.filled_amount_after_cancellation);
        let expected = filled_amount;
        assert_eq!(changed_amount, expected);
    }

    #[test]
    fn order_status_updated() {
        let (exchange, _rx) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            exchange.exchange_account_id,
            currency_pair,
            fill_price,
            order_amount,
            order_side,
        );

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        exchange.update_local_order(&order_ref, filled_amount, source_type, &exchange_order_id);

        let order_status = order_ref.status();
        assert_eq!(order_status, OrderStatus::Canceled);

        let order_event_source_type = order_ref
            .fn_ref(|x| x.internal_props.cancellation_event_source_type)
            .expect("in test");
        assert_eq!(order_event_source_type, source_type);
    }

    #[test]
    fn canceled_not_from_wait_cancel_order() {
        let (exchange, mut event_receiver) = test_helper::get_test_exchange(false);

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_side = OrderSide::Buy;
        let order_amount = dec!(12);
        let order_role = OrderRole::Maker;
        let fill_price = dec!(0.8);

        let order_ref = test_helper::create_order_ref(
            &client_order_id,
            Some(order_role),
            exchange.exchange_account_id,
            currency_pair,
            fill_price,
            order_amount,
            order_side,
        );

        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let exchange_order_id = ExchangeOrderId::new("".into());
        let filled_amount = Some(dec!(5));
        let source_type = EventSourceType::Rest;
        exchange.update_local_order(&order_ref, filled_amount, source_type, &exchange_order_id);

        let canceled_not_from_wait_cancel_order =
            order_ref.fn_ref(|x| x.internal_props.canceled_not_from_wait_cancel_order);
        assert_eq!(canceled_not_from_wait_cancel_order, true);

        let event = match event_receiver.try_recv().expect("Event was not received") {
            ExchangeEvent::OrderEvent(v) => v,
            _ => panic!("Should be OrderEvent"),
        };

        let gotten_id = event.order.client_order_id();
        assert_eq!(gotten_id, client_order_id);
    }
}
