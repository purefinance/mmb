use std::fmt::{Display, Formatter};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use itertools::Itertools;
use mmb_utils::infrastructure::{SpawnFutureFlags, WithExpect};
use mmb_utils::{nothing_to_do, DateTime};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::{broadcast, oneshot};

use crate::disposition_execution::strategy::DispositionStrategy;
use crate::disposition_execution::trading_context_calculation::calculate_trading_context;
use crate::exchanges::general::exchange::Exchange;
use crate::exchanges::general::request_type::RequestType;
use crate::explanation::{Explanation, WithExplanation};
use crate::lifecycle::trading_engine::{EngineContext, Service};
use crate::misc::reserve_parameters::ReserveParameters;
use crate::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::{
    disposition_execution::trade_limit::is_enough_amount_and_cost, infrastructure::spawn_future,
};
use crate::{
    disposition_execution::{
        CompositeOrder, OrderRecord, OrdersState, PriceSlot, TradeCycle, TradingContext,
    },
    statistic_service::StatisticService,
};
use chrono::Duration;
use mmb_domain::events::ExchangeEvent;
use mmb_domain::exchanges::symbol::Symbol;
use mmb_domain::market::CurrencyPair;
use mmb_domain::market::{ExchangeAccountId, MarketAccountId};
use mmb_domain::order::event::OrderEventType;
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_domain::order::snapshot::{
    ClientOrderId, OrderExecutionType, OrderHeader, OrderSide, OrderSnapshot, OrderStatus,
    OrderType,
};
use mmb_utils::cancellation_token::CancellationToken;

static DISPOSITION_EXECUTOR: &str = "DispositionExecutor";
static DISPOSITION_EXECUTOR_REQUESTS_GROUP: &str = "DispositionExecutorRG";
const ALLOWED_AMOUNT_DEVIATION_RATE: Decimal = dec!(0.001);
const GROUP_REQUESTS_COUNT: usize = 4;

struct DisplaySmallOrder {
    price: Decimal,
    amount: Decimal,
}

impl Display for DisplaySmallOrder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.price, self.amount)
    }
}

pub struct DispositionExecutorService {
    work_finished_receiver: Mutex<Option<oneshot::Receiver<Result<()>>>>,
}

impl DispositionExecutorService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine_ctx: Arc<EngineContext>,
        events_receiver: broadcast::Receiver<ExchangeEvent>,
        local_snapshots_service: LocalSnapshotsService,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        strategy: Box<dyn DispositionStrategy>,
        cancellation_token: CancellationToken,
        statistics: Arc<StatisticService>,
    ) -> Arc<Self> {
        let (work_finished_sender, receiver) = oneshot::channel();

        let action = async move {
            let mut disposition_executor = DispositionExecutor::new(
                engine_ctx,
                events_receiver,
                local_snapshots_service,
                exchange_account_id,
                currency_pair,
                strategy,
                work_finished_sender,
                cancellation_token,
                statistics,
            );

            disposition_executor.start().await
        };
        spawn_future(
            "Start disposition executor",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            action,
        );

        Arc::new(DispositionExecutorService {
            work_finished_receiver: Mutex::new(Some(receiver)),
        })
    }
}

impl Service for DispositionExecutorService {
    fn name(&self) -> &str {
        DISPOSITION_EXECUTOR
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        let work_finished_receiver = self.work_finished_receiver.lock().take();
        if work_finished_receiver.is_none() {
            log::warn!("'work_finished_receiver' wasn't created when started graceful shutdown in DispositionExecutor");
        }

        work_finished_receiver
    }
}

struct DispositionExecutor {
    engine_ctx: Arc<EngineContext>,
    exchange_account_id: ExchangeAccountId,
    symbol: Arc<Symbol>,
    events_receiver: broadcast::Receiver<ExchangeEvent>,
    local_snapshots_service: LocalSnapshotsService,
    orders_state: OrdersState,
    strategy: Box<dyn DispositionStrategy>,
    work_finished_sender: Option<oneshot::Sender<Result<()>>>,
    cancellation_token: CancellationToken,
    statistics: Arc<StatisticService>,
}

impl DispositionExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine_ctx: Arc<EngineContext>,
        events_receiver: broadcast::Receiver<ExchangeEvent>,
        local_snapshots_service: LocalSnapshotsService,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        strategy: Box<dyn DispositionStrategy>,
        work_finished_sender: oneshot::Sender<Result<()>>,
        cancellation_token: CancellationToken,
        statistics: Arc<StatisticService>,
    ) -> Self {
        let symbol = engine_ctx
            .exchanges
            .get(&exchange_account_id)
            .expect("Target exchange should exists")
            .get_symbol(currency_pair)
            .expect("Currency pair symbol should exists for target trading place");

        DispositionExecutor {
            engine_ctx,
            events_receiver,
            local_snapshots_service,
            exchange_account_id,
            symbol,
            orders_state: OrdersState::new(),
            strategy,
            work_finished_sender: Some(work_finished_sender),
            cancellation_token,
            statistics,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        let mut trading_context: Option<TradingContext> = None;

        loop {
            let event = tokio::select! {
                event_res = self.events_receiver.recv() => event_res.map_err(|e| anyhow!("Error during receiving event in DispositionExecutor::start(). Error: {e}."))?,
                _ = self.cancellation_token.when_cancelled() => {
                    let _ = self.work_finished_sender.take().ok_or_else(|| anyhow!("Can't take `work_finished_sender` in DispositionExecutor"))?.send(Ok(()));
                    return Ok(());
                }
            };

            self.handle_event(&event, &mut trading_context)?;
        }
    }

    fn handle_event(
        &mut self,
        event: &ExchangeEvent,
        last_trading_context: &mut Option<TradingContext>,
    ) -> Result<()> {
        let now = now();
        let need_recalculate_trading_context = self.prepare_estimate_trading_context(event, now);

        match event {
            ExchangeEvent::OrderBookEvent(order_book_event) => {
                let _ = self.local_snapshots_service.update(order_book_event);
            }
            ExchangeEvent::OrderEvent(order_event) => {
                let order = &order_event.order;
                if order.fn_ref(|s| s.header.order_type.is_external_order()) {
                    return Ok(());
                }

                match order_event.event_type {
                    OrderEventType::CreateOrderSucceeded => nothing_to_do(),
                    OrderEventType::CreateOrderFailed => {
                        let client_order_id = order.client_order_id();
                        log::trace!("Started handling event CreateOrderFailed {client_order_id} in DispositionExecutor");
                        let price_slot = self.get_price_slot(order);
                        let price_slot = match price_slot {
                            None => return Ok(()),
                            Some(v) => v,
                        };

                        self.finish_order(order, price_slot)?;
                        log::trace!("Finished handling event CreateOrderFailed {client_order_id} in DispositionExecutor");
                    }
                    OrderEventType::OrderFilled { ref cloned_order } => {
                        log::trace!(
                            "Started handling event OrderFilled {} in DispositionExecutor",
                            cloned_order.header.client_order_id
                        );
                        let price_slot = self.get_price_slot(order);
                        if let Some(price_slot) = price_slot {
                            self.engine_ctx.balance_manager.lock().order_was_filled(
                                self.strategy.configuration_descriptor(),
                                cloned_order,
                            );

                            if cloned_order.status() == OrderStatus::Completed {
                                return Ok(());
                            }

                            self.handle_order_fill(cloned_order, price_slot)?;
                        }
                        log::trace!(
                            "Finished handling event OrderFilled {} in DispositionExecutor",
                            cloned_order.header.client_order_id
                        );
                    }
                    OrderEventType::OrderCompleted { ref cloned_order } => {
                        log::trace!(
                            "Started handling event OrderCompleted {} in DispositionExecutor",
                            cloned_order.header.client_order_id
                        );
                        let price_slot = self.get_price_slot(order);
                        if let Some(price_slot) = price_slot {
                            self.handle_order_fill(cloned_order, price_slot)?;
                            self.finish_order(order, price_slot)?;
                        }
                        log::trace!(
                            "Finished handling event OrderCompleted {} in DispositionExecutor",
                            cloned_order.header.client_order_id
                        );
                    }
                    OrderEventType::CancelOrderSucceeded => {
                        let client_order_id = order.client_order_id();
                        log::trace!("Started handling event CancelOrderSucceeded {client_order_id} in DispositionExecutor");

                        let price_slot = self.get_price_slot(order);
                        let price_slot = match price_slot {
                            None => return Ok(()),
                            Some(v) => v,
                        };

                        self.finish_order(order, price_slot)?;
                        log::trace!("Finished handling event CancelOrderSucceeded {client_order_id} in DispositionExecutor");
                    }
                    OrderEventType::CancelOrderFailed => {
                        //We should use WaitCancelOrder everywhere, so we don't need to
                        //manually call CancelOrder if CancelOrderFailed
                        //like we used to in a event-driven approach

                        // TODO save state to Database
                    }
                }
            }
            _ => nothing_to_do(),
        };

        let mut new_trading_context = estimate_trading_context(
            need_recalculate_trading_context,
            event,
            self.strategy.as_mut(),
            &self.local_snapshots_service,
            now,
        )?;

        if last_trading_context == &mut new_trading_context {
            return Ok(());
        }

        self.synchronize_price_slots_for_trading_context(&mut new_trading_context, now)?;
        *last_trading_context = new_trading_context;

        Ok(())
    }

    fn synchronize_price_slots_for_trading_context(
        &mut self,
        trading_context: &mut Option<TradingContext>,
        now: DateTime,
    ) -> Result<()> {
        let trading_context = match trading_context {
            None => return Ok(()),
            Some(v) => v,
        };

        for (side, state_by_side) in self.orders_state.by_side.iter() {
            let trading_context_by_side = &mut trading_context.by_side[side];

            self.synchronize_price_slots_for_list(
                &state_by_side.slots,
                &mut trading_context_by_side.estimating[..],
                trading_context_by_side.max_amount,
                now,
            )?
        }

        let explanations = trading_context.get_explanations(
            self.exchange_account_id.exchange_id,
            self.symbol.currency_pair(),
        );

        self.engine_ctx
            .event_recorder
            .save(explanations)
            .unwrap_or_else(|err| log::error!("unable save explanations: {err}"));

        Ok(())
    }

    fn synchronize_price_slots_for_list(
        &self,
        slots: &[PriceSlot],
        estimating: &mut [WithExplanation<Option<TradeCycle>>],
        max_amount: Decimal,
        now: DateTime,
    ) -> Result<()> {
        if slots.len() != estimating.len() {
            bail!("ExchangeAccountId {} slots count is different is trading context ({}) and DispositionExecutor state ({})", self.exchange_account_id, estimating.len(), slots.len());
        }

        for level_index in 0..slots.len() {
            let price_slot = &slots[level_index];
            let with_explanation = &mut estimating[level_index];

            let (trade_cycle, explanation) = with_explanation.as_mut_all();

            self.synchronize_price_slot(trade_cycle, price_slot, max_amount, now, explanation)?;
        }

        Ok(())
    }

    fn synchronize_price_slot(
        &self,
        new_estimating: &Option<TradeCycle>,
        price_slot: &PriceSlot,
        max_amount: Decimal,
        now: DateTime,
        explanation: &mut Explanation,
    ) -> Result<()> {
        let composite_order = &price_slot.order;
        log::trace!(
            "Starting synchronize price slot {} {}",
            price_slot.id,
            composite_order.borrow().side
        );

        if self
            .engine_ctx
            .exchange_blocker
            .is_blocked(self.exchange_account_id)
        {
            self.start_cancelling_all_orders(
                "target exchange is locked",
                &mut composite_order.borrow_mut(),
                explanation,
            );

            return Ok(());
        }

        // TODO close position if needed

        let new_estimating = match new_estimating {
            None => {
                match *price_slot.estimating.borrow() {
                    None => explanation.add_reason("New estimation is not trade"),
                    Some(_) => {
                        self.start_cancelling_all_orders(
                            "new estimation: not trade orders in price slot",
                            &mut composite_order.borrow_mut(),
                            explanation,
                        );
                    }
                }

                return Ok(());
            }
            Some(v) => v,
        };
        let new_estimating_disposition = &new_estimating.disposition;

        let composite_order_ref = composite_order.borrow();
        if composite_order_ref.side != new_estimating_disposition.side() {
            panic!(
                "Unmatched orders side. New disposition {new_estimating_disposition:?}. Current composite order {composite_order_ref:?}"
            );
        }

        explanation.add_reason(format!(
            "Price slot orders: {}",
            composite_order_ref
                .orders
                .values()
                .map(|or| or.order.fn_ref(|x| DisplaySmallOrder {
                    price: x.price(),
                    amount: x.amount()
                }))
                .join(", ")
        ));

        let desired_amount = new_estimating_disposition.order.amount;
        if new_estimating_disposition.order.price == composite_order_ref.price {
            explanation.add_reason(format!(
                "New price == old price ({})",
                composite_order_ref.price
            ));

            let remaining_amount = composite_order_ref.remaining_amount();
            if remaining_amount >= desired_amount {
                let desired_amount_with_allowed_deviation =
                    desired_amount * (dec!(1) + ALLOWED_AMOUNT_DEVIATION_RATE);

                if remaining_amount > desired_amount_with_allowed_deviation {
                    explanation.add_reason(format!("Existing amount ({remaining_amount}) > desired amount + allowed deviation ({desired_amount_with_allowed_deviation})"));

                    drop(composite_order_ref);
                    let mut composite_order_mut = price_slot.order.borrow_mut();
                    let cancelling_order_records = get_cancelling_orders(
                        composite_order_mut.orders.values_mut(),
                        desired_amount,
                        remaining_amount,
                    );

                    self.start_cancelling_orders_with_cause(
                        "there are outside order records",
                        cancelling_order_records.into_iter(),
                        explanation,
                    );

                    return Ok(());
                } else {
                    explanation.add_reason(format!("Desired amount  ({desired_amount}) <= existing amount ({remaining_amount}) <= desired amount + allowed deviation ({desired_amount_with_allowed_deviation})"));
                }
            }

            drop(composite_order_ref);
            self.try_create_order(
                desired_amount - remaining_amount,
                price_slot,
                new_estimating,
                max_amount,
                now,
                explanation,
            )?;
        } else {
            explanation.add_reason(format!(
                "New price ({}) != old price ({})",
                new_estimating_disposition.order.price, composite_order_ref.price
            ));

            if composite_order_ref.orders.is_empty() {
                drop(composite_order_ref);
                self.try_create_order(
                    desired_amount,
                    price_slot,
                    new_estimating,
                    max_amount,
                    now,
                    explanation,
                )?;
            } else {
                explanation.add_reason("Cancelling existing orders");

                drop(composite_order_ref);
                self.start_cancelling_all_orders(
                    "needed order recreation",
                    &mut price_slot.order.borrow_mut(),
                    explanation,
                );
            }
        }

        log::trace!(
            "Finish synchronize price slot {} {}",
            price_slot.id,
            price_slot.order.borrow().side
        );

        Ok(())
    }

    fn start_cancelling_all_orders(
        &self,
        cause: &str,
        composite_order: &mut CompositeOrder,
        explanation: &mut Explanation,
    ) {
        let orders_records = composite_order.orders.values_mut();

        self.start_cancelling_orders(
            &format!("Cancelling all orders because {cause}"),
            orders_records,
            explanation,
        )
    }

    fn start_cancelling_orders<'a>(
        &self,
        explanation_msg: &str,
        order_records: impl Iterator<Item = &'a mut OrderRecord>,
        explanation: &mut Explanation,
    ) {
        explanation.add_reason(explanation_msg);

        log::trace!("start_cancelling_orders: begin ({explanation_msg})");

        order_records.for_each(|or| self.cancel_order(or, explanation));

        log::trace!("start_cancelling_orders: Finish ({explanation_msg})");
    }

    fn cancel_order(&self, order_record: &mut OrderRecord, explanation: &mut Explanation) {
        if order_record.is_cancellation_requested {
            log::trace!(
                "Trying cancelling order {}. Cancellation was started already.",
                order_record.order.client_order_id()
            );
            return;
        }
        order_record.is_cancellation_requested = true;

        let order = order_record.order.clone();
        let client_order_id = order.client_order_id();
        explanation.add_reason(format!(
            "Cancelling order {client_order_id} {}",
            order.exchange_account_id()
        ));

        log::trace!("Begin cancel_order {client_order_id}");

        let request_group_id = order_record.request_group_id;
        let exchange = self.exchange();
        let cancellation_token = self.cancellation_token.clone();

        let action = async move {
            log::trace!("Begin wait_cancel_order {client_order_id}");
            exchange
                .wait_cancel_order(order, Some(request_group_id), false, cancellation_token)
                .await?;
            log::trace!("Finished wait_cancel_order {client_order_id}");

            Ok(())
        };
        spawn_future(
            "Start wait_cancel_order from DispositionExecutor::cancel_order()",
            SpawnFutureFlags::empty(),
            action,
        );
    }

    fn start_cancelling_orders_with_cause<'a>(
        &self,
        cause: &str,
        order_records: impl Iterator<Item = &'a mut OrderRecord>,
        explanation: &mut Explanation,
    ) {
        self.start_cancelling_orders(
            &format!("Cancelling orders because {cause}"),
            order_records,
            explanation,
        )
    }

    fn try_create_order(
        &self,
        desired_amount: Decimal,
        price_slot: &PriceSlot,
        new_estimating: &TradeCycle,
        max_amount: Decimal,
        now: DateTime,
        explanation: &mut Explanation,
    ) -> Result<()> {
        log::trace!("Begin try_create_order");

        let side = price_slot.order.borrow().side;
        let new_disposition = &new_estimating.disposition;

        let new_price = new_disposition.order.price;
        let found = self.find_new_order_crossing_existing_orders(new_price, side);
        if let Some(crossed_order) = found {
            let msg = format!("Finished `try_create_order` because there is order {} with price {} that crossing current price {new_price}", crossed_order.client_order_id(), crossed_order.price());
            return log_trace(msg, explanation);
        }

        let new_order_amount = self.calculate_new_order_amount(
            new_disposition.market_account_id(),
            side,
            desired_amount,
            max_amount,
            explanation,
        );

        if let Err(reason) =
            is_enough_amount_and_cost(new_disposition, new_order_amount, true, &self.symbol)
        {
            return log_trace(
                format!("Finished `try_create_order` by reason: {reason}"),
                explanation,
            );
        }

        let new_client_order_id = ClientOrderId::unique_id();

        let requests_group_id = self.engine_ctx.timeout_manager.try_reserve_group(
            self.exchange_account_id,
            GROUP_REQUESTS_COUNT,
            DISPOSITION_EXECUTOR_REQUESTS_GROUP.to_string(),
        );

        let requests_group_id = match requests_group_id {
            None => {
                return log_trace(
                    "Finished `try_create_order` because can't reserve reservation group",
                    explanation,
                )
            }
            Some(v) => v,
        };

        let target_reserve_parameters = ReserveParameters::new(
            self.strategy.configuration_descriptor(),
            self.exchange_account_id,
            self.symbol.clone(),
            new_disposition.side(),
            new_disposition.price(),
            new_order_amount,
        );

        let reservation_id;
        *explanation = {
            let mut explanation = Some(explanation.clone());

            // This expect can happened if try_reserve() sets the explanation to None
            let explanation_err_msg =
                "DispositionExecutor::try_create_order(): Explanation should be non None here";

            reservation_id = match self
                .engine_ctx
                .balance_manager
                .lock()
                .try_reserve(&target_reserve_parameters, &mut explanation)
            {
                Some(reservation_id) => reservation_id,
                None => {
                    self.engine_ctx
                        .timeout_manager
                        .remove_group(self.exchange_account_id, requests_group_id);

                    return log_trace(format!("Finished try_create_order because can't reserve balance {new_order_amount}"),
                        &mut explanation.expect(explanation_err_msg),
                    );
                }
            };

            explanation.expect(explanation_err_msg)
        };

        if !self.engine_ctx.timeout_manager.try_reserve_group_instant(
            self.exchange_account_id,
            RequestType::CreateOrder,
            Some(requests_group_id),
        ) {
            self.engine_ctx
                .balance_manager
                .lock()
                .unreserve_rest(
                    reservation_id,
                )
                .with_expect(|| format!("DispositionExecutor::try_create_order() failed to unreserve_rest for: {reservation_id:?}"));

            let _ = self
                .engine_ctx
                .timeout_manager
                .remove_group(self.exchange_account_id, requests_group_id);

            return log_trace(
                "Finished `try_create_order` because can't reserve requests",
                explanation,
            );
        }

        *price_slot.estimating.borrow_mut() = Some(Box::new(new_estimating.clone()));

        let order_header = OrderHeader::new(
            new_client_order_id.clone(),
            self.exchange_account_id,
            self.symbol.currency_pair(),
            OrderType::Limit,
            new_disposition.side(),
            Some(new_disposition.price()),
            new_order_amount,
            OrderExecutionType::MakerOnly,
            Some(reservation_id),
            None,
            new_estimating.strategy_name.clone(),
        );

        let exchange = self.exchange();

        let new_order = exchange.orders.add_simple_initial(
            order_header.clone(),
            now,
            exchange.exchange_client.get_initial_extension_data(),
        );

        price_slot.add_order(
            new_disposition.side(),
            new_disposition.price(),
            new_order,
            requests_group_id,
        );

        explanation.add_reason(format!("Creating order {new_client_order_id}"));

        self.cancellation_token.error_if_cancellation_requested()?;

        {
            let new_client_order_id = new_client_order_id.clone();
            let cancellation_token = self.cancellation_token.clone();

            let action = async move {
                log::trace!("Begin create_order {new_client_order_id}");

                exchange
                    .create_order(order_header, Some(requests_group_id), cancellation_token)
                    .await?;

                log::trace!("Finished create_order {new_client_order_id}");

                Ok(())
            };

            spawn_future(
                "create_order in blocking try_create_order",
                SpawnFutureFlags::empty(),
                action,
            );
        }

        log::trace!("Begin try_create_order {new_client_order_id}");

        Ok(())
    }

    fn find_new_order_crossing_existing_orders(
        &self,
        new_order_price: Price,
        side: OrderSide,
    ) -> Option<OrderRef> {
        let buy_comparator = &|order: &OrderRef| order.price() <= new_order_price;
        let sell_comparator = &|order: &OrderRef| new_order_price <= order.price();

        let is_crossing: &dyn Fn(&OrderRef) -> bool = match side {
            OrderSide::Buy => buy_comparator,
            OrderSide::Sell => sell_comparator,
        };

        for slot in &self.orders_state.by_side[side.change_side()].slots {
            for order_record in slot.order.borrow().orders.values() {
                let order = &order_record.order;
                if order.is_finished() && is_crossing(order) {
                    return Some(order.clone());
                }
            }
        }

        None
    }

    fn calculate_new_order_amount(
        &self,
        _market_account_id: MarketAccountId,
        side: OrderSide,
        desired_amount: Decimal,
        max_amount: Decimal,
        explanation: &mut Explanation,
    ) -> Decimal {
        let total_remaining_amount = self.orders_state.by_side[side].calc_total_remaining_amount();
        // TODO is needed high priority amount?
        let high_priority_amount = dec!(0);
        let balance_quota = max_amount - total_remaining_amount;
        let new_amount = desired_amount.min(balance_quota).max(dec!(0)) - high_priority_amount;

        explanation.add_reason(format!("max_amount {max_amount} total_remaining_amount {total_remaining_amount} high_priority_amount {high_priority_amount} balance_quota {balance_quota} new_order_amount {new_amount}"));

        new_amount
    }

    fn get_price_slot(&self, order: &OrderRef) -> Option<&PriceSlot> {
        let header = order.fn_ref(|x| x.header.clone());
        let price_slot = self.orders_state.by_side[header.side].find_price_slot(order);
        if price_slot.is_some() {
            return price_slot;
        }

        log::error!(
            "Can't find order with client_order_id {} {} in orders state of DispositionExecutor",
            header.client_order_id,
            self.exchange_account_id
        );
        None
    }

    fn finish_order(&self, order: &OrderRef, price_slot: &PriceSlot) -> Result<()> {
        let client_order_id = order.client_order_id();
        log::trace!("Started DispositionExecutor::finish_order {client_order_id}");
        self.unreserve_order_amount(order, price_slot);
        self.remove_request_group(order, price_slot);

        price_slot.remove_order(order);

        log::trace!("Finished DispositionExecutor::finish_order {client_order_id}");
        Ok(())
    }

    fn unreserve_order_amount(&self, order: &OrderRef, _price_slot: &PriceSlot) {
        let (reservation_id, client_order_id, amount) = order.fn_ref(|x| {
            (
                x.header.reservation_id,
                x.header.client_order_id.clone(),
                x.header.amount,
            )
        });

        let reservation_id = reservation_id.expect("InternalEventsLoop: ReservationId is None");
        self.engine_ctx
            .balance_manager
            .lock()
            .unreserve_by_client_order_id(reservation_id, client_order_id.clone(), amount)
            .with_expect(|| {
                format!("InternalEventsLoop: failed to unreserve order {client_order_id:?}")
            });
    }

    fn remove_request_group(&self, order: &OrderRef, price_slot: &PriceSlot) {
        let request_group_id =
            price_slot.order.borrow().orders[&order.client_order_id()].request_group_id;

        let _ = self
            .engine_ctx
            .timeout_manager
            .remove_group(self.exchange_account_id, request_group_id);
    }

    fn handle_order_fill(
        &self,
        cloned_order: &Arc<OrderSnapshot>,
        price_slot: &PriceSlot,
    ) -> Result<()> {
        log::trace!("Begin handle_order_fill");

        let result = self.strategy.handle_order_fill(
            cloned_order,
            price_slot,
            self.exchange_account_id,
            self.cancellation_token.clone(),
        );

        log::trace!("Finish handle_order_fill");
        result
    }

    fn exchange(&self) -> Arc<Exchange> {
        self.engine_ctx
            .exchanges
            .get(&self.exchange_account_id)
            .expect("Target exchange for strategy should exists")
            .value()
            .clone()
    }

    fn prepare_estimate_trading_context(&self, event: &ExchangeEvent, now: DateTime) -> bool {
        let event_time = match event {
            ExchangeEvent::OrderBookEvent(order_book_event) => order_book_event.creation_time,
            ExchangeEvent::LiquidationPrice(liquidation_price) => {
                liquidation_price.event_creation_time
            }
            _ => return false,
        };

        // max delay for skipping recalculation of trading context and orders synchronization
        let delay_for_skipping_event: Duration = Duration::milliseconds(50);
        if event_time + delay_for_skipping_event < now {
            self.statistics.clone().register_skipped_event();

            return false;
        }

        true
    }
}

fn estimate_trading_context(
    need_recalculate_trading_context: bool,
    event: &ExchangeEvent,
    strategy: &mut dyn DispositionStrategy,
    local_snapshots_service: &LocalSnapshotsService,
    now: DateTime,
) -> Result<Option<TradingContext>> {
    if !need_recalculate_trading_context {
        return Ok(None);
    }

    Ok(calculate_trading_context(
        event,
        strategy,
        local_snapshots_service,
        now,
    ))
}

fn get_cancelling_orders<'a>(
    order_records: impl Iterator<Item = &'a mut OrderRecord>,
    desired_amount: Amount,
    remaining_amount: Amount,
) -> Vec<&'a mut OrderRecord> {
    log::trace!("Started get_cancelling_orders");

    let delta_amount = remaining_amount - desired_amount;

    let mut sorted_order_records = order_records.collect_vec();
    sorted_order_records.sort_by_key(|x| x.order.amount());

    let mut cancelling_orders = Vec::with_capacity(sorted_order_records.len());

    let mut sum = dec!(0);

    for record in sorted_order_records {
        let order = &mut record.order;

        let remaining_order_amount = order.fn_ref(|x| {
            if order.is_finished() {
                dec!(0)
            } else {
                x.amount() - x.filled_amount()
            }
        });

        cancelling_orders.push(record);

        sum += remaining_order_amount;

        if sum >= delta_amount {
            break;
        }
    }

    log::trace!("Finished get_cancelling_orders");

    cancelling_orders
}

fn now() -> DateTime {
    Utc::now()
}

#[inline(always)]
fn log_trace(msg: impl AsRef<str>, explanation: &mut Explanation) -> Result<()> {
    let msg = msg.as_ref();
    log::trace!("{msg}");
    explanation.add_reason(msg);

    Ok(())
}
