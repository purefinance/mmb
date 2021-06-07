use std::fmt::{Display, Formatter};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use itertools::Itertools;
use log::{error, trace, warn};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::{broadcast, oneshot};

use crate::core::disposition_execution::trade_limit::is_enough_amount_and_cost;
use crate::core::disposition_execution::{
    CompositeOrder, OrderRecord, OrdersState, PriceSlot, TradeCycle, TradingContext,
};
use crate::core::exchanges::cancellation_token::CancellationToken;
use crate::core::exchanges::common::{
    Amount, CurrencyPair, ExchangeAccountId, Price, TradePlaceAccount,
};
use crate::core::exchanges::events::ExchangeEvent;
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::exchanges::general::request_type::RequestType;
use crate::core::explanation::{Explanation, WithExplanation};
use crate::core::lifecycle::trading_engine::{EngineContext, Service};
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::orders::event::OrderEventType;
use crate::core::orders::order::{
    ClientOrderId, OrderCreating, OrderExecutionType, OrderHeader, OrderSide, OrderSnapshot,
    OrderStatus, OrderType, ReservationId,
};
use crate::core::orders::pool::OrderRef;
use crate::core::{nothing_to_do, DateTime};
use crate::strategies::disposition_strategy::DispositionStrategy;

static DISPOSITION_EXECUTOR: &str = "DispositionExecutor";
static DISPOSITION_EXECUTOR_TARGET_REQUESTS_GROUP: &str = "DispositionExecutor_Target";
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
    pub fn new(
        engine_ctx: Arc<EngineContext>,
        events_receiver: broadcast::Receiver<ExchangeEvent>,
        local_snapshots_service: LocalSnapshotsService,
        target_eai: ExchangeAccountId,
        target_currency_pair: CurrencyPair,
        strategy: Arc<DispositionStrategy>,
        cancellation_token: CancellationToken,
    ) -> Self {
        let (work_finished_sender, receiver) = oneshot::channel();

        tokio::spawn(async move {
            let mut disposition_executor = DispositionExecutor::new(
                engine_ctx,
                events_receiver,
                local_snapshots_service,
                target_eai,
                target_currency_pair,
                strategy,
                work_finished_sender,
                cancellation_token,
            );

            if let Err(_error) = disposition_executor.start().await {
                // TODO handle errors
            };
        });

        DispositionExecutorService {
            work_finished_receiver: Mutex::new(Some(receiver)),
        }
    }
}

impl Service for DispositionExecutorService {
    fn name(&self) -> &str {
        DISPOSITION_EXECUTOR
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        let work_finished_receiver = self.work_finished_receiver.lock().take();
        if work_finished_receiver.is_none() {
            warn!("'work_finished_receiver' wasn't created when started graceful shutdown in DispositionExecutor");
        }

        work_finished_receiver
    }
}

struct DispositionExecutor {
    engine_ctx: Arc<EngineContext>,
    target_eai: ExchangeAccountId,
    target_currency_pair_metadata: Arc<CurrencyPairMetadata>,
    events_receiver: broadcast::Receiver<ExchangeEvent>,
    local_snapshots_service: LocalSnapshotsService,
    orders_state: OrdersState,
    strategy: Arc<DispositionStrategy>,
    work_finished_sender: Option<oneshot::Sender<Result<()>>>,
    cancellation_token: CancellationToken,
}

impl DispositionExecutor {
    pub fn new(
        engine_ctx: Arc<EngineContext>,
        events_receiver: broadcast::Receiver<ExchangeEvent>,
        local_snapshots_service: LocalSnapshotsService,
        target_eai: ExchangeAccountId,
        target_currency_pair: CurrencyPair,
        strategy: Arc<DispositionStrategy>,
        work_finished_sender: oneshot::Sender<Result<()>>,
        cancellation_token: CancellationToken,
    ) -> Self {
        let target_currency_pair_metadata = engine_ctx
            .exchanges
            .get(&target_eai)
            .expect("Target exchange should exists")
            .get_currency_pair_metadata(&target_currency_pair)
            .expect("Currency pair metadata should exists for target trading place");

        DispositionExecutor {
            engine_ctx,
            events_receiver,
            local_snapshots_service,
            target_eai,
            target_currency_pair_metadata,
            orders_state: OrdersState::new(),
            strategy,
            work_finished_sender: Some(work_finished_sender),
            cancellation_token,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        let mut trading_context: Option<TradingContext> = None;

        loop {
            let event = tokio::select! {
                event_res = self.events_receiver.recv() => event_res.context("Error during receiving event in DispositionExecutor::start()")?,
                _ = self.cancellation_token.when_cancelled() => {
                    let _ = self.work_finished_sender.take().ok_or(anyhow!("Can't take `work_finished_sender` in DispositionExecutor"))?.send(Ok(()));
                    return Ok(());
                }
            };

            self.handle_event(event, &mut trading_context)?;
        }
    }

    fn handle_event(
        &mut self,
        event: ExchangeEvent,
        last_trading_context: &mut Option<TradingContext>,
    ) -> Result<()> {
        let init_estimation = prepare_estimate_trading_context(&event);

        match event {
            ExchangeEvent::OrderBookEvent(order_book_event) => {
                let _ = self.local_snapshots_service.update(order_book_event);
            }
            ExchangeEvent::OrderEvent(order_event) => {
                if order_event.order.is_external_order() {
                    return Ok(());
                }

                let order = &order_event.order;
                match order_event.event_type {
                    OrderEventType::CreateOrderSucceeded => nothing_to_do(),
                    OrderEventType::CreateOrderFailed => {
                        let price_slot = self.get_price_slot(order);
                        let price_slot = match price_slot {
                            None => return Ok(()),
                            Some(v) => v,
                        };

                        self.finish_order(order, price_slot)?;
                    }
                    OrderEventType::OrderFilled { ref cloned_order } => {
                        let price_slot = self.get_price_slot(order);
                        let price_slot = match price_slot {
                            None => return Ok(()),
                            Some(v) => v,
                        };

                        // TODO recalculate balances on order fill when BalanceManager will be implemented
                        if cloned_order.status() == OrderStatus::Completed {
                            return Ok(());
                        }

                        self.handle_order_fill(cloned_order, price_slot)?;
                    }
                    OrderEventType::OrderCompleted { ref cloned_order } => {
                        let price_slot = self.get_price_slot(order);
                        let price_slot = match price_slot {
                            None => return Ok(()),
                            Some(v) => v,
                        };

                        self.handle_order_fill(cloned_order, price_slot)?;
                        self.finish_order(order, price_slot)?;
                    }
                    OrderEventType::CancelOrderSucceeded => {
                        let price_slot = self.get_price_slot(order);
                        let price_slot = match price_slot {
                            None => return Ok(()),
                            Some(v) => v,
                        };

                        self.finish_order(order, price_slot)?;
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

        let mut new_trading_context =
            estimate_trading_context(&init_estimation, &self.local_snapshots_service)?;

        if last_trading_context == &mut new_trading_context {
            return Ok(());
        }

        self.synchronize_price_slots_for_trading_context(&mut new_trading_context)?;
        *last_trading_context = new_trading_context;

        Ok(())
    }

    fn synchronize_price_slots_for_trading_context(
        &mut self,
        trading_context: &mut Option<TradingContext>,
    ) -> Result<()> {
        for (side, state_by_side) in self.orders_state.by_side.iter() {
            let trading_context_by_side =
                match trading_context.as_mut().map(|x| &mut x.by_side[side]) {
                    None => continue,
                    Some(v) => v,
                };

            self.synchronize_price_slots_for_list(
                &state_by_side.slots,
                &mut trading_context_by_side.estimating[..],
                trading_context_by_side.max_amount,
            )?
        }

        // TODO save explanations
        Ok(())
    }

    fn synchronize_price_slots_for_list(
        &self,
        slots: &[PriceSlot],
        estimating: &mut [WithExplanation<Option<TradeCycle>>],
        max_amount: Decimal,
    ) -> Result<()> {
        if slots.len() != estimating.len() {
            bail!("TargetExchangeAccountId {} slots count is different is trading context ({}) and DispositionExecutor state ({})", self.target_eai, estimating.len(), slots.len());
        }

        for level_index in 0..slots.len() {
            let price_slot = &slots[level_index];
            let with_explanation = &mut estimating[level_index];

            let (trade_cycle, explanation) = with_explanation.as_mut_all();

            self.synchronize_price_slot(trade_cycle, price_slot, max_amount, explanation)?;
        }

        Ok(())
    }

    fn synchronize_price_slot(
        &self,
        new_estimating: &Option<TradeCycle>,
        price_slot: &PriceSlot,
        max_amount: Decimal,
        explanation: &mut Explanation,
    ) -> Result<()> {
        let composite_order = &price_slot.order;
        trace!(
            "Starting synchronize price slot {} {}",
            price_slot.id,
            composite_order.borrow().side
        );

        if self
            .engine_ctx
            .exchange_blocker
            .is_blocked(&self.target_eai)
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
        let new_estimating_target = &new_estimating.disposition;

        let composite_order_ref = composite_order.borrow();
        if composite_order_ref.side != new_estimating_target.side() {
            panic!(
                "Unmatched orders side. New disposition {:?}. Current composite order {:?}",
                new_estimating_target, composite_order_ref
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

        let desired_amount = new_estimating_target.order.amount;
        if new_estimating_target.order.price == composite_order_ref.price {
            explanation.add_reason(format!(
                "New price == old price ({})",
                composite_order_ref.price
            ));

            let remaining_amount = composite_order_ref.remaining_amount();
            if remaining_amount >= desired_amount {
                let desired_amount_with_allowed_deviation =
                    desired_amount * (dec!(1) + ALLOWED_AMOUNT_DEVIATION_RATE);

                if remaining_amount > desired_amount_with_allowed_deviation {
                    explanation.add_reason(format!(
                        "Existing amount ({}) > desired amount + allowed deviation ({})",
                        remaining_amount, desired_amount_with_allowed_deviation
                    ));

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
                    explanation.add_reason(format!("Desired amount  ({}) <= existing amount ({}) <= desired amount + allowed deviation ({})", desired_amount, remaining_amount, desired_amount_with_allowed_deviation));
                }
            }

            drop(composite_order_ref);
            self.try_create_order(
                desired_amount - remaining_amount,
                price_slot,
                new_estimating,
                max_amount,
                explanation,
            )?;
        } else {
            explanation.add_reason(format!(
                "New price ({}) != old price ({})",
                new_estimating_target.order.price, composite_order_ref.price
            ));

            if composite_order_ref.orders.is_empty() {
                drop(composite_order_ref);
                self.try_create_order(
                    desired_amount,
                    price_slot,
                    new_estimating,
                    max_amount,
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

        trace!(
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
            &format!("Cancelling all orders because {}", cause),
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

        trace!("start_cancelling_orders: begin ({})", explanation_msg);

        order_records.for_each(|or| self.cancel_order(or, explanation));

        trace!("start_cancelling_orders: Finish ({})", explanation_msg);
    }

    fn cancel_order(&self, order_record: &mut OrderRecord, explanation: &mut Explanation) {
        if order_record.is_cancellation_requested {
            return;
        }
        order_record.is_cancellation_requested = true;

        let order = order_record.order.clone();
        explanation.add_reason(format!(
            "Cancelling order {} {}",
            order.client_order_id(),
            order.exchange_account_id()
        ));

        trace!("Begin cancel_order {}", order.client_order_id());

        let client_order_id = order.client_order_id();
        let target_request_group_id = order_record.request_group_id.clone();
        let exchange = self.exchange();
        let cancellation_token = self.cancellation_token.clone();
        let _ = tokio::spawn(async move {
            trace!("Begin wait_cancel_order {}", client_order_id);
            exchange
                .wait_cancel_order(
                    order,
                    Some(target_request_group_id),
                    false,
                    cancellation_token,
                )
                .await?;
            trace!("Finished wait_cancel_order {}", client_order_id);

            Ok(()) as Result<()>
        });
    }

    fn start_cancelling_orders_with_cause<'a>(
        &self,
        cause: &str,
        order_records: impl Iterator<Item = &'a mut OrderRecord>,
        explanation: &mut Explanation,
    ) {
        self.start_cancelling_orders(
            &format!("Cancelling orders because {}", cause),
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
        explanation: &mut Explanation,
    ) -> Result<()> {
        trace!("Begin try_create_order");

        let side = price_slot.order.borrow().side;
        let new_disposition = &new_estimating.disposition;

        let new_price = new_disposition.order.price;
        let found = self.find_new_order_crossing_existing_orders(new_price, side);
        if let Some(crossed_order) = found {
            let msg = format!("Finished `try_create_order` because there is order {} with price {} that crossing current price {}", crossed_order.client_order_id(),
                                 crossed_order.price(),
                                 new_price
            );
            return log_trace(msg, explanation);
        }

        let new_order_amount = self.calculate_new_order_amount(
            new_disposition.trade_place_account(),
            side,
            desired_amount,
            max_amount,
            explanation,
        );

        if let Err(reason) = is_enough_amount_and_cost(
            new_disposition,
            new_order_amount,
            true,
            &self.target_currency_pair_metadata,
        ) {
            return log_trace(
                format!("Finished `try_create_order` by reason: {}", reason),
                explanation,
            );
        }

        let new_client_order_id = ClientOrderId::unique_id();

        let requests_group_id = self.engine_ctx.timeout_manager.try_reserve_group(
            &self.target_eai,
            GROUP_REQUESTS_COUNT,
            DISPOSITION_EXECUTOR_TARGET_REQUESTS_GROUP.to_string(),
        )?;

        let requests_group_id =
            match requests_group_id {
                None => return log_trace(
                    "Finished `try_create_order` because can't reserve target reservation group",
                    explanation,
                ),
                Some(v) => v,
            };

        // TODO reserve balances

        if !self.engine_ctx.timeout_manager.try_reserve_group_instant(
            &self.target_eai,
            RequestType::CancelOrder,
            Some(requests_group_id),
        )? {
            // TODO unreserve balances

            let _ = self
                .engine_ctx
                .timeout_manager
                .remove_group(&self.target_eai, requests_group_id)?;

            return log_trace(
                "Finished `try_create_order` because can't reserve requests",
                explanation,
            );
        }

        *price_slot.estimating.borrow_mut() = Some(Box::new(new_estimating.clone()));

        let new_order_header = OrderHeader::new(
            new_client_order_id.clone(),
            now(),
            self.target_eai.clone(),
            self.target_currency_pair_metadata.currency_pair(),
            OrderType::Limit,
            new_disposition.side(),
            new_order_amount,
            OrderExecutionType::MakerOnly,
            // TODO fix after implementation balances reservation
            Some(ReservationId::generate()),
            None,
            new_estimating.strategy_name.clone(),
        );

        let exchange = self.exchange();

        let new_order = exchange
            .orders
            .add_simple_initial(new_order_header.clone(), Some(new_disposition.price()));

        price_slot.add_order(
            new_disposition.side(),
            new_disposition.price(),
            new_order,
            requests_group_id,
        );

        explanation.add_reason(format!("Creating order {}", new_client_order_id));

        self.cancellation_token.error_if_cancellation_requested()?;

        {
            let new_client_order_id = new_client_order_id.clone();
            let cancellation_token = self.cancellation_token.clone();
            tokio::spawn(async move {
                trace!("Begin create_order {}", new_client_order_id);

                let order_creating = OrderCreating {
                    header: new_order_header,
                    price: new_price,
                };

                let order_creation_res = exchange
                    .create_order(&order_creating, cancellation_token)
                    .await;
                match order_creation_res {
                    Ok(_) => return,
                    Err(_) => { /* TODO handle error occurred during order creation */ }
                }

                trace!("Finished create_order {}", new_client_order_id);
            });
        }

        trace!("Begin try_create_order {}", new_client_order_id);
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
            for (_, order_record) in &slot.order.borrow().orders {
                let order = &order_record.order;
                if order.is_finished() && is_crossing(order) {
                    return Some(order.clone());
                }
            }
        }

        return None;
    }

    fn calculate_new_order_amount(
        &self,
        _trade_place_account: TradePlaceAccount,
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

        explanation.add_reason(format!("max_amount {} total_remaining_amount {} high_priority_amount {} balance_quota {} new_order_amount {}", max_amount, total_remaining_amount, high_priority_amount, balance_quota, new_amount));

        new_amount
    }

    fn get_price_slot(&self, order: &OrderRef) -> Option<&PriceSlot> {
        let side = order.side();
        let price_slot = self.orders_state.by_side[side].find_price_slot(order);
        if price_slot.is_some() {
            return price_slot;
        }

        error!(
            "Can't find order with client_order_id {} {} in orders state of DispositionExecutor",
            order.client_order_id(),
            self.target_eai
        );
        return None;
    }

    fn finish_order(&self, order: &OrderRef, price_slot: &PriceSlot) -> Result<()> {
        self.unreserve_target_order_amount(order, price_slot);
        self.remove_target_request_group(order, price_slot)?;

        price_slot.remove_order(order);

        Ok(())
    }
    fn unreserve_target_order_amount(&self, _order: &OrderRef, _price_slot: &PriceSlot) {
        // TODO needed implementation after BalanceManager
    }
    fn remove_target_request_group(&self, order: &OrderRef, price_slot: &PriceSlot) -> Result<()> {
        let request_group_id =
            price_slot.order.borrow().orders[&order.client_order_id()].request_group_id;

        let _ = self
            .engine_ctx
            .timeout_manager
            .remove_group(&self.target_eai, request_group_id)?;
        Ok(())
    }

    fn handle_order_fill(
        &self,
        cloned_order: &Arc<OrderSnapshot>,
        price_slot: &PriceSlot,
    ) -> Result<()> {
        trace!("Begin handle_order_fill");

        let result = self.strategy.handle_order_fill(
            cloned_order,
            price_slot,
            &self.target_eai,
            self.cancellation_token.clone(),
        );

        trace!("Finish handle_order_fill");
        result
    }

    fn exchange(&self) -> Arc<Exchange> {
        self.engine_ctx
            .exchanges
            .get(&self.target_eai)
            .expect("Target exchange for strategy should exists")
            .value()
            .clone()
    }
}

struct InitEstimation {
    event_time: DateTime,
}

fn prepare_estimate_trading_context(event: &ExchangeEvent) -> Option<InitEstimation> {
    match event {
        ExchangeEvent::OrderBookEvent(order_book_event) => Some(InitEstimation {
            event_time: order_book_event.creation_time,
        }),
        ExchangeEvent::LiquidationPrice(liquidation_price) => Some(InitEstimation {
            event_time: liquidation_price.event_creation_time,
        }),
        _ => None,
    }
}

// TODO implement
fn estimate_trading_context(
    prepare_state: &Option<InitEstimation>,
    _local_snapshots_service: &LocalSnapshotsService,
) -> Result<Option<TradingContext>> {
    let _prepare_state = match prepare_state {
        None => return Ok(None),
        Some(v) => v,
    };

    todo!()
}

fn get_cancelling_orders<'a>(
    order_records: impl Iterator<Item = &'a mut OrderRecord>,
    desired_amount: Amount,
    remaining_amount: Amount,
) -> Vec<&'a mut OrderRecord> {
    trace!("Started get_cancelling_orders");

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

    trace!("Finished get_cancelling_orders");

    cancelling_orders
}

fn now() -> DateTime {
    Utc::now()
}

#[inline(always)]
fn log_trace<'a>(msg: impl AsRef<str>, explanation: &mut Explanation) -> Result<()> {
    let msg = msg.as_ref();

    trace!("{}", msg);
    explanation.add_reason(msg);

    Ok(())
}
