use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::core::balance_manager::balance_reservation::BalanceReservation;
use crate::core::balance_manager::position_change::PositionChange;
use crate::core::balances::balance_reservation_manager::BalanceReservationManager;
use crate::core::exchanges::common::{Amount, Price};
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, TradePlaceAccount};
use crate::core::exchanges::events::ExchangeBalancesAndPositions;
use crate::core::exchanges::general::currency_pair_metadata::{BeforeAfter, CurrencyPairMetadata};
use crate::core::exchanges::general::currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::explanation::Explanation;
use crate::core::misc::derivative_position::DerivativePosition;
use crate::core::misc::reserve_parameters::ReserveParameters;
use crate::core::misc::service_value_tree::ServiceValueTree;
use crate::core::orders::fill::OrderFill;
use crate::core::orders::order::{
    ClientOrderId, OrderSide, OrderSnapshot, OrderStatus, OrderType, ReservationId,
};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::core::DateTime;
use crate::core::{balance_manager::balances::Balances, exchanges::common::ExchangeAccountId};

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[cfg(test)]
use mockall::automock;

/// The entity for getting information about account balances for selected exchanges
#[derive(Clone)]
pub struct BalanceManager {
    exchange_id_with_restored_positions: HashSet<ExchangeAccountId>,
    balance_reservation_manager: BalanceReservationManager,
    last_order_fills: HashMap<TradePlaceAccount, OrderFill>,
}

impl BalanceManager {
    pub fn new(
        exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
        currency_pair_to_metadata_converter: CurrencyPairToMetadataConverter,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            exchange_id_with_restored_positions: HashSet::new(),
            balance_reservation_manager: BalanceReservationManager::new(
                exchanges_by_id,
                currency_pair_to_metadata_converter,
            ),
            last_order_fills: HashMap::new(),
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
                        .update_balances(exchange_account_id, balance);
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
            .try_get_reservation(&reservation_id)
            .with_context(|| format!("Can't find reservation_id: {}", reservation_id))?
            .unreserved_amount;
        return self.unreserve(reservation_id, amount);
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
        // TODO: fix me when DataRecorder will be added
        // if self.data_recorder.is_none() {
        //     return ()
        // }

        let _balances = self.get_balances();
        // self.data_recorder.save(balances);
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
        exchange_account_id: &ExchangeAccountId,
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

        let mut position_info_by_currency_pair_metadata = HashMap::new();

        for position_info in positions {
            let currency_pair = position_info.currency_pair.clone();
            let currency_pair_metadata = self
                .balance_reservation_manager
                .exchanges_by_id
                .get(exchange_account_id)
                .with_context(|| format!(
                        "currency_pair_metadata not found for exchange with account id {:?} and currency pair {}",
                        exchange_account_id,
                        currency_pair,
                    )
                )?
                .get_currency_pair_metadata(&currency_pair)?;

            if currency_pair_metadata.is_derivative {
                position_info_by_currency_pair_metadata
                    .insert(currency_pair_metadata.clone(), position_info);
            }
        }

        if !self
            .exchange_id_with_restored_positions
            .contains(exchange_account_id)
        {
            for (currency_pair_metadata, position_info) in position_info_by_currency_pair_metadata {
                self.balance_reservation_manager
                    .restore_fill_amount_position(
                        exchange_account_id,
                        currency_pair_metadata.clone(),
                        position_info.position,
                    )?;
            }
            self.exchange_id_with_restored_positions
                .insert(exchange_account_id.clone());
        } else {
            let fill_positions =
                self.get_balances()
                    .position_by_fill_amount
                    .with_context(|| {
                        "Failed to get fill_positions while restoring fill amount positions"
                    })?;
            let currency_pair_metadatas = position_info_by_currency_pair_metadata
                .keys()
                .cloned()
                .collect_vec();

            let expected_positions_by_currency_pair: HashMap<CurrencyPair, Decimal> =
                position_info_by_currency_pair_metadata
                    .iter()
                    .map(|(k, v)| (k.currency_pair(), v.position))
                    .collect();

            let actual_positions_by_currency_pair: HashMap<CurrencyPair, Decimal> =
                currency_pair_metadatas
                    .iter()
                    .map(|x| {
                        (
                            x.currency_pair(),
                            fill_positions
                                .get(exchange_account_id, &x.currency_pair())
                                .unwrap_or(dec!(0)),
                        )
                    })
                    .collect();

            let currency_pairs_with_diffs = currency_pair_metadatas
                .iter()
                .filter(|metadata| {
                    let currency_pair = &metadata.currency_pair();
                    let expected_position = expected_positions_by_currency_pair.get(currency_pair);
                    let actual_position = actual_positions_by_currency_pair.get(currency_pair);
                    expected_position != actual_position
                })
                .map(|x| x.currency_pair())
                .collect_vec();

            let mut position_differs_times_in_row_by_exchange_id: HashMap<
                ExchangeAccountId,
                HashMap<CurrencyPair, i32>,
            > = HashMap::new();
            if !currency_pairs_with_diffs.is_empty() {
                let diff_times_by_currency_pair: &mut HashMap<_, _> =
                    position_differs_times_in_row_by_exchange_id
                        .entry(exchange_account_id.clone())
                        .or_default();

                for currency_pair in currency_pairs_with_diffs {
                    *diff_times_by_currency_pair
                        .entry(currency_pair)
                        .or_default() += 1;
                }

                const MAX_TIMES_FOR_ERROR: i32 = 5;
                let any_at_max_times = diff_times_by_currency_pair
                    .values()
                    .any(|&x| x > MAX_TIMES_FOR_ERROR);

                let log_level = if any_at_max_times {
                    log::Level::Error
                } else {
                    log::Level::Warn
                };
                log::log!(
                    log_level,
                    "Position on {} differs from local {:?} {:?}",
                    exchange_account_id,
                    expected_positions_by_currency_pair,
                    actual_positions_by_currency_pair
                );

                if any_at_max_times {
                    bail!("Position on {} differs from local", exchange_account_id);
                }
            } else {
                position_differs_times_in_row_by_exchange_id.remove(exchange_account_id);
            }
        }
        Ok(())
    }

    pub fn update_exchange_balance(
        &mut self,
        exchange_account_id: &ExchangeAccountId,
        balances_and_positions: &ExchangeBalancesAndPositions,
    ) -> Result<()> {
        let mut filtred_exchange_balances = HashMap::new();
        let mut reservations_by_exchange_account_id = Vec::new();

        let whole_balances_before = self.calculate_whole_balances()?;

        {
            let exchange_currencies = self
                .balance_reservation_manager
                .exchanges_by_id
                .get(exchange_account_id)
                .with_context(|| format!("Failed to get exchange with id {}", exchange_account_id))?
                .currencies
                .lock()
                .clone();

            for exchange_balance in &balances_and_positions.balances {
                //We skip currencies with zero balances if they are not part of Exchange currency pairs
                if exchange_balance.balance.is_zero()
                    && !exchange_currencies.contains(&exchange_balance.currency_code)
                {
                    continue;
                }

                filtred_exchange_balances.insert(
                    exchange_balance.currency_code.clone(),
                    exchange_balance.balance.clone(),
                );
            }
        }
        self.restore_fill_amount_position(exchange_account_id, &balances_and_positions.positions)?;

        {
            let exchange_currencies = self
                .balance_reservation_manager
                .exchanges_by_id
                .get(exchange_account_id)
                .with_context(|| format!("Failed to get exchange with id {}", exchange_account_id))?
                .currencies
                .lock();

            for exchange_currency in exchange_currencies.iter() {
                filtred_exchange_balances
                    .entry(exchange_currency.clone())
                    .or_insert_with(|| dec!(0));
            }
        }

        for x in self
            .balance_reservation_manager
            .balance_reservation_storage
            .get_all_raw_reservations()
            .values()
        {
            if &x.exchange_account_id == exchange_account_id {
                reservations_by_exchange_account_id.push(x);
            }
        }

        for reservation in &reservations_by_exchange_account_id {
            let not_approved_amount_cost =
                reservation.get_proportional_cost_amount(reservation.not_approved_amount)?;
            if let Some(filtred_exchange_balance) =
                filtred_exchange_balances.get_mut(&reservation.reservation_currency_code)
            {
                *filtred_exchange_balance -=
                    reservation.convert_in_reservation_currency(not_approved_amount_cost);
            }
        }

        self.balance_reservation_manager
            .virtual_balance_holder
            .update_balances(exchange_account_id, &filtred_exchange_balances);

        let whole_balances_after = self.calculate_whole_balances()?;

        log::info!(
            "Updated balances for {} {:?} {:?} {:?}",
            exchange_account_id,
            balances_and_positions,
            reservations_by_exchange_account_id,
            filtred_exchange_balances
        );

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

            if !balances_dict.contains_key(&reservation.exchange_account_id) {
                continue;
            }

            let balances = balances_dict
                .get_mut(&reservation.exchange_account_id)
                .with_context(|| {
                    format!(
                        "failed to get balances from for {}",
                        reservation.exchange_account_id,
                    )
                })?;

            let mut balance = balances
                .get_mut(&reservation.reservation_currency_code)
                .with_context(|| {
                    format!(
                        "failed to get balance from balances for {}",
                        reservation.reservation_currency_code,
                    )
                })?;

            balance += reservation.get_proportional_cost_amount(reservation.not_approved_amount)?;
        }
        Ok(balances_dict)
    }

    pub fn custom_clone(this: Arc<Mutex<Self>>) -> Arc<Mutex<BalanceManager>> {
        let this_locked = this.lock();
        let balances = this_locked.get_balances();
        let exchanges_by_id = &this_locked.balance_reservation_manager.exchanges_by_id;
        let new_balance_manager = Self::new(
            exchanges_by_id.clone(),
            CurrencyPairToMetadataConverter::new(exchanges_by_id.clone()),
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
        orders: Option<Vec<OrderSnapshot>>,
    ) -> Result<Arc<Mutex<BalanceManager>>> {
        let balance_manager = Self::custom_clone(this.clone());

        let mut bm_locked = balance_manager.lock();
        let not_full_approved_reservations: HashMap<_, _> = bm_locked
            .balance_reservation_manager
            .balance_reservation_storage
            .get_all_raw_reservations()
            .iter()
            .filter(|(_, reservation)| reservation.not_approved_amount > dec!(0))
            .map(|(id, reservation)| (id.clone(), reservation.clone()))
            .collect();

        let orders_to_subtract = orders.unwrap_or_default();

        let mut applied_orders = HashSet::new();
        for order in orders_to_subtract {
            if order.props.is_finished() || order.status() == OrderStatus::Creating {
                continue;
            }

            if order.header.order_type == OrderType::Market {
                bail!("Clone doesn't support market orders because we need to know the price")
            }

            if applied_orders.insert(order.header.client_order_id.clone()) {
                let reservation_id = match order.header.reservation_id {
                    Some(reservation_id) => reservation_id,
                    None => continue,
                };

                bm_locked.unreserve_by_client_order_id(
                    reservation_id,
                    order.header.client_order_id.clone(),
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
            bm_locked.unreserve(reservation_id.clone(), amount_to_unreserve)?;
        }

        drop(bm_locked);

        Ok(balance_manager)
    }

    pub fn get_last_order_fills(&self) -> &HashMap<TradePlaceAccount, OrderFill> {
        &self.last_order_fills
    }

    pub fn get_fill_amount_position_percent(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> Decimal {
        self.balance_reservation_manager
            .get_fill_amount_position_percent(
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata.clone(),
                side,
            )
    }

    /// here we have OrderSnapshot in non actual state it's a cloned_order
    /// from OrderEventType::OrderFilled
    pub fn order_was_filled(
        &mut self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        order_snapshot: &OrderSnapshot,
    ) {
        let order_fill = order_snapshot
            .fills
            .fills
            .last()
            .expect(format!("failed to get fills from order {:?}", order_snapshot).as_str());

        self.order_was_filled_with_fill(configuration_descriptor, order_snapshot, order_fill)
    }

    pub fn order_was_filled_with_fill(
        &mut self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        order_snapshot: &OrderSnapshot,
        order_fill: &OrderFill,
    ) {
        let exchange_account_id = &order_snapshot.header.exchange_account_id;
        let currency_pair_metadata = self
            .balance_reservation_manager
            .currency_pair_to_metadata_converter
            .get_currency_pair_metadata(exchange_account_id, &order_snapshot.header.currency_pair);
        self.handle_order_fill(
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata,
            order_snapshot,
            order_fill,
        );
        self.save_balances();
        // _balanceChangesService?.AddBalanceChange(configurationDescriptor, order, orderFill); // TODO: fix me when added
    }

    fn handle_order_fill(
        &mut self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
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
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata.clone(),
            );

        let (amount_in_after_trade_currency_code, currency_code_after_trade) = self
            .balance_reservation_manager
            .handle_position_fill_amount_change(
                order_snapshot.header.side,
                BeforeAfter::After,
                order_fill.client_order_fill_id(),
                -order_fill.amount(),
                order_fill.price(),
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata.clone(),
            );

        self.balance_reservation_manager
            .handle_position_fill_amount_change_commission(
                order_fill.commission_currency_code().clone(),
                order_fill.commission_amount(),
                order_fill.converted_commission_currency_code().clone(),
                order_fill.converted_commission_amount(),
                order_fill.price(),
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata.clone(),
            );

        self.update_last_order_fill(
            exchange_account_id.clone(),
            currency_pair_metadata.currency_pair(),
            order_fill.clone(),
        );

        let position = self
            .balance_reservation_manager
            .get_position_in_amount_currency_code(
                exchange_account_id,
                currency_pair_metadata.clone(),
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
            TradePlaceAccount::new(exchange_account_id, currency_pair),
            order_fill,
        );
    }

    /// here we have OrderSnapshot in non actual state it's a cloned_order
    /// from OrderEventType::OrderCompleted
    pub fn order_was_finished(
        &mut self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        order_snapshot: &OrderSnapshot,
    ) {
        for order_fill in &order_snapshot.fills.fills {
            self.order_was_filled_with_fill(
                configuration_descriptor.clone(),
                order_snapshot,
                order_fill,
            );
        }

        if order_snapshot.status() == OrderStatus::Canceled {
            if let Some(reservation_id) = order_snapshot.header.reservation_id {
                if self.try_get_reservation(&reservation_id).is_some() {
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
    pub fn try_get_reservation(
        &self,
        reservation_id: &ReservationId,
    ) -> Option<&BalanceReservation> {
        self.balance_reservation_manager
            .balance_reservation_storage
            .try_get(reservation_id)
    }

    pub fn get_reservation(&self, reservation_id: &ReservationId) -> &BalanceReservation {
        self.try_get_reservation(reservation_id)
            .expect("failed to get reservation for reservation_id: {}")
    }

    pub fn get_mut_reservation(
        &mut self,
        reservation_id: ReservationId,
    ) -> Option<&mut BalanceReservation> {
        self.balance_reservation_manager
            .get_mut_reservation(&reservation_id)
    }

    pub fn unreserve_pair(
        &mut self,
        reservation_id_1: ReservationId,
        amount_1: Amount,
        reservation_id_2: ReservationId,
        amount_2: Amount,
    ) {
        self.balance_reservation_manager
            .unreserve(reservation_id_1, amount_1, &None)
            .expect(format!("failed to unreserve {} {}", reservation_id_1, amount_1).as_str());
        self.balance_reservation_manager
            .unreserve(reservation_id_2, amount_2, &None)
            .expect(format!("failed to unreserve {} {}", reservation_id_2, amount_2).as_str());
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
            .expect(
                format!(
                    "failed to approve reservation {} {} {} ",
                    reservation_id, client_order_id, amount,
                )
                .as_str(),
            );

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
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        currency_code: &CurrencyCode,
    ) -> Option<Amount> {
        self.balance_reservation_manager
            .virtual_balance_holder
            .get_exchange_balance(
                exchange_account_id,
                currency_pair_metadata.clone(),
                currency_code,
                None,
            )
    }

    pub fn get_all_virtual_balance_diffs(&self) -> &ServiceValueTree {
        self.balance_reservation_manager
            .virtual_balance_holder
            .get_virtual_balance_diffs()
    }

    pub fn get_leveraged_balance_in_amount_currency_code(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        side: OrderSide,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        price_quote_to_base: Price,
        explanation: &mut Option<Explanation>,
    ) -> Option<Decimal> {
        match self
            .balance_reservation_manager
            .get_available_leveraged_balance(
                configuration_descriptor,
                exchange_account_id,
                currency_pair_metadata.clone(),
                side,
                price_quote_to_base,
                explanation,
            ) {
            Some(balance) => {
                let currency_code =
                    currency_pair_metadata.get_trade_code(side, BeforeAfter::Before);
                let balance_in_amount_currency_code = currency_pair_metadata
                    .convert_amount_into_amount_currency_code(
                        &currency_code,
                        balance,
                        price_quote_to_base,
                    );
                return Some(
                    currency_pair_metadata
                        .round_to_remove_amount_precision_error(balance_in_amount_currency_code)
                        .expect(
                            format!(
                                "failed to round to remove amount precision error from {:?} for {}",
                                currency_pair_metadata, balance_in_amount_currency_code
                            )
                            .as_str(),
                        ),
                );
            }
            None => {
                log::warn!(
                    "There's no balance for {}:{} {}",
                    exchange_account_id,
                    currency_pair_metadata.currency_pair(),
                    side
                );
                return None;
            }
        }
    }

    pub fn get_balance_by_currency_code(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        currency_code: &CurrencyCode,
        price: Price,
    ) -> Option<Amount> {
        self.balance_reservation_manager
            .try_get_available_balance_with_unknown_side(
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata,
                currency_code,
                price,
            )
    }

    pub fn get_balance_by_side(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
        price: Price,
    ) -> Option<Amount> {
        self.balance_reservation_manager.try_get_available_balance(
            configuration_descriptor.clone(),
            exchange_account_id,
            currency_pair_metadata,
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
            reserve_parameters.configuration_descriptor.clone(),
            &reserve_parameters.exchange_account_id,
            reserve_parameters.currency_pair_metadata.clone(),
            reserve_parameters.order_side,
            reserve_parameters.price,
        )
    }

    pub fn get_balance_reservation_currency_code(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> CurrencyCode {
        self.balance_reservation_manager
            .exchanges_by_id
            .get(exchange_account_id)
            .expect("failed to get exchange")
            .get_balance_reservation_currency_code(currency_pair_metadata, side)
    }

    pub fn balance_was_received(&self, exchange_account_id: &ExchangeAccountId) -> bool {
        self.balance_reservation_manager
            .virtual_balance_holder
            .has_real_balance_on_exchange(exchange_account_id)
    }
    pub fn set_target_amount_limit(
        &mut self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        limit: Amount,
    ) {
        self.balance_reservation_manager.set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata,
            limit,
        );
    }
    // TODO: uncomment me when BalanceChangeServic will be implemented
    // pub fn set_balance_changes_service(&mut self, service: BalanceChangesService) {
    //     self.balance_changes_service = Some(service);
    // }

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
        trade_place: &TradePlaceAccount,
        start_of_period: DateTime,
    ) -> Option<PositionChange> {
        self.balance_reservation_manager
            .get_last_position_change_before_period(trade_place, start_of_period)
    }

    pub fn get_position(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        side: OrderSide,
    ) -> Decimal {
        self.balance_reservation_manager
            .get_position(exchange_account_id, currency_pair, side)
    }
}
