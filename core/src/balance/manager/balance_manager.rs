use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::balance::balance_reservation_manager::BalanceReservationManager;
use crate::balance::changes::balance_changes_service::BalanceChangesService;
use crate::balance::manager::balance_reservation::BalanceReservation;
use crate::balance::manager::position_change::PositionChange;
use crate::exchanges::common::{Amount, Price};
use crate::exchanges::common::{CurrencyCode, CurrencyPair, MarketAccountId};
use crate::exchanges::events::ExchangeBalancesAndPositions;
use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
use crate::exchanges::general::symbol::{BeforeAfter, Symbol};
use crate::explanation::Explanation;
use crate::misc::derivative_position::DerivativePosition;
use crate::misc::reserve_parameters::ReserveParameters;
use crate::misc::service_value_tree::ServiceValueTree;
use crate::orders::fill::OrderFill;
use crate::orders::order::{
    ClientOrderId, OrderSide, OrderSnapshot, OrderStatus, OrderType, ReservationId,
};
use crate::orders::pool::OrderRef;
use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::{balance::manager::balances::Balances, exchanges::common::ExchangeAccountId};

use anyhow::{bail, Context, Result};
use futures::future::join_all;
use itertools::Itertools;
use log::log;
use log::Level::{Error, Warn};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::{impl_mock_initializer, nothing_to_do, DateTime};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::database::events::recorder::EventRecorder;
#[cfg(test)]
use crate::MOCK_MUTEX;
#[cfg(test)]
use mockall::automock;

/// The entity for getting information about account balances for selected exchanges
#[derive(Clone)]
pub struct BalanceManager {
    exchange_id_with_restored_positions: HashSet<ExchangeAccountId>,
    balance_reservation_manager: BalanceReservationManager,
    last_order_fills: HashMap<MarketAccountId, OrderFill>,
    balance_changes_service: Option<Arc<BalanceChangesService>>,
    position_differs_times_in_row_by_exchange_id:
        HashMap<ExchangeAccountId, HashMap<CurrencyPair, u32>>,
    event_recorder: Option<Arc<EventRecorder>>,
}

impl BalanceManager {
    pub fn new(
        currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>,
        event_recorder: Option<Arc<EventRecorder>>,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            exchange_id_with_restored_positions: HashSet::new(),
            balance_reservation_manager: BalanceReservationManager::new(
                currency_pair_to_symbol_converter,
            ),
            last_order_fills: HashMap::new(),
            balance_changes_service: None,
            position_differs_times_in_row_by_exchange_id: Default::default(),
            event_recorder,
        }))
    }

    pub fn restore_balance_state(
        &mut self,
        balances: &Balances,
        update_exchange_balances_before_restoring: bool,
    ) {
        if update_exchange_balances_before_restoring {
            if let Some(balances_by_exchange_id) = &balances.balances_by_exchange_id {
                for (exchange_account_id, balance) in balances_by_exchange_id {
                    self.balance_reservation_manager
                        .virtual_balance_holder
                        .update_balances(*exchange_account_id, balance);
                }
            }
        }

        if let Some(virtual_diff_balances) = &balances.virtual_diff_balances {
            for (request, diff) in virtual_diff_balances.get_as_balances() {
                self.balance_reservation_manager
                    .virtual_balance_holder
                    .add_balance(&request, diff);
            }
        }

        if let (Some(balance_reservations_by_reservation_id), Some(_)) = (
            &balances.balance_reservations_by_reservation_id,
            &balances.reserved_amount,
        ) {
            self.balance_reservation_manager
                .update_reserved_balances(balance_reservations_by_reservation_id);
        }

        if let (Some(amount_limits), Some(position_by_fill_amount)) =
            (&balances.amount_limits, &balances.position_by_fill_amount)
        {
            self.balance_reservation_manager
                .restore_fill_amount_limits(amount_limits.clone(), position_by_fill_amount.clone());
        }

        self.last_order_fills = balances.last_order_fills.clone();
    }

    pub fn get_reservation_ids(&self) -> Vec<ReservationId> {
        self.balance_reservation_manager
            .balance_reservation_storage
            .get_reservation_ids()
    }

    #[cfg(test)]
    pub(crate) fn restore_balance_state_with_reservations_handling(
        &mut self,
        balances: &Balances,
    ) -> Result<()> {
        self.restore_balance_state(balances, false);

        for reservation_id in self.get_reservation_ids() {
            self.unreserve_rest(reservation_id)?;
        }
        Ok(())
    }

    pub fn unreserve_rest(&mut self, reservation_id: ReservationId) -> Result<()> {
        let amount = self
            .balance_reservation_manager
            .get_reservation(reservation_id)
            .with_context(|| format!("Can't find reservation_id: {reservation_id}"))?
            .unreserved_amount;

        self.unreserve(reservation_id, amount)
    }

    pub fn unreserve(&mut self, reservation_id: ReservationId, amount: Amount) -> Result<()> {
        self.balance_reservation_manager
            .unreserve(reservation_id, amount, &None)?;
        self.save_balances();
        Ok(())
    }

    pub fn unreserve_by_client_order_id(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: ClientOrderId,
        amount: Amount,
    ) -> Result<()> {
        self.balance_reservation_manager.unreserve(
            reservation_id,
            amount,
            &Some(client_order_id),
        )?;
        self.save_balances();
        Ok(())
    }

    fn save_balances(&mut self) {
        match &self.event_recorder {
            None => {}
            Some(event_recorder) => {
                let balances = self.get_balances();
                event_recorder
                    .save(balances)
                    .expect("Failure save balances");
            }
        }
    }

    pub fn get_balances(&self) -> Balances {
        let mut balances = self.balance_reservation_manager.get_state();
        balances.last_order_fills = self.last_order_fills.clone();
        balances
    }

    fn save_balance_update(
        &self,
        _whole_balance_before: HashMap<ExchangeAccountId, HashMap<CurrencyCode, Amount>>,
        _whole_balance_after: HashMap<ExchangeAccountId, HashMap<CurrencyCode, Amount>>,
    ) {
        // TODO: fix me when DataRecorder will be added
        // if self.data_recorder.is_none()
        // {
        //     return;
        // }

        let _reservation_clone = self
            .balance_reservation_manager
            .balance_reservation_storage
            .get_all_raw_reservations()
            .clone();

        // var balanceUpdate = new BalanceUpdate(
        //     _dateTimeService.UtcNow,
        //     reservationsClone,
        //     wholeBalanceBefore,
        //     wholeBalanceAfter);

        // _dataRecorder.Save(balanceUpdate);
    }

    fn restore_fill_amount_position(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        positions: &Option<Vec<DerivativePosition>>,
    ) -> Result<()> {
        let positions = if let Some(positions) = positions {
            if positions.is_empty() {
                return Ok(());
            }
            positions
        } else {
            return Ok(());
        };

        let mut position_info_by_symbol = HashMap::new();

        for position_info in positions {
            let currency_pair = position_info.currency_pair;
            let symbol = self
                .balance_reservation_manager
                .exchanges_by_id()
                .get(&exchange_account_id)
                .with_context(|| { format!("symbol not found for exchange with account id {exchange_account_id:?} and currency pair {currency_pair}") })?
                .get_symbol(currency_pair)?;

            if symbol.is_derivative {
                position_info_by_symbol.insert(symbol.clone(), position_info);
            }
        }

        if !self
            .exchange_id_with_restored_positions
            .contains(&exchange_account_id)
        {
            for (symbol, position_info) in position_info_by_symbol {
                self.balance_reservation_manager
                    .restore_fill_amount_position(
                        exchange_account_id,
                        symbol.clone(),
                        position_info.position,
                    )?;
            }
            self.exchange_id_with_restored_positions
                .insert(exchange_account_id);
        } else {
            let fill_positions = self
                .get_balances()
                .position_by_fill_amount
                .context("Failed to get fill_positions while restoring fill amount positions")?;
            let symbols = position_info_by_symbol.keys().cloned().collect_vec();

            let expected_positions_by_currency_pair: HashMap<CurrencyPair, Decimal> =
                position_info_by_symbol
                    .iter()
                    .map(|(k, v)| (k.currency_pair(), v.position))
                    .collect();

            let actual_positions_by_currency_pair: HashMap<CurrencyPair, Decimal> = symbols
                .iter()
                .map(|x| {
                    let position = fill_positions
                        .get(exchange_account_id, x.currency_pair())
                        .unwrap_or(dec!(0));
                    (x.currency_pair(), position)
                })
                .collect();

            let currency_pairs_with_diffs = symbols
                .iter()
                .filter(|symbol| {
                    let currency_pair = &symbol.currency_pair();
                    let expected_position = expected_positions_by_currency_pair.get(currency_pair);
                    let actual_position = actual_positions_by_currency_pair.get(currency_pair);
                    expected_position != actual_position
                })
                .map(|x| x.currency_pair())
                .collect_vec();

            if !currency_pairs_with_diffs.is_empty() {
                let diff_times_by_currency_pair = self
                    .position_differs_times_in_row_by_exchange_id
                    .entry(exchange_account_id)
                    .or_default();

                for currency_pair in currency_pairs_with_diffs {
                    *diff_times_by_currency_pair
                        .entry(currency_pair)
                        .or_default() += 1;
                }

                const MAX_TIMES_FOR_ERROR: u32 = 5;
                let any_at_max_times = diff_times_by_currency_pair
                    .values()
                    .any(|&x| x > MAX_TIMES_FOR_ERROR);

                let log_level = if any_at_max_times { Error } else { Warn };
                log!(log_level, "Position on {exchange_account_id} differs from local {expected_positions_by_currency_pair:?} {actual_positions_by_currency_pair:?}");

                if any_at_max_times {
                    bail!("Position on {exchange_account_id} differs from local");
                }
            } else {
                self.position_differs_times_in_row_by_exchange_id
                    .remove(&exchange_account_id);
            }
        }
        Ok(())
    }

    pub fn update_exchange_balance(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        balances_and_positions: &ExchangeBalancesAndPositions,
    ) -> Result<()> {
        let whole_balances_before = self.calculate_whole_balances()?;

        let currencies: HashSet<_> = self
            .balance_reservation_manager
            .exchanges_by_id()
            .get(&exchange_account_id)
            .with_context(|| format!("Failed to get exchange with id {exchange_account_id}"))?
            .currencies
            .lock()
            .iter()
            .cloned()
            .collect();

        let mut filtered_exchange_balances: HashMap<_, _> = balances_and_positions
            .balances
            .iter()
            .filter(|x| !x.balance.is_zero() || currencies.contains(&x.currency_code)) //We skip currencies with zero balances if they are not part of Exchange currency pairs
            .map(|x| (x.currency_code, x.balance))
            .collect();

        for currency in currencies {
            let _ = filtered_exchange_balances.entry(currency).or_default();
        }

        self.restore_fill_amount_position(exchange_account_id, &balances_and_positions.positions)?;

        let reservations_by_exchange_account_id = self
            .balance_reservation_manager
            .balance_reservation_storage
            .get_all_raw_reservations()
            .values()
            .filter(|&x| x.exchange_account_id == exchange_account_id)
            .collect_vec();

        for reservation in &reservations_by_exchange_account_id {
            let not_approved_amount_cost =
                reservation.get_proportional_cost_amount(reservation.not_approved_amount)?;
            if let Some(filtered_exchange_balance) =
                filtered_exchange_balances.get_mut(&reservation.reservation_currency_code)
            {
                *filtered_exchange_balance -=
                    reservation.convert_in_reservation_currency(not_approved_amount_cost);
            }
        }

        self.balance_reservation_manager
            .virtual_balance_holder
            .update_balances(exchange_account_id, &filtered_exchange_balances);

        let whole_balances_after = self.calculate_whole_balances()?;

        log::info!("Updated balances for {exchange_account_id} {filtered_exchange_balances:?} {reservations_by_exchange_account_id:?} {balances_and_positions:?}");

        self.save_balances();
        self.save_balance_update(whole_balances_before, whole_balances_after);
        Ok(())
    }

    fn calculate_whole_balances(
        &self,
    ) -> Result<HashMap<ExchangeAccountId, HashMap<CurrencyCode, Amount>>> {
        let mut balances_dict = self
            .balance_reservation_manager
            .virtual_balance_holder
            .get_raw_exchange_balances()
            .clone();
        let balance_reservations = self
            .balance_reservation_manager
            .balance_reservation_storage
            .get_all_raw_reservations()
            .values()
            .collect_vec();

        for reservation in balance_reservations {
            if reservation.not_approved_amount.is_zero() {
                continue;
            }

            let balances = match balances_dict.get_mut(&reservation.exchange_account_id) {
                Some(balances) => balances,
                None => continue,
            };

            let reservation_currency_code = reservation.reservation_currency_code;
            let mut balance = balances
                .get_mut(&reservation_currency_code)
                .with_context(|| {
                    format!("failed to get balance from balances for {reservation_currency_code}")
                })?;

            balance += reservation.convert_in_reservation_currency(
                reservation.get_proportional_cost_amount(reservation.not_approved_amount)?,
            );
        }
        Ok(balances_dict)
    }

    pub fn custom_clone(this: Arc<Mutex<Self>>) -> Arc<Mutex<BalanceManager>> {
        let this_locked = this.lock();
        let balances = this_locked.get_balances();
        let event_recorder = this_locked.event_recorder.clone();
        let exchanges_by_id = this_locked.balance_reservation_manager.exchanges_by_id();
        let new_balance_manager = Self::new(
            CurrencyPairToSymbolConverter::new(exchanges_by_id.clone()),
            event_recorder,
        );
        drop(this_locked);

        let mut new_bm_lock = new_balance_manager.lock();
        new_bm_lock.restore_balance_state(&balances, true);
        new_bm_lock.balance_reservation_manager.is_call_from_clone = true;
        drop(new_bm_lock);

        new_balance_manager
    }

    pub fn clone_and_subtract_not_approved_data(
        this: Arc<Mutex<Self>>,
        orders: Option<Vec<OrderRef>>,
    ) -> Result<Arc<Mutex<BalanceManager>>> {
        let balance_manager = Self::custom_clone(this);

        let mut bm_locked = balance_manager.lock();
        let not_full_approved_reservations: HashMap<_, _> = bm_locked
            .balance_reservation_manager
            .balance_reservation_storage
            .get_all_raw_reservations()
            .iter()
            .filter(|(_, reservation)| reservation.not_approved_amount > dec!(0))
            .map(|(id, reservation)| (*id, reservation.clone()))
            .collect();

        let orders_to_subtract = orders.unwrap_or_default();

        let mut applied_orders = HashSet::new();
        for order in orders_to_subtract {
            let (is_finished, order_type, client_order_id, reservation_id, status) =
                order.fn_ref(|x| {
                    (
                        x.props.is_finished(),
                        x.header.order_type,
                        x.header.client_order_id.clone(),
                        x.header.reservation_id,
                        x.props.status,
                    )
                });

            if is_finished || status == OrderStatus::Creating {
                continue;
            }

            if order_type == OrderType::Market {
                bail!("Clone doesn't support market orders because we need to know the price")
            }

            if applied_orders.insert(client_order_id.clone()) {
                let reservation_id = match reservation_id {
                    Some(reservation_id) => reservation_id,
                    None => continue,
                };

                bm_locked.unreserve_by_client_order_id(
                    reservation_id,
                    client_order_id,
                    order.amount(),
                )?
            }
        }

        for (reservation_id, reservation) in not_full_approved_reservations {
            let amount_to_unreserve = reservation.not_approved_amount;
            if reservation.is_amount_within_symbol_margin_error(amount_to_unreserve) {
                // just in case if there is a possible precision error
                continue;
            }
            bm_locked.unreserve(reservation_id, amount_to_unreserve)?;
        }

        drop(bm_locked);

        Ok(balance_manager)
    }

    pub fn get_last_order_fills(&self) -> &HashMap<MarketAccountId, OrderFill> {
        &self.last_order_fills
    }

    pub fn get_fill_amount_position_percent(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> Decimal {
        self.balance_reservation_manager
            .get_fill_amount_position_percent(
                configuration_descriptor,
                exchange_account_id,
                symbol,
                side,
            )
    }

    /// here we have OrderSnapshot in non actual state it's a cloned_order
    /// from OrderEventType::OrderFilled
    pub fn order_was_filled(
        &mut self,
        configuration_descriptor: ConfigurationDescriptor,
        order_snapshot: &OrderSnapshot,
    ) {
        let order_fill = order_snapshot.fills.fills.last().with_expect_args(|f| {
            f(&format_args!(
                "failed to get fills from order {:?}",
                order_snapshot
            ))
        });

        self.order_was_filled_with_fill(configuration_descriptor, order_snapshot, order_fill)
    }

    pub fn order_was_filled_with_fill(
        &mut self,
        configuration_descriptor: ConfigurationDescriptor,
        order_snapshot: &OrderSnapshot,
        order_fill: &OrderFill,
    ) {
        let exchange_account_id = order_snapshot.header.exchange_account_id;
        let symbol = self
            .balance_reservation_manager
            .currency_pair_to_symbol_converter
            .get_symbol(exchange_account_id, order_snapshot.header.currency_pair);
        self.handle_order_fill(
            configuration_descriptor,
            exchange_account_id,
            symbol,
            order_snapshot,
            order_fill,
        );
        self.save_balances();

        if let Some(balance_changes_service) = &self.balance_changes_service {
            balance_changes_service.add_balance_change(
                configuration_descriptor,
                order_snapshot,
                order_fill,
            );
        }
    }

    fn handle_order_fill(
        &mut self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        order_snapshot: &OrderSnapshot,
        order_fill: &OrderFill,
    ) {
        let (amount_in_before_trade_currency_code, currency_code_before_trade) = self
            .balance_reservation_manager
            .handle_position_fill_amount_change(
                order_snapshot.header.side,
                BeforeAfter::Before,
                order_fill.client_order_fill_id(),
                order_fill.amount(),
                order_fill.price(),
                configuration_descriptor,
                exchange_account_id,
                symbol.clone(),
            );

        let (amount_in_after_trade_currency_code, currency_code_after_trade) = self
            .balance_reservation_manager
            .handle_position_fill_amount_change(
                order_snapshot.header.side,
                BeforeAfter::After,
                order_fill.client_order_fill_id(),
                -order_fill.amount(),
                order_fill.price(),
                configuration_descriptor,
                exchange_account_id,
                symbol.clone(),
            );

        self.balance_reservation_manager
            .handle_position_fill_amount_change_commission(
                order_fill.commission_currency_code(),
                order_fill.commission_amount(),
                order_fill.converted_commission_currency_code(),
                order_fill.converted_commission_amount(),
                order_fill.price(),
                configuration_descriptor,
                exchange_account_id,
                symbol.clone(),
            );

        self.update_last_order_fill(
            exchange_account_id,
            symbol.currency_pair(),
            order_fill.clone(),
        );

        let position = self
            .balance_reservation_manager
            .get_position_in_amount_currency_code(
                exchange_account_id,
                symbol,
                order_snapshot.header.side,
            );

        log::info!(
            "Order was filled handle_order_fill {} {} {} {:?} {:?} {} {} {} {} {} {} {} {}",
            position,
            order_snapshot.header.exchange_account_id,
            order_snapshot.header.client_order_id,
            order_snapshot.props.exchange_order_id,
            order_fill.trade_id(),
            order_fill.price(),
            order_fill.amount(),
            order_fill.commission_currency_code(),
            order_fill.commission_amount(),
            currency_code_before_trade,
            amount_in_before_trade_currency_code,
            currency_code_after_trade,
            amount_in_after_trade_currency_code
        );
    }

    fn update_last_order_fill(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        order_fill: OrderFill,
    ) {
        self.last_order_fills.insert(
            MarketAccountId::new(exchange_account_id, currency_pair),
            order_fill,
        );
    }

    /// here we have OrderSnapshot in non actual state it's a cloned_order
    /// from OrderEventType::OrderCompleted
    pub fn order_was_finished(
        &mut self,
        configuration_descriptor: ConfigurationDescriptor,
        order_snapshot: &OrderSnapshot,
    ) {
        for order_fill in &order_snapshot.fills.fills {
            self.order_was_filled_with_fill(configuration_descriptor, order_snapshot, order_fill);
        }

        if order_snapshot.status() == OrderStatus::Canceled {
            if let Some(reservation_id) = order_snapshot.header.reservation_id {
                if self.get_reservation(reservation_id).is_some() {
                    self.balance_reservation_manager
                        .cancel_approved_reservation(
                            reservation_id,
                            &order_snapshot.header.client_order_id,
                        );
                    self.save_balances();
                }
            }
        }
    }

    pub fn get_reservation(&self, reservation_id: ReservationId) -> Option<&BalanceReservation> {
        self.balance_reservation_manager
            .get_reservation(reservation_id)
    }

    pub fn get_reservation_expected(&self, reservation_id: ReservationId) -> &BalanceReservation {
        self.balance_reservation_manager
            .get_reservation_expected(reservation_id)
    }

    pub fn get_mut_reservation(
        &mut self,
        reservation_id: ReservationId,
    ) -> Option<&mut BalanceReservation> {
        self.balance_reservation_manager
            .get_mut_reservation(reservation_id)
    }

    pub fn get_mut_reservation_expected(
        &mut self,
        reservation_id: ReservationId,
    ) -> &mut BalanceReservation {
        self.balance_reservation_manager
            .get_mut_reservation_expected(reservation_id)
    }

    pub fn unreserve_pair(
        &mut self,
        reservation_id_1: ReservationId,
        amount_1: Amount,
        reservation_id_2: ReservationId,
        amount_2: Amount,
    ) {
        self.balance_reservation_manager
            .unreserve_expected(reservation_id_1, amount_1, &None);
        self.balance_reservation_manager
            .unreserve_expected(reservation_id_2, amount_2, &None);
        self.save_balances();
    }

    pub fn approve_reservation(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &ClientOrderId,
        amount: Amount,
    ) {
        self.balance_reservation_manager
            .approve_reservation(reservation_id, client_order_id, amount)
            .with_expect_args(|f| {
                f(&format_args!(
                    "failed to approve reservation {} {} {} ",
                    reservation_id, client_order_id, amount,
                ))
            });

        self.save_balances();
    }

    pub fn try_transfer_reservation(
        &mut self,
        src_reservation_id: ReservationId,
        dst_reservation_id: ReservationId,
        amount: Amount,
        client_order_id: &Option<ClientOrderId>,
    ) -> bool {
        if !self.balance_reservation_manager.try_transfer_reservation(
            src_reservation_id,
            dst_reservation_id,
            amount,
            client_order_id,
        ) {
            return false;
        }
        self.save_balances();
        true
    }

    pub fn try_update_reservation(
        &mut self,
        reservation_id: ReservationId,
        new_price: Price,
    ) -> bool {
        if !self
            .balance_reservation_manager
            .try_update_reservation_price(reservation_id, new_price)
        {
            return false;
        }

        self.save_balances();
        true
    }

    pub fn try_reserve(
        &mut self,
        reserve_parameters: &ReserveParameters,
        explanation: &mut Option<Explanation>,
    ) -> Option<ReservationId> {
        if let Some(reservation_id) = self
            .balance_reservation_manager
            .try_reserve(reserve_parameters, explanation)
        {
            self.save_balances();
            return Some(reservation_id);
        }
        None
    }

    pub fn try_reserve_pair(
        &mut self,
        order1: ReserveParameters,
        order2: ReserveParameters,
    ) -> Option<(ReservationId, ReservationId)> {
        let reservations_id = self
            .balance_reservation_manager
            .try_reserve_multiple(&[order1, order2], &mut None)?;
        if reservations_id.len() == 2 {
            self.save_balances();
            return Some((reservations_id[0], reservations_id[1]));
        }
        None
    }

    pub fn try_reserve_three(
        &mut self,
        order1: ReserveParameters,
        order2: ReserveParameters,
        order3: ReserveParameters,
    ) -> Option<(ReservationId, ReservationId, ReservationId)> {
        let reservations_id = self
            .balance_reservation_manager
            .try_reserve_multiple(&[order1, order2, order3], &mut None)?;
        if reservations_id.len() == 3 {
            self.save_balances();
            return Some((reservations_id[0], reservations_id[1], reservations_id[2]));
        }
        None
    }

    pub fn can_reserve(
        &self,
        reserve_parameters: &ReserveParameters,
        explanation: &mut Option<Explanation>,
    ) -> bool {
        self.balance_reservation_manager
            .can_reserve(reserve_parameters, explanation)
    }

    pub fn get_exchange_balance(
        &self,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        currency_code: CurrencyCode,
    ) -> Option<Amount> {
        self.balance_reservation_manager
            .virtual_balance_holder
            .get_exchange_balance(exchange_account_id, symbol, currency_code, None)
    }

    pub fn get_all_virtual_balance_diffs(&self) -> &ServiceValueTree {
        self.balance_reservation_manager
            .virtual_balance_holder
            .get_virtual_balance_diffs()
    }

    pub fn get_leveraged_balance_in_amount_currency_code(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        side: OrderSide,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        price_quote_to_base: Price,
        explanation: &mut Option<Explanation>,
    ) -> Option<Decimal> {
        match self
            .balance_reservation_manager
            .get_available_leveraged_balance(
                configuration_descriptor,
                exchange_account_id,
                symbol.clone(),
                side,
                price_quote_to_base,
                explanation,
            ) {
            Some(balance) => {
                let currency_code = symbol.get_trade_code(side, BeforeAfter::Before);
                let balance_in_amount_currency_code = symbol
                    .convert_amount_into_amount_currency_code(
                        currency_code,
                        balance,
                        price_quote_to_base,
                    );
                Some(symbol.round_to_remove_amount_precision_error_expected(
                    balance_in_amount_currency_code,
                ))
            }
            None => {
                log::warn!(
                    "There's no balance for {}:{} {}",
                    exchange_account_id,
                    symbol.currency_pair(),
                    side
                );
                None
            }
        }
    }

    pub fn get_balance_by_currency_code(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        currency_code: CurrencyCode,
        price: Price,
    ) -> Option<Amount> {
        self.balance_reservation_manager
            .try_get_available_balance_with_unknown_side(
                configuration_descriptor,
                exchange_account_id,
                symbol,
                currency_code,
                price,
            )
    }

    pub fn get_balance_by_side(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
        price: Price,
    ) -> Option<Amount> {
        self.balance_reservation_manager.try_get_available_balance(
            configuration_descriptor,
            exchange_account_id,
            symbol,
            side,
            price,
            true,
            false,
            &mut None,
        )
    }

    pub fn get_balance_by_reserve_parameters(
        &self,
        reserve_parameters: &ReserveParameters,
    ) -> Option<Amount> {
        self.get_balance_by_side(
            reserve_parameters.configuration_descriptor,
            reserve_parameters.exchange_account_id,
            reserve_parameters.symbol.clone(),
            reserve_parameters.order_side,
            reserve_parameters.price,
        )
    }

    pub fn get_balance_reservation_currency_code(
        &self,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> CurrencyCode {
        self.balance_reservation_manager
            .exchanges_by_id()
            .get(&exchange_account_id)
            .expect("failed to get exchange")
            .get_balance_reservation_currency_code(symbol, side)
    }

    pub fn balance_was_received(&self, exchange_account_id: ExchangeAccountId) -> bool {
        self.balance_reservation_manager
            .virtual_balance_holder
            .has_real_balance_on_exchange(exchange_account_id)
    }

    pub fn set_target_amount_limit(
        &mut self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        limit: Amount,
    ) {
        self.balance_reservation_manager.set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            symbol,
            limit,
        );
    }

    pub fn set_balance_changes_service(&mut self, service: Arc<BalanceChangesService>) {
        self.balance_changes_service = Some(service);
    }

    pub async fn update_balances_for_exchanges(
        this: Arc<Mutex<Self>>,
        cancellation_token: CancellationToken,
    ) {
        log::trace!("Balance update started");

        let exchanges = this
            .lock()
            .balance_reservation_manager
            .exchanges_by_id()
            .values()
            .cloned()
            .collect_vec();

        let update_actions = exchanges.iter().map(|exchange| {
            let this = this.clone();
            let cancellation_token = cancellation_token.clone();

            async move {
                let run = async move {
                    let balances_and_positions = exchange
                        .get_balance(cancellation_token.clone())
                        .await
                        .with_context(|| {
                            format!("failed get_balance for {}", exchange.exchange_account_id)
                        })?;

                    this.lock()
                        .update_exchange_balance(
                            exchange.exchange_account_id,
                            &balances_and_positions,
                        )
                        .context("failed to update exchange balance")
                };

                match run.await {
                    Ok(()) => nothing_to_do(),
                    Err(err) => log::error!("{err:?}"),
                }
            }
        });

        join_all(update_actions).await;

        log::trace!("Balance update finished")
    }

    // TODO: should be implemented
    // public void ExecuteTransaction(Action action)
    // {
    //     lock (_syncObject)
    //     {
    //         action();
    //     }
    // }
}

#[cfg_attr(test, automock)]
impl BalanceManager {
    pub fn get_last_position_change_before_period(
        &self,
        market_account_id: &MarketAccountId,
        start_of_period: DateTime,
    ) -> Option<PositionChange> {
        self.balance_reservation_manager
            .get_last_position_change_before_period(market_account_id, start_of_period)
    }

    pub fn get_position(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        side: OrderSide,
    ) -> Decimal {
        self.balance_reservation_manager
            .get_position(exchange_account_id, currency_pair, side)
    }
}

impl_mock_initializer!(MockBalanceManager);
