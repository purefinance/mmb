use crate::exchanges::general::exchange::Exchange;
use crate::exchanges::general::handlers::should_ignore_event;
use crate::exchanges::traits::ExchangeError;
use chrono::Utc;
use function_name::named;
use mmb_domain::market::ExchangeErrorType;
use mmb_domain::order::event::OrderEventType;
use mmb_domain::order::fill::EventSourceType;
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::OrderStatus;
use mmb_domain::order::snapshot::{ClientOrderId, ExchangeOrderId};
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::nothing_to_do;

impl Exchange {
    #[named]
    pub(crate) fn handle_cancel_order_failed(
        &self,
        exchange_order_id: &ExchangeOrderId,
        error: ExchangeError,
        event_source_type: EventSourceType,
    ) {
        log::trace!(
            concat!("started ", function_name!(), " {} {:?} {:?}"),
            exchange_order_id,
            error,
            event_source_type
        );

        let allowed_cancel_event_source_type = self.features.allowed_cancel_event_source_type;
        if should_ignore_event(allowed_cancel_event_source_type, event_source_type) {
            return;
        }

        match self.orders.cache_by_exchange_id.get(exchange_order_id) {
            None => log::error!("cancel_order_failed was called with error {error:?} for an order which is not in the local order pool: {exchange_order_id:?} on {}", self.exchange_account_id),
            Some(order) => self.react_based_on_order_status(&order, error, exchange_order_id, event_source_type),
        }
    }

    fn react_based_on_order_status(
        &self,
        order: &OrderRef,
        error: ExchangeError,
        exchange_order_id: &ExchangeOrderId,
        event_source_type: EventSourceType,
    ) {
        let (status, client_order_id) = order.fn_ref(|x| (x.status(), x.client_order_id()));
        match status {
            OrderStatus::Canceled | OrderStatus::Completed => log::warn!("cancel_order_failed was called for already {status:?} order: {client_order_id} {exchange_order_id:?} on {}", self.exchange_account_id),
            _ => {
                order.fn_mut(|order| {
                    order.internal_props.last_cancellation_error = Some(error.error_type);
                    order.internal_props.cancellation_event_source_type = Some(event_source_type);
                });

                self.react_based_on_error_type(order, &client_order_id, exchange_order_id, error, event_source_type);
            }
        }
    }

    fn react_based_on_error_type(
        &self,
        order: &OrderRef,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
        error: ExchangeError,
        event_source_type: EventSourceType,
    ) {
        match error.error_type {
            ExchangeErrorType::OrderNotFound => {
                self.handle_cancel_order_succeeded(None, exchange_order_id, None, event_source_type)
            }
            ExchangeErrorType::OrderCompleted => nothing_to_do(),
            _ => {
                if event_source_type == EventSourceType::RestFallback {
                    // TODO Some metrics
                }

                order.fn_mut(|x| x.set_status(OrderStatus::FailedToCancel, Utc::now()));

                self.add_event_on_order_change(order, OrderEventType::CancelOrderFailed)
                    .with_expect(|| format!("Failed to add event CancelOrderFailed on order change {client_order_id:?}"));

                log::warn!(
                    "Order cancellation failed: {client_order_id} {exchange_order_id:?} on {} with error: {:?} {:?} {}",
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
    use crate::exchanges::general::test_helper;
    use crate::exchanges::general::test_helper::get_test_exchange;
    use mmb_domain::events::ExchangeEvent;
    use mmb_domain::market::CurrencyPair;
    use mmb_domain::market::ExchangeErrorType;
    use mmb_domain::order::pool::OrdersPool;
    use mmb_domain::order::snapshot::OrderRole;
    use mmb_domain::order::snapshot::{
        ClientOrderId, OrderExecutionType, OrderFills, OrderHeader, OrderSide, OrderSimpleProps,
        OrderSnapshot, OrderStatusHistory, OrderType, SystemInternalOrderProps,
    };
    use parking_lot::RwLock;
    use rust_decimal_macros::dec;
    use std::mem::discriminant;
    use std::sync::Arc;
    use tokio::sync::broadcast::error::TryRecvError;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_such_order_in_local_pool() {
        // Arrange
        let (exchange, mut event_receiver) = get_test_exchange(false);
        let exchange_order_id = ExchangeOrderId::new("test".into());
        let error = ExchangeError::new(ExchangeErrorType::Unknown, "test_error".to_owned(), None);

        // Act
        exchange.handle_cancel_order_failed(&exchange_order_id, error, EventSourceType::WebSocket);

        // Assert
        let error = event_receiver.try_recv().expect_err("should be error");
        assert_eq!(error, TryRecvError::Empty);
    }

    mod order_status {
        use super::*;
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn order_canceled() {
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
                client_order_id,
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
                None,
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
            let error = event_receiver.try_recv().expect_err("should be error");
            assert_eq!(error, TryRecvError::Empty);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn order_completed() {
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
                client_order_id,
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
                None,
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
            let error = event_receiver.try_recv().expect_err("should be error");
            assert_eq!(error, TryRecvError::Empty);
        }
    }

    mod order_not_found {
        use super::*;
        use mmb_domain::events::ExchangeEvent;
        use std::mem::discriminant;

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn error_type_not_found_no_event() {
            // Arrange
            let (exchange, mut event_receiver) = get_test_exchange(false);
            let exchange_order_id = ExchangeOrderId::new("test".into());

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_amount = dec!(12);
            let order_price = dec!(0.2);
            let order_role = OrderRole::Maker;

            let header = OrderHeader::new(
                client_order_id,
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
                None,
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

            let error = event_receiver.try_recv().expect_err("should be error");
            assert_eq!(error, TryRecvError::Empty);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn error_type_not_found_event_from_handler() {
            // Arrange
            let (exchange, mut event_receiver) = get_test_exchange(false);
            let exchange_order_id = ExchangeOrderId::new("test".into());

            let client_order_id = ClientOrderId::unique_id();
            let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
            let order_amount = dec!(12);
            let order_price = dec!(0.2);
            let order_role = OrderRole::Maker;

            let header = OrderHeader::new(
                client_order_id,
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
                None,
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn order_completed() {
        // Arrange
        let (exchange, mut event_receiver) = get_test_exchange(false);
        let exchange_order_id = ExchangeOrderId::new("test".into());

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_amount = dec!(12);
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;

        let header = OrderHeader::new(
            client_order_id,
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
            None,
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

        let error = event_receiver.try_recv().expect_err("should be error");
        assert_eq!(error, TryRecvError::Empty);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_to_cancel() {
        // Arrange
        let (exchange, mut event_receiver) = get_test_exchange(false);
        let exchange_order_id = ExchangeOrderId::new("test".into());

        let client_order_id = ClientOrderId::unique_id();
        let currency_pair = CurrencyPair::from_codes("PHB".into(), "BTC".into());
        let order_amount = dec!(12);
        let order_price = dec!(0.2);
        let order_role = OrderRole::Maker;

        let header = OrderHeader::new(
            client_order_id,
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
            None,
        );
        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
        test_helper::try_add_snapshot_by_exchange_id(&exchange, &order_ref);

        let error = ExchangeError::authentication("Authentication error".to_owned());

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
