use chrono::Utc;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::nothing_to_do;

use crate::{
    exchanges::common::ExchangeError, exchanges::common::ExchangeErrorType,
    exchanges::general::exchange::Exchange, orders::event::OrderEventType,
    orders::fill::EventSourceType, orders::order::ExchangeOrderId, orders::order::OrderStatus,
    orders::pool::OrderRef,
};

impl Exchange {
    pub(crate) fn handle_cancel_order_failed(
        &self,
        exchange_order_id: &ExchangeOrderId,
        error: ExchangeError,
        event_source_type: EventSourceType,
    ) {
        if Self::should_ignore_event(
            self.features.allowed_cancel_event_source_type,
            event_source_type,
        ) {
            return;
        }

        match self.orders.cache_by_exchange_id.get(exchange_order_id) {
            None => {
                log::error!("cancel_order_failed was called for an order which is not in the local order pool: {:?} on {}",
                    exchange_order_id,
                    self.exchange_account_id);
            }
            Some(order) => self.react_based_on_order_status(
                &order,
                error,
                exchange_order_id,
                event_source_type,
            ),
        }
    }

    fn react_based_on_order_status(
        &self,
        order: &OrderRef,
        error: ExchangeError,
        exchange_order_id: &ExchangeOrderId,
        event_source_type: EventSourceType,
    ) {
        match order.status() {
            OrderStatus::Canceled => {
                log::warn!(
                    "cancel_order_failed was called for already Canceled order: {} {:?} on {}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                );
            }
            OrderStatus::Completed => {
                log::warn!(
                    "cancel_order_failed was called for already Completed order: {} {:?} on {}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                );
            }
            _ => {
                order.fn_mut(|order| {
                    order.internal_props.last_cancellation_error = Some(error.error_type);
                    order.internal_props.cancellation_event_source_type = Some(event_source_type);
                });

                self.react_based_on_error_type(order, error, exchange_order_id, event_source_type);
            }
        }
    }

    fn react_based_on_error_type(
        &self,
        order: &OrderRef,
        error: ExchangeError,
        exchange_order_id: &ExchangeOrderId,
        event_source_type: EventSourceType,
    ) {
        match error.error_type {
            ExchangeErrorType::OrderNotFound => {
                self.handle_cancel_order_succeeded(
                    None,
                    exchange_order_id,
                    None,
                    event_source_type,
                );
            }
            ExchangeErrorType::OrderCompleted => nothing_to_do(),
            _ => {
                if event_source_type == EventSourceType::RestFallback {
                    // TODO Some metrics
                }

                order.fn_mut(|order| order.set_status(OrderStatus::FailedToCancel, Utc::now()));
                self.add_event_on_order_change(order, OrderEventType::CancelOrderFailed)
                    .with_expect(|| {
                        format!(
                            "Failed to add event CancelOrderFailed on order change {:?}",
                            order.client_order_id()
                        )
                    });

                log::warn!(
                    "Order cancellation failed: {} {:?} on {} with error: {:?} {:?} {}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                    error.code,
                    error.error_type,
                    error.message
                );

                // TODO DataRecorder.save()
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::exchanges::events::ExchangeEvent;
    use crate::exchanges::{common::ExchangeErrorType, general::test_helper::get_test_exchange};
    use crate::{
        exchanges::common::CurrencyPair,
        exchanges::general::test_helper,
        orders::order::OrderRole,
        orders::order::{
            ClientOrderId, OrderExecutionType, OrderFills, OrderHeader, OrderSide,
            OrderSimpleProps, OrderSnapshot, OrderStatusHistory, OrderType,
            SystemInternalOrderProps,
        },
        orders::pool::OrdersPool,
    };
    use parking_lot::RwLock;
    use rust_decimal_macros::dec;
    use std::mem::discriminant;
    use std::sync::Arc;
    use tokio::sync::broadcast::error::TryRecvError;

    #[test]
    fn no_such_order_in_local_pool() {
        // Arrange
        let (exchange, mut event_receiver) = get_test_exchange(false);
        let exchange_order_id = ExchangeOrderId::new("test".into());
        let error = ExchangeError::new(ExchangeErrorType::Unknown, "test_error".to_owned(), None);

        // Act
        exchange.handle_cancel_order_failed(&exchange_order_id, error, EventSourceType::WebSocket);

        // Assert
        match event_receiver.try_recv() {
            Ok(_) => assert!(false),
            Err(error) => assert_eq!(error, TryRecvError::Empty),
        }
    }

    mod order_status {
        use super::*;
        #[test]
        fn order_canceled() {
            // Arrange
            let (exchange, mut event_receiver) = get_test_exchange(false);
            let exchange_order_id = &ExchangeOrderId::new("test".into());
            let error =
                ExchangeError::new(ExchangeErrorType::Unknown, "test_error".to_owned(), None);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_amount = dec!(12);
            let order_price = dec!(0.2);
            let order_role = OrderRole::Maker;

            let header = OrderHeader::new(
                client_order_id.clone(),
                Utc::now(),
                exchange.exchange_account_id,
                currency_pair,
                OrderType::Limit,
                OrderSide::Buy,
                order_amount,
                OrderExecutionType::None,
                None,
                None,
                "FromTest".to_owned(),
            );
            let props = OrderSimpleProps::new(
                Some(order_price),
                Some(order_role),
                Some(exchange_order_id.clone()),
                Default::default(),
                Default::default(),
                OrderStatus::Canceled,
                None,
            );
            let order = OrderSnapshot::new(
                header,
                props,
                OrderFills::default(),
                OrderStatusHistory::default(),
                SystemInternalOrderProps::default(),
            );
            let order_pool = OrdersPool::new();
            let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
            test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

            // Act
            exchange.handle_cancel_order_failed(
                exchange_order_id,
                error,
                EventSourceType::WebSocket,
            );

            // Assert
            match event_receiver.try_recv() {
                Ok(_) => assert!(false),
                Err(error) => assert_eq!(error, TryRecvError::Empty),
            }
        }

        #[test]
        fn order_completed() {
            // Arrange
            let (exchange, mut event_receiver) = get_test_exchange(false);
            let exchange_order_id = ExchangeOrderId::new("test".into());
            let error =
                ExchangeError::new(ExchangeErrorType::Unknown, "test_error".to_owned(), None);

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_amount = dec!(12);
            let order_price = dec!(0.2);
            let order_role = OrderRole::Maker;

            let header = OrderHeader::new(
                client_order_id.clone(),
                Utc::now(),
                exchange.exchange_account_id,
                currency_pair,
                OrderType::Limit,
                OrderSide::Buy,
                order_amount,
                OrderExecutionType::None,
                None,
                None,
                "FromTest".to_owned(),
            );
            let props = OrderSimpleProps::new(
                Some(order_price),
                Some(order_role),
                Some(exchange_order_id.clone()),
                Default::default(),
                Default::default(),
                OrderStatus::Completed,
                None,
            );
            let order = OrderSnapshot::new(
                header,
                props,
                OrderFills::default(),
                OrderStatusHistory::default(),
                SystemInternalOrderProps::default(),
            );
            let order_pool = OrdersPool::new();
            let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
            test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

            // Act
            exchange.handle_cancel_order_failed(
                &exchange_order_id,
                error,
                EventSourceType::WebSocket,
            );

            // Assert
            match event_receiver.try_recv() {
                Ok(_) => assert!(false),
                Err(error) => assert_eq!(error, TryRecvError::Empty),
            }
        }
    }

    mod order_not_found {
        use super::*;
        use crate::exchanges::events::ExchangeEvent;
        use std::mem::discriminant;

        #[test]
        fn error_type_not_found_no_event() {
            // Arrange
            let (exchange, mut event_receiver) = get_test_exchange(false);
            let exchange_order_id = ExchangeOrderId::new("test".into());

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_amount = dec!(12);
            let order_price = dec!(0.2);
            let order_role = OrderRole::Maker;

            let header = OrderHeader::new(
                client_order_id.clone(),
                Utc::now(),
                exchange.exchange_account_id,
                currency_pair,
                OrderType::Limit,
                OrderSide::Buy,
                order_amount,
                OrderExecutionType::None,
                None,
                None,
                "FromTest".to_owned(),
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
            let mut order = OrderSnapshot::new(
                header,
                props,
                OrderFills::default(),
                OrderStatusHistory::default(),
                SystemInternalOrderProps::default(),
            );
            let order_pool = OrdersPool::new();
            order.internal_props.is_canceling_from_wait_cancel_order = true;
            let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
            test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

            let error = ExchangeError::new(
                ExchangeErrorType::OrderNotFound,
                "Order_not_found".to_owned(),
                None,
            );

            // Act
            exchange.handle_cancel_order_failed(
                &exchange_order_id,
                error.clone(),
                EventSourceType::WebSocket,
            );

            // Assert
            assert_eq!(order_ref.status(), OrderStatus::Canceled);
            assert_eq!(
                order_ref
                    .fn_ref(|x| x.internal_props.last_cancellation_error)
                    .expect("in test"),
                error.error_type
            );
            assert_eq!(
                order_ref
                    .fn_ref(|x| x.internal_props.cancellation_event_source_type)
                    .expect("in test"),
                EventSourceType::WebSocket,
            );

            match event_receiver.try_recv() {
                Ok(_) => assert!(false),
                Err(error) => assert_eq!(error, TryRecvError::Empty),
            }
        }

        #[test]
        fn error_type_not_found_event_from_handler() {
            // Arrange
            let (exchange, mut event_receiver) = get_test_exchange(false);
            let exchange_order_id = ExchangeOrderId::new("test".into());

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_amount = dec!(12);
            let order_price = dec!(0.2);
            let order_role = OrderRole::Maker;

            let header = OrderHeader::new(
                client_order_id.clone(),
                Utc::now(),
                exchange.exchange_account_id,
                currency_pair,
                OrderType::Limit,
                OrderSide::Buy,
                order_amount,
                OrderExecutionType::None,
                None,
                None,
                "FromTest".to_owned(),
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
            let mut order = OrderSnapshot::new(
                header,
                props,
                OrderFills::default(),
                OrderStatusHistory::default(),
                SystemInternalOrderProps::default(),
            );
            order.internal_props.is_canceling_from_wait_cancel_order = false;
            let order_pool = OrdersPool::new();
            let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
            test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

            let error = ExchangeError::new(
                ExchangeErrorType::OrderNotFound,
                "Order_not_found".to_owned(),
                None,
            );

            // Act
            exchange.handle_cancel_order_failed(
                &exchange_order_id,
                error.clone(),
                EventSourceType::WebSocket,
            );

            // Assert
            assert_eq!(order_ref.status(), OrderStatus::Canceled);
            assert_eq!(
                order_ref
                    .fn_ref(|x| x.internal_props.last_cancellation_error)
                    .expect("in test"),
                error.error_type
            );
            assert_eq!(
                order_ref
                    .fn_ref(|x| x.internal_props.cancellation_event_source_type)
                    .expect("in test"),
                EventSourceType::WebSocket,
            );

            let received_event = event_receiver.try_recv().expect("in test");
            let received_event = match received_event {
                ExchangeEvent::OrderEvent(v) => v,
                _ => panic!("Should receive OrderEvent"),
            };

            assert_eq!(
                discriminant(&received_event.event_type),
                discriminant(&OrderEventType::CancelOrderSucceeded)
            );
        }
    }

    #[test]
    fn order_completed() {
        // Arrange
        let (exchange, mut event_receiver) = get_test_exchange(false);
        let exchange_order_id = ExchangeOrderId::new("test".into());

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_amount = dec!(12);
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;

        let header = OrderHeader::new(
            client_order_id.clone(),
            Utc::now(),
            exchange.exchange_account_id,
            currency_pair,
            OrderType::Limit,
            OrderSide::Buy,
            order_amount,
            OrderExecutionType::None,
            None,
            None,
            "FromTest".to_owned(),
        );
        let props = OrderSimpleProps::new(
            Some(order_price),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            Default::default(),
            OrderStatus::Created,
            None,
        );
        let order = OrderSnapshot::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
        );
        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let error = ExchangeError::new(
            ExchangeErrorType::OrderCompleted,
            "Order Completed".to_owned(),
            None,
        );

        // Act
        exchange.handle_cancel_order_failed(
            &exchange_order_id,
            error.clone(),
            EventSourceType::WebSocket,
        );

        // Assert
        assert_eq!(order_ref.status(), OrderStatus::Created);
        assert_eq!(
            order_ref
                .fn_ref(|x| x.internal_props.last_cancellation_error)
                .expect("in test"),
            error.error_type
        );
        assert_eq!(
            order_ref
                .fn_ref(|x| x.internal_props.cancellation_event_source_type)
                .expect("in test"),
            EventSourceType::WebSocket,
        );

        match event_receiver.try_recv() {
            Ok(_) => assert!(false),
            Err(error) => assert_eq!(error, TryRecvError::Empty),
        }
    }

    #[test]
    fn failed_to_cancel() {
        // Arrange
        let (exchange, mut event_receiver) = get_test_exchange(false);
        let exchange_order_id = ExchangeOrderId::new("test".into());

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_amount = dec!(12);
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;

        let header = OrderHeader::new(
            client_order_id.clone(),
            Utc::now(),
            exchange.exchange_account_id,
            currency_pair,
            OrderType::Limit,
            OrderSide::Buy,
            order_amount,
            OrderExecutionType::None,
            None,
            None,
            "FromTest".to_owned(),
        );
        let props = OrderSimpleProps::new(
            Some(order_price),
            Some(order_role),
            Some(exchange_order_id.clone()),
            Default::default(),
            Default::default(),
            OrderStatus::Created,
            None,
        );
        let order = OrderSnapshot::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
        );
        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let error = ExchangeError::new(
            ExchangeErrorType::Authentication,
            "Authentication error".to_owned(),
            None,
        );

        // Act
        exchange.handle_cancel_order_failed(
            &exchange_order_id,
            error.clone(),
            EventSourceType::WebSocket,
        );

        // Assert
        assert_eq!(order_ref.status(), OrderStatus::FailedToCancel);
        assert_eq!(
            order_ref
                .fn_ref(|x| x.internal_props.last_cancellation_error)
                .expect("in test"),
            error.error_type
        );
        assert_eq!(
            order_ref
                .fn_ref(|x| x.internal_props.cancellation_event_source_type)
                .expect("in test"),
            EventSourceType::WebSocket,
        );

        let received_event = event_receiver.try_recv().expect("in test");
        let received_event = match received_event {
            ExchangeEvent::OrderEvent(v) => v,
            _ => panic!("Should receive OrderEvent"),
        };

        assert_eq!(
            discriminant(&received_event.event_type),
            discriminant(&OrderEventType::CancelOrderFailed)
        );
    }
}
