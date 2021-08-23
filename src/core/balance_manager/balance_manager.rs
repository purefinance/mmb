use std::collections::{HashMap, HashSet};

use crate::core::balances::balance_reservation_manager::BalanceReservationManager;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, TradePlaceAccount};
use crate::core::exchanges::events::ExchangeBalancesAndPositions;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::misc::derivative_position_info::DerivativePositionInfo;
use crate::core::orders::fill::OrderFill;
use crate::core::{balance_manager::balances::Balances, exchanges::common::ExchangeAccountId};

use anyhow::{bail, Result};
use itertools::Itertools;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

struct BalanceManager {
    // private readonly IDateTimeService _dateTimeService;
    // private readonly ILogger _logger = Log.ForContext<BalanceManager>();
    // private readonly object _syncObject = new object();
    exchanges_by_id: HashMap<ExchangeAccountId, Exchange>,

    // private readonly ICurrencyPairToSymbolConverter _currencyPairToSymbolConverter;
    exchange_id_with_restored_positions: HashSet<ExchangeAccountId>,
    balance_reservation_manager: BalanceReservationManager,
    position_differs_times_in_row_by_exchange_id:
        HashMap<ExchangeAccountId, HashMap<CurrencyPair, i32>>,

    // private readonly IDataRecorder? _dataRecorder;
    // private volatile IBalanceChangesService? _balanceChangesService;
    last_order_fills: HashMap<TradePlaceAccount, OrderFill>,
}

impl BalanceManager {
    pub fn restore_balance_state(&mut self, balances: &Balances, restore_exchange_balances: bool) {
        if restore_exchange_balances {
            match &balances.balances_by_exchange_id {
                Some(balances_by_exchange_id) => {
                    for (exchange_account_id, balance) in balances_by_exchange_id {
                        self.balance_reservation_manager
                            .virtual_balance_holder
                            .update_balances(exchange_account_id, balance);
                    }
                }
                None => {
                    log::error!(""); // TODO: grays fix me
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

        if let (Some(amount_limits), Some(position_by_fill_amount)) = (
            balances.amount_limits.clone(),
            balances.position_by_fill_amount.clone(),
        ) {
            self.balance_reservation_manager
                .restore_fill_amount_limits(amount_limits, position_by_fill_amount);
        }

        self.last_order_fills = balances.last_order_fills.clone();
    }

    pub fn get_reservation_ids(&self) -> Vec<i64> {
        self.balance_reservation_manager
            .balance_reservation_storage
            .get_reservation_ids()
    }

    pub(crate) fn restore_balance_state_with_reservations_handling(
        &mut self,
        balances: &Balances,
    ) -> Result<()> {
        self.restore_balance_state(balances, false);

        let active_reservations = self.get_reservation_ids();
        for reservation_id in active_reservations {
            self.unreserve_rest(reservation_id.clone())?;
        }
        Ok(())
    }

    pub fn unreserve_rest(&mut self, reservation_id: i64) -> Result<()> {
        if let Some(reservation) = self
            .balance_reservation_manager
            .get_reservation(&reservation_id)
        {
            let amount = reservation.unreserved_amount;
            return self.unreserve(reservation_id, amount);
        } else {
            bail!("Can't find reservation_id: {}", reservation_id)
        }
    }

    pub fn unreserve(&mut self, reservation_id: i64, amount: Decimal) -> Result<()> {
        self.balance_reservation_manager
            .unreserve(reservation_id, amount, &None)?;
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
        _whole_balance_before: HashMap<ExchangeAccountId, HashMap<CurrencyCode, Decimal>>,
        _whole_balance_after: HashMap<ExchangeAccountId, HashMap<CurrencyCode, Decimal>>,
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
        positions: Option<&Vec<DerivativePositionInfo>>,
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
            let currency_pair_metadata = match self.exchanges_by_id.get(exchange_account_id) {
                Some(exchange) => exchange.get_currency_pair_metadata(&currency_pair)?,
                None => {
                    bail!(
                        "currency_pair_metadata not found for exchange with account id {:?} and currency pair {}",
                        exchange_account_id,
                        currency_pair,
                    )
                }
            };
            if !currency_pair_metadata.is_derivative {
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
                        &currency_pair_metadata,
                        position_info.position,
                    )?;
            }
            self.exchange_id_with_restored_positions
                .insert(exchange_account_id.clone());
        } else {
            let fill_positions = match self.get_balances().position_by_fill_amount {
                Some(fill_positions) => fill_positions,
                None => bail!("Failed to get fill_positions while restoring fill amount positions"),
            };
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
                    .map(move |x| {
                        (
                            x.currency_pair(),
                            fill_positions
                                .get(exchange_account_id, &x.currency_pair())
                                .unwrap_or(dec!(0)),
                        )
                    })
                    .collect();

            let mut has_difference = false;
            let mut currency_pairs_with_diffs = Vec::new();
            for currency_pair in currency_pair_metadatas
                .iter()
                .map(|x| x.currency_pair())
                .collect_vec()
            {
                let expected_position = expected_positions_by_currency_pair.get(&currency_pair);
                let actual_position = actual_positions_by_currency_pair.get(&currency_pair);
                if expected_position != actual_position {
                    has_difference = true;
                    currency_pairs_with_diffs.push(currency_pair);
                }
            }

            if has_difference {
                if self
                    .position_differs_times_in_row_by_exchange_id
                    .get_mut(exchange_account_id)
                    .is_none()
                {
                    self.position_differs_times_in_row_by_exchange_id.insert(
                        exchange_account_id.clone(),
                        HashMap::<CurrencyPair, i32>::new(),
                    );
                }

                let diff_times_by_currency_pair = if let Some(res) = self
                    .position_differs_times_in_row_by_exchange_id
                    .get_mut(exchange_account_id)
                {
                    res
                } else {
                    bail!(
                        "diff_times_by_currency_pair not found in {:?} for id: {}",
                        self.position_differs_times_in_row_by_exchange_id,
                        exchange_account_id
                    )
                };
                for currency_pair in currency_pairs_with_diffs {
                    let new_diff_times = if let Some(new_diff_times) =
                        diff_times_by_currency_pair.get(&currency_pair)
                    {
                        new_diff_times + 1
                    } else {
                        1
                    };
                    diff_times_by_currency_pair.insert(currency_pair, new_diff_times);
                }

                let max_times_for_error = 5;
                let any_at_max_times = diff_times_by_currency_pair
                    .values()
                    .max()
                    .cloned()
                    .unwrap_or(0)
                    > max_times_for_error;

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
                self.position_differs_times_in_row_by_exchange_id
                    .remove(exchange_account_id);
            }
        }
        Ok(())
    }

    pub fn update_exchange_balance(
        &mut self,
        exchange_account_id: &ExchangeAccountId,
        balances_and_positions: ExchangeBalancesAndPositions,
    ) -> Result<()> {
        let mut filtred_exchange_balances = HashMap::new();
        let mut reservations_by_exchange_account_id = Vec::new();

        let whole_balances_before = self.calculate_whole_balances()?;

        {
            let exchange_currencies =
                if let Some(exchange) = self.exchanges_by_id.get(exchange_account_id) {
                    let tmp_mut = &exchange.currencies;
                    tmp_mut.lock()
                } else {
                    bail!("Failed to get exchange with id {}", exchange_account_id)
                };

            for exchange_balance in &balances_and_positions.balances {
                //We skip currencies with zero balances if they are not part of Exchange currency pairs
                if exchange_balance.balance == dec!(0)
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

        self.restore_fill_amount_position(
            exchange_account_id,
            Some(&balances_and_positions.positions),
        )?;

        {
            let exchange_currencies =
                if let Some(exchange) = self.exchanges_by_id.get(exchange_account_id) {
                    let tmp_mut = &exchange.currencies;
                    tmp_mut.lock()
                } else {
                    bail!("Failed to get exchange with id {}", exchange_account_id)
                };

            for exchange_currency in exchange_currencies.iter() {
                if let Some(balance) = filtred_exchange_balances.get_mut(exchange_currency) {
                    *balance = dec!(0);
                }
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
                    reservation.convert_in_reservation_currency(not_approved_amount_cost)?;
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
    ) -> Result<HashMap<ExchangeAccountId, HashMap<CurrencyCode, Decimal>>> {
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
            if reservation.not_approved_amount == dec!(0) {
                continue;
            }

            if !balances_dict.contains_key(&reservation.exchange_account_id) {
                continue;
            }

            let balances = match balances_dict.get_mut(&reservation.exchange_account_id) {
                Some(balances) => balances,
                None => bail!(
                    "failed to get balances from balances_dic {:?} for {}",
                    balances_dict,
                    reservation.exchange_account_id
                ),
            };

            let mut balance = match balances.get_mut(&reservation.reservation_currency_code) {
                Some(balance) => balance,
                None => bail!(
                    "failed to get balance from balances {:?} for {}",
                    balances,
                    reservation.reservation_currency_code
                ),
            };
            balance += reservation.get_proportional_cost_amount(reservation.not_approved_amount)?;
        }
        Ok(balances_dict)
    }
}
