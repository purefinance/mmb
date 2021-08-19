use std::collections::{HashMap, HashSet};

use crate::core::balance_manager::balances::Balances;
use crate::core::balances::balance_reservation_manager::BalanceReservationManager;
use crate::core::exchanges::common::TradePlaceAccount;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::orders::fill::OrderFill;

use anyhow::{bail, Result};
use rust_decimal::Decimal;

struct BalanceManager {
    // private readonly IDateTimeService _dateTimeService;
    // private readonly ILogger _logger = Log.ForContext<BalanceManager>();
    // private readonly object _syncObject = new object();
    exchanges_by_id: HashMap<String, Exchange>,

    // private readonly ICurrencyPairToSymbolConverter _currencyPairToSymbolConverter;
    exchange_id_with_restored_positions: HashSet<String>,
    balance_reservation_manager: BalanceReservationManager,
    position_differs_times_in_row_by_exchange_id: HashMap<String, HashMap<String, usize>>,

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

    pub fn restore_balance_state_with_reservations_handling(
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
        // TODO: uncomment when DataRecorder will be added
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
}
