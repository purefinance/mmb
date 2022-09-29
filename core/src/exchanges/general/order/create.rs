use crate::exchanges::general::exchange::RequestResult::{Error, Success};
use crate::exchanges::general::handlers::should_ignore_event;
use crate::exchanges::general::request_type::RequestType;
use crate::exchanges::timeouts::requests_timeout_manager::RequestGroupId;
use crate::exchanges::traits::ExchangeError;
use crate::misc::time::time_manager;
use crate::{exchanges::general::exchange::Exchange, exchanges::general::exchange::RequestResult};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use function_name::named;
use futures::pin_mut;
use mmb_domain::events::AllowedEventSourceType;
use mmb_domain::market::{ExchangeAccountId, ExchangeErrorType};
use mmb_domain::order::event::OrderEventType;
use mmb_domain::order::fill::EventSourceType;
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::{
    ClientOrderId, ExchangeOrderId, OrderCreating, OrderInfo, OrderStatus, OrderType,
};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::time::ToStdExpected;
use mmb_utils::{nothing_to_do, OPERATION_CANCELED_MSG};
use std::borrow::Cow;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CreateOrderResult {
    pub outcome: RequestResult<ExchangeOrderId>,
    pub source_type: EventSourceType,
}

impl CreateOrderResult {
    pub fn succeed(order_id: &ExchangeOrderId, source_type: EventSourceType) -> Self {
        CreateOrderResult {
            outcome: Success(order_id.clone()),
            source_type,
        }
    }

    pub fn failed(error: ExchangeError, source_type: EventSourceType) -> Self {
        CreateOrderResult {
            outcome: Error(error),
            source_type,
        }
    }
}

impl Exchange {
    pub async fn create_order(
        &self,
        order_to_create: OrderCreating,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
    ) -> Result<OrderRef> {
        use AllowedEventSourceType::*;

        log::info!("Submitting order {order_to_create:?}");

        let order = self.orders.add_simple_initial(
            order_to_create.header.clone(),
            time_manager::now(),
            Some(order_to_create.price),
            self.exchange_client.get_initial_extension_data(),
        );

        let linked_ct = cancellation_token.create_linked_token();

        let create_order_fut = self.create_order_base(&order, linked_ct.clone());

        let duration = Duration::from_secs(5 * 60);
        let poll_creation_fut = {
            let order = order.clone();
            let linked_ct = linked_ct.clone();
            async move {
                timeout(duration, self.poll_order_create(order, pre_reservation_group_id, linked_ct)).await.unwrap_or_else(|_| bail!("Time in form of {duration:?} is over, but future `poll order create` is not completed yet"))
            }
        };

        async fn handle_create_order_res(
            this: &Exchange,
            order: &OrderRef,
            pre_reservation_group_id: Option<RequestGroupId>,
            created_order_result: Result<CreateOrderResult>,
            linked_ct: CancellationToken,
            cancellation_token: CancellationToken,
        ) -> Result<()> {
            linked_ct.cancel();

            let created_order_result = created_order_result.context("failed create_order")?;

            match created_order_result.outcome {
                Success(exchange_order_id) => {
                    let exchange_order_id_from_pool = order
                        .exchange_order_id()
                        .expect("exchange_order_id should exists after check_order_creation");
                    if exchange_order_id_from_pool != exchange_order_id {
                        panic!("exchange_order_id {exchange_order_id:?} from create order request is different from exchange order from orders pool {exchange_order_id_from_pool:?}");
                    }
                }
                Error(exchange_error) => {
                    if exchange_error.error_type == ExchangeErrorType::ParsingError {
                        this.check_order_creation(
                            order.clone(),
                            Some(exchange_error),
                            pre_reservation_group_id,
                            cancellation_token.clone(),
                        )
                        .await;

                        order
                            .exchange_order_id()
                            .expect("exchange_order_id should exists after check_order_creation");
                    } else {
                        bail!("failed create_order: {}", exchange_error.message);
                    }
                }
            }

            Ok(())
        }

        fn handle_poll_creation_order_res(
            order: &OrderRef,
            poll_result: Result<()>,
            linked_ct: CancellationToken,
        ) -> Result<()> {
            linked_ct.cancel();
            poll_result.context("failed create_order fallback polling")?;
            order
                .exchange_order_id()
                .expect("exchange_order_id should exists after poll_order_create");
            Ok(())
        }

        match self.features.allowed_create_event_source_type {
            All => {
                tokio::select! {
                    created_order_result = create_order_fut => {
                        handle_create_order_res(
                            self,
                            &order,
                            pre_reservation_group_id,
                            created_order_result,
                            linked_ct.clone(),
                            cancellation_token.clone(),
                        ).await?;
                    },
                    poll_result = poll_creation_fut => handle_poll_creation_order_res(&order, poll_result, linked_ct)?,
                };
            }
            FallbackOnly => {
                pin_mut!(poll_creation_fut);
                let need_poll = tokio::select! {
                    _ = create_order_fut => true,
                    poll_result = &mut poll_creation_fut => {
                        handle_poll_creation_order_res(&order, poll_result, linked_ct.clone())?;
                        false
                    },
                };

                if need_poll {
                    let poll_result = poll_creation_fut.await;
                    handle_poll_creation_order_res(&order, poll_result, linked_ct)?;
                }
            }
            NonFallback => {
                let created_order_result = create_order_fut.await;
                handle_create_order_res(
                    self,
                    &order,
                    pre_reservation_group_id,
                    created_order_result,
                    linked_ct.clone(),
                    cancellation_token.clone(),
                )
                .await?;
            }
        }

        self.handle_created_order(&order, pre_reservation_group_id, cancellation_token)
            .await
            .unwrap_or_else(|err| log::error!("failed handle_created_order: {err}"));

        Ok(order)
    }

    async fn handle_created_order(
        &self,
        order: &OrderRef,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        let (is_rest_fallback, is_failed_to_created) = order.fn_ref(|x| {
            (
                x.internal_props.creation_event_source_type == Some(EventSourceType::RestFallback),
                x.status() != OrderStatus::FailedToCreate,
            )
        });

        if is_rest_fallback && is_failed_to_created {
            self.check_order_fills(order, false, pre_reservation_group_id, cancellation_token)
                .await?;
        }

        let (status, client_order_id) = order.fn_ref(|o| (o.status(), o.client_order_id()));

        if status == OrderStatus::Creating {
            log::error!("OrderStatus of order {client_order_id} is Creating at the end of create order procedure");
        }

        self.event_recorder
            .save(order.clone())
            .expect("Failure save order");

        let (header, exchange_order_id) =
            order.fn_ref(|o| (o.header.clone(), o.props.exchange_order_id.clone()));

        log::info!(
            "Order was submitted {client_order_id} {exchange_order_id:?} {:?} on {}",
            header.reservation_id,
            header.exchange_account_id,
        );

        Ok(())
    }

    async fn poll_order_create(
        &self,
        order: OrderRef,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        loop {
            let (status, last_order_creation_status_request_time) = order.fn_ref(|o| {
                (
                    o.status(),
                    o.internal_props.last_order_creation_status_request_time,
                )
            });

            if cancellation_token.is_cancellation_requested() || status != OrderStatus::Creating {
                break;
            }

            let now = time_manager::now();
            let order_creation_status_request_period = chrono::Duration::seconds(5);
            let delay_till_fallback_request = match last_order_creation_status_request_time {
                None => Some(order_creation_status_request_period.to_std_expected()),
                Some(last_request_time) => (order_creation_status_request_period
                    - (now - last_request_time))
                    .to_std()
                    .ok(),
            };

            if let Some(delay) = delay_till_fallback_request {
                sleep(delay).await;
            }

            self.check_order_creation(
                order.clone(),
                None,
                pre_reservation_group_id,
                cancellation_token.clone(),
            )
            .await;
        }

        Ok(())
    }

    async fn check_order_creation(
        &self,
        order: OrderRef,
        error: Option<ExchangeError>,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
    ) {
        while !cancellation_token.is_cancellation_requested() {
            let (status, client_order_id, exchange_order_id) =
                order.fn_ref(|o| (o.status(), o.client_order_id(), o.exchange_order_id()));

            if status != OrderStatus::Creating {
                return;
            }

            if self
                .features
                .order_features
                .supports_get_order_info_by_client_order_id
            {
                order.fn_mut(|o| {
                    o.internal_props.last_order_creation_status_request_time =
                        Some(time_manager::now())
                });

                log::trace!("Checking order info in CheckOrderCreation {client_order_id} {exchange_order_id:?} {}", self.exchange_account_id);

                self.timeout_manager
                    .reserve_when_available(
                        self.exchange_account_id,
                        RequestType::GetOrderInfo,
                        pre_reservation_group_id,
                        cancellation_token.clone(),
                    )
                    .await;

                let order_info_res = self.get_order_info(&order).await;

                let (status, client_order_id) = order.fn_ref(|o| (o.status(), o.client_order_id()));

                //In case order's status has changed while we were receiving OrderInfo
                if status != OrderStatus::Creating {
                    return;
                }

                match order_info_res {
                    Ok(order_info) => {
                        let exchange_order_id = order.fn_mut(|order| {
                            order.props.exchange_order_id =
                                Some(order_info.exchange_order_id.clone());
                            order.exchange_order_id()
                        });
                        self.handle_creating_order_from_check_order_info(
                            &client_order_id,
                            &exchange_order_id,
                            &order,
                            &order_info,
                        );

                        return;
                    }
                    Err(err) => {
                        self.handle_error_from_check_order_info(
                            &order,
                            &client_order_id,
                            &error,
                            err,
                        )
                        .await
                    }
                }
            } else if let Some(error) = &error {
                //ToDo: Here we need to try to find a similar order #
                self.handle_create_order_failed(
                    &order.client_order_id(),
                    error,
                    EventSourceType::RestFallback,
                )
                .unwrap_or_else(|err| {
                    log::error!(
                        "Failed handle_create_order_failed in check_order_creation: {err:?}"
                    )
                })
            } else {
                tokio::select! {
                    _ = sleep(Duration::from_millis(25)) => nothing_to_do(),
                    _ = cancellation_token.when_cancelled() => nothing_to_do(),
                }
            }
        }
    }

    async fn handle_error_from_check_order_info(
        &self,
        order: &OrderRef,
        client_order_id: &ClientOrderId,
        error: &Option<ExchangeError>,
        get_order_info_error: ExchangeError,
    ) {
        log::trace!(
            "CheckOrderCreation GetOrderInfo response err {get_order_info_error:?} for {client_order_id} on {}",
            self.exchange_account_id
        );

        // TODO hack for aax

        match get_order_info_error.error_type {
            ExchangeErrorType::OrderNotFound => {
                let (client_order_id, init_time) =
                    order.fn_ref(|o| (o.client_order_id(), o.init_time()));

                let now = time_manager::now();
                let min_timeout_for_failed_to_create_order = chrono::Duration::minutes(1);

                if now - init_time > min_timeout_for_failed_to_create_order {
                    let new_error = match &error {
                        None => Cow::Owned(ExchangeError::unknown("Creation fallback did not find an order by clientOrderId, so we're assuming this order was not created")),
                        Some(error) => Cow::Borrowed(error),
                    };

                    log::warn!(
                        "{} {client_order_id} on {}",
                        new_error.message,
                        self.exchange_account_id
                    );

                    self.handle_create_order_failed(
                        &client_order_id,
                        &new_error,
                        EventSourceType::RestFallback,
                    )
                    .unwrap_or_else(|err| {
                        log::error!(
                            "failed handle_create_order_failed in check_order_creation: {err:?}"
                        )
                    })
                }
            }
            ExchangeErrorType::RateLimit | ExchangeErrorType::ServiceUnavailable => {
                // TODO Integrate ExchangeBlocker to wait_order_finish/wait_cancel_order fallbacks #641
                let delay = self.get_timeout();
                // TODO fix for AAX
                sleep(delay).await;
            }
            _ => nothing_to_do(),
        }
    }

    fn handle_creating_order_from_check_order_info(
        &self,
        client_order_id: &ClientOrderId,
        exchange_order_id: &Option<ExchangeOrderId>,
        order: &OrderRef,
        order_info: &OrderInfo,
    ) {
        fn log_status(
            this: &Exchange,
            status: OrderStatus,
            client_order_id: &ClientOrderId,
            exchange_order_id: &Option<ExchangeOrderId>,
        ) {
            log::warn!("CheckOrderCreation fallback found a {status:?} order {client_order_id} {exchange_order_id:?} on {}", this.exchange_account_id);
        }

        let status = order_info.order_status;
        match status {
            OrderStatus::FailedToCreate => {
                log_status(self, status, client_order_id, exchange_order_id);

                self.handle_create_order_failed(
                    client_order_id,
                    &ExchangeError::unknown("Fallback"),
                    EventSourceType::RestFallback,
                )
                .unwrap_or_else(|err| log::error!("Failed 'check_order_creation' for order status 'FailedToCreate' with error: {err:?}"));
            }
            OrderStatus::Canceled => {
                log_status(self, status, client_order_id, exchange_order_id);

                let exchange_order_id = exchange_order_id
                    .as_ref()
                    .expect("exchange_order_id should be known when order status is `Canceled`");
                self.handle_cancel_order_succeeded(
                    Some(client_order_id),
                    exchange_order_id,
                    Some(order_info.filled_amount),
                    EventSourceType::RestFallback,
                )
            }
            OrderStatus::Created | OrderStatus::Completed => {
                log_status(self, status, client_order_id, exchange_order_id);

                order.fn_mut(|x| {
                    let filled_amount = Some(order_info.filled_amount);
                    if x.internal_props.filled_amount_after_cancellation < filled_amount {
                        x.internal_props.filled_amount_after_cancellation = filled_amount
                    }
                });

                self.raise_order_created(
                    client_order_id,
                    &order_info.exchange_order_id,
                    EventSourceType::RestFallback,
                );
            }
            _ => log::warn!(
                "Unknown order status {status:?} {client_order_id} {exchange_order_id:?} on {}",
                self.exchange_account_id
            ),
        }
    }

    async fn create_order_base(
        &self,
        order: &OrderRef,
        cancellation_token: CancellationToken,
    ) -> Result<CreateOrderResult> {
        let client_order_id = order.client_order_id();
        let create_order_result = self.create_order_core(order, cancellation_token).await;

        if let Some(created_order) = create_order_result {
            match &created_order.outcome {
                Success(exchange_order_id) => {
                    self.handle_create_order_succeeded(
                        self.exchange_account_id,
                        &client_order_id,
                        exchange_order_id,
                        created_order.source_type,
                    )?;
                }
                Error(exchange_error) => {
                    if exchange_error.error_type != ExchangeErrorType::ParsingError {
                        self.handle_create_order_failed(
                            &client_order_id,
                            exchange_error,
                            created_order.source_type,
                        )?
                    }
                }
            }

            return Ok(created_order);
        }

        bail!(OPERATION_CANCELED_MSG)
    }

    #[named]
    fn handle_create_order_failed(
        &self,
        client_order_id: &ClientOrderId,
        exchange_error: &ExchangeError,
        source_type: EventSourceType,
    ) -> Result<()> {
        log::trace!(
            concat!("started ", function_name!(), " {} {:?} {:?}"),
            client_order_id,
            source_type,
            exchange_error,
        );

        if should_ignore_event(self.features.allowed_create_event_source_type, source_type) {
            return Ok(());
        }

        let args_to_log = (self.exchange_account_id, client_order_id);

        if client_order_id.as_str().is_empty() {
            bail!("Order was created but client_order_id is empty. Order: {args_to_log:?}");
        }

        let order_ref = self.orders.cache_by_client_id.get(client_order_id).with_context(|| {
            let error_msg = format!(
                "CreateOrderSucceeded was received for an order which is not in the local orders pool {args_to_log:?}");

            log::error!("{error_msg}");
            error_msg
        })?;

        let args_to_log = (
            self.exchange_account_id,
            client_order_id,
            &order_ref.exchange_order_id(),
        );
        self.react_on_status_when_failed(&order_ref, args_to_log, source_type, exchange_error)
    }

    fn react_on_status_when_failed(
        &self,
        order_ref: &OrderRef,
        args_to_log: (ExchangeAccountId, &ClientOrderId, &Option<ExchangeOrderId>),
        _source_type: EventSourceType,
        exchange_error: &ExchangeError,
    ) -> Result<()> {
        let status = order_ref.status();
        match status {
            OrderStatus::Created
            | OrderStatus::Canceling
            | OrderStatus::Canceled
            | OrderStatus::Completed
            | OrderStatus::FailedToCancel => {
                let error_msg = format!(
                    "CreateOrderFailed was received for a {status:?} order {args_to_log:?}"
                );

                log::error!("{error_msg}");
                bail!(error_msg)
            }
            OrderStatus::FailedToCreate => {
                log::warn!("CreateOrderFailed was received for a {status:?} order {args_to_log:?}");
                Ok(())
            }
            OrderStatus::Creating => {
                // TODO RestFallback and some metrics

                order_ref.fn_mut(|order| {
                    order.set_status(OrderStatus::FailedToCreate, Utc::now());
                    order.internal_props.last_creation_error_type = Some(exchange_error.error_type);
                    order.internal_props.last_creation_error_message =
                        exchange_error.message.clone();
                });

                self.add_event_on_order_change(order_ref, OrderEventType::CreateOrderFailed)?;

                self.event_recorder
                    .save(order_ref.clone())
                    .expect("Failure save order");

                log::error!("Order creation failed {args_to_log:?}: {exchange_error:?}");

                Ok(())
            }
        }
    }

    #[named]
    pub(crate) fn handle_create_order_succeeded(
        &self,
        exchange_account_id: ExchangeAccountId,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
        source_type: EventSourceType,
    ) -> Result<()> {
        log::trace!(
            concat!("started ", function_name!(), " {} {:?}"),
            client_order_id,
            source_type,
        );

        if should_ignore_event(self.features.allowed_create_event_source_type, source_type) {
            return Ok(());
        }

        let args_to_log = (exchange_account_id, client_order_id, exchange_order_id);

        if client_order_id.as_str().is_empty() {
            let error_msg =
                format!("Order was created but client_order_id is empty. Order: {args_to_log:?}");

            log::error!("{error_msg}");
            bail!(error_msg);
        }

        if exchange_order_id.as_str().is_empty() {
            let error_msg =
                format!("Order was created but exchange_order_id is empty. Order: {args_to_log:?}");

            log::error!("{error_msg}");
            bail!(error_msg);
        }

        match self.orders.cache_by_client_id.get(client_order_id) {
            None => {
                log::warn!("CreateOrderSucceeded was received for an order which is not in the local orders pool {args_to_log:?}");
                Ok(())
            }
            Some(order_ref) => {
                order_ref.fn_mut(|order| {
                    order.props.exchange_order_id = Some(exchange_order_id.clone());
                });
                self.react_on_status_when_succeed(&order_ref, args_to_log, source_type)
            }
        }
    }

    fn react_on_status_when_succeed(
        &self,
        order_ref: &OrderRef,
        args_to_log: (ExchangeAccountId, &ClientOrderId, &ExchangeOrderId),
        source_type: EventSourceType,
    ) -> Result<()> {
        let status = order_ref.status();
        let exchange_order_id = args_to_log.2;
        match status {
            OrderStatus::FailedToCreate => {
                let error_msg = format!("CreateOrderSucceeded was received for a FailedToCreate order. Probably FailedToCreate fallback was received before Creation Response {args_to_log:?}");
                log::error!("{error_msg}");
                bail!(error_msg)
            }
            OrderStatus::Created
            | OrderStatus::Canceling
            | OrderStatus::Canceled
            | OrderStatus::Completed
            | OrderStatus::FailedToCancel => {
                log::warn!(
                    "CreateOrderSucceeded was received for a {status:?} order {args_to_log:?}"
                );
                Ok(())
            }
            OrderStatus::Creating => {
                if self
                    .orders
                    .cache_by_exchange_id
                    .contains_key(exchange_order_id)
                {
                    log::info!(
                        "Order has already been added to the local orders pool {args_to_log:?}"
                    );

                    return Ok(());
                }

                // TODO RestFallback and some metrics

                order_ref.fn_mut(|order| {
                    order.set_status(OrderStatus::Created, Utc::now());
                    order.internal_props.creation_event_source_type = Some(source_type);
                });

                self.orders
                    .cache_by_exchange_id
                    .insert(exchange_order_id.clone(), order_ref.clone());

                let header = order_ref.fn_ref(|x| x.header.clone());
                let client_order_id = header.client_order_id.clone();
                if order_ref.order_type() != OrderType::Liquidation {
                    match header.reservation_id {
                        None => {
                            log::warn!("Created order {client_order_id} without reservation_id")
                        }
                        Some(reservation_id) => {
                            let bm_lock = self.balance_manager.lock();
                            match bm_lock.as_ref().expect("BalanceManager should be initialized before receiving order events").upgrade() {
                                None => log::warn!("BalanceManager ref can't be upgraded in handler create order succeeded event"),
                                Some(balance_manager) => balance_manager.lock().approve_reservation(
                                    reservation_id,
                                    &client_order_id,
                                    header.amount,
                                )
                            }
                        }
                    };
                }

                self.add_event_on_order_change(order_ref, OrderEventType::CreateOrderSucceeded)?;

                let mut buffered_fills_manager = self.buffered_fills_manager.lock();
                if let Some(buffered_fills) = buffered_fills_manager.get_fills(exchange_order_id) {
                    log::trace!(
                        "Found buffered fills for an order {client_order_id} {exchange_order_id} on {}:\n{buffered_fills:?}",
                        self.exchange_account_id,
                    );

                    for buffered_fill in buffered_fills {
                        let mut fill_event =
                            buffered_fill.to_fill_event_data(client_order_id.clone());
                        self.handle_order_filled(&mut fill_event);
                    }

                    buffered_fills_manager.remove_fills(exchange_order_id);
                }
                drop(buffered_fills_manager);

                let mut buffered_canceled_orders_manager =
                    self.buffered_canceled_orders_manager.lock();
                if buffered_canceled_orders_manager.is_order_buffered(exchange_order_id) {
                    self.handle_cancel_order_succeeded(
                        Some(&client_order_id),
                        exchange_order_id,
                        None,
                        source_type,
                    );
                    buffered_canceled_orders_manager.remove_order(exchange_order_id);
                }
                drop(buffered_canceled_orders_manager);

                self.event_recorder
                    .save(order_ref.clone())
                    .expect("Failure save order");

                log::info!("Order was created: {args_to_log:?}");

                Ok(())
            }
        }
    }

    pub(super) async fn create_order_created_fut(
        &self,
        order: &OrderRef,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        let (status, client_order_id, exchange_order_id) =
            order.fn_ref(|x| (x.status(), x.client_order_id(), x.exchange_order_id()));

        if status != OrderStatus::Creating {
            log::info!("Instantly exiting create_order_created_task because order's status is {status:?} {client_order_id} {exchange_order_id:?} on {}", self.exchange_account_id);
            return Ok(());
        }

        cancellation_token.error_if_cancellation_requested()?;

        let (tx, rx) = oneshot::channel();
        self.orders_created_events
            .entry(order.client_order_id())
            .or_insert(tx);

        let (status, client_order_id, exchange_order_id) =
            order.fn_ref(|x| (x.status(), x.client_order_id(), x.exchange_order_id()));

        if status != OrderStatus::Creating {
            log::info!("Exiting create_order_created_task because order's status turned {status:?} while oneshot::channel were creating {client_order_id} {exchange_order_id:?} on {}", self.exchange_account_id);
            self.order_created_notify(order);
            return Ok(());
        }

        tokio::select! {
            _ = rx => nothing_to_do(),
            _ = cancellation_token.when_cancelled() => nothing_to_do(),
        }

        Ok(())
    }

    pub fn order_created_notify(&self, order: &OrderRef) {
        if let Some((_, tx)) = self.orders_created_events.remove(&order.client_order_id()) {
            let _ = tx.send(());
        }
    }
}
