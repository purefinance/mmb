use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_utils::decimal_inverse_sign::DecimalInverseSign;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::{nothing_to_do, DateTime};
use mockall_double::double;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::balance::balance_position_model::BalancePositionModel;
use crate::balance::manager::approved_part::ApprovedPart;
use crate::balance::manager::balance_position_by_fill_amount::BalancePositionByFillAmount;
use crate::balance::manager::balance_request::BalanceRequest;
use crate::balance::manager::balance_reservation::BalanceReservation;
use crate::balance::manager::balances::Balances;
use crate::balance::manager::position_change::PositionChange;
use crate::balance::{
    balance_reservation_storage::BalanceReservationStorage,
    virtual_balance_holder::VirtualBalanceHolder,
};
use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
use crate::exchanges::general::exchange::Exchange;
use crate::explanation::{Explanation, OptionExplanationAddReasonExt};
use crate::misc::reserve_parameters::ReserveParameters;
use crate::misc::service_value_tree::ServiceValueTree;
#[double]
use crate::misc::time::time_manager;
use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use mmb_domain::exchanges::symbol::{BeforeAfter, Symbol};
use mmb_domain::market::{CurrencyCode, CurrencyPair, ExchangeAccountId, MarketAccountId};
use mmb_domain::order::snapshot::ReservationId;
use mmb_domain::order::snapshot::{ClientOrderFillId, ClientOrderId, OrderSide};

use super::balance_reservation_preset::BalanceReservationPreset;

pub(super) struct CanReserveResult {
    can_reserve: bool,
    preset: BalanceReservationPreset,
    potential_position: Option<Decimal>,
    old_balance: Decimal,
    new_balance: Decimal,
}

#[derive(Clone)]
pub(crate) struct BalanceReservationManager {
    pub currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>,
    reserved_amount_in_amount_currency: ServiceValueTree,
    amount_limits_in_amount_currency: ServiceValueTree,

    position_by_fill_amount_in_amount_currency: BalancePositionByFillAmount,

    pub virtual_balance_holder: VirtualBalanceHolder,
    pub balance_reservation_storage: BalanceReservationStorage,

    pub(crate) is_call_from_clone: bool,
}

impl BalanceReservationManager {
    pub fn new(currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>) -> Self {
        Self {
            currency_pair_to_symbol_converter: currency_pair_to_symbol_converter.clone(),
            reserved_amount_in_amount_currency: ServiceValueTree::default(),
            amount_limits_in_amount_currency: ServiceValueTree::default(),
            position_by_fill_amount_in_amount_currency: BalancePositionByFillAmount::default(),
            virtual_balance_holder: VirtualBalanceHolder::new(
                currency_pair_to_symbol_converter.exchanges_by_id().clone(),
            ),
            balance_reservation_storage: BalanceReservationStorage::new(),
            is_call_from_clone: false,
        }
    }

    pub fn exchanges_by_id(&self) -> &HashMap<ExchangeAccountId, Arc<Exchange>> {
        self.currency_pair_to_symbol_converter.exchanges_by_id()
    }

    pub fn update_reserved_balances(
        &mut self,
        reserved_balances_by_id: &HashMap<ReservationId, BalanceReservation>,
    ) {
        self.balance_reservation_storage.clear();
        for (&reservation_id, reservation) in reserved_balances_by_id {
            self.balance_reservation_storage
                .add(reservation_id, reservation.clone());
        }
        self.sync_reservation_amounts();
    }

    pub fn sync_reservation_amounts(&mut self) {
        fn make_balance_request(reservation: &BalanceReservation) -> BalanceRequest {
            BalanceRequest::new(
                reservation.configuration_descriptor,
                reservation.exchange_account_id,
                reservation.symbol.currency_pair(),
                reservation
                    .symbol
                    .get_trade_code(reservation.order_side, BeforeAfter::Before),
            )
        }

        let reservations_by_id = self.balance_reservation_storage.get_all_raw_reservations();

        let mut reserved_by_request = HashMap::with_capacity(reservations_by_id.len());
        for reservation in reservations_by_id.values() {
            let balance_request = make_balance_request(reservation);
            if let Some(grouped_reservations) = reserved_by_request.get_mut(&balance_request) {
                *grouped_reservations += reservation.unreserved_amount;
            } else {
                reserved_by_request.insert(balance_request, reservation.unreserved_amount);
            }
        }

        let mut svt = ServiceValueTree::default();
        for (request, reserved) in reserved_by_request {
            svt.set_by_balance_request(&request, reserved);
        }
        self.reserved_amount_in_amount_currency = svt;
    }

    pub fn restore_fill_amount_limits(
        &mut self,
        amount_limits: ServiceValueTree,
        position_by_fill_amount: BalancePositionByFillAmount,
    ) {
        self.amount_limits_in_amount_currency = amount_limits;
        self.position_by_fill_amount_in_amount_currency = position_by_fill_amount;
    }

    pub fn get_reservation(&self, reservation_id: ReservationId) -> Option<&BalanceReservation> {
        self.balance_reservation_storage.get(reservation_id)
    }

    pub fn get_reservation_expected(&self, reservation_id: ReservationId) -> &BalanceReservation {
        self.balance_reservation_storage
            .get_expected(reservation_id)
    }

    pub fn get_mut_reservation(
        &mut self,
        reservation_id: ReservationId,
    ) -> Option<&mut BalanceReservation> {
        self.balance_reservation_storage.get_mut(reservation_id)
    }

    pub fn get_mut_reservation_expected(
        &mut self,
        reservation_id: ReservationId,
    ) -> &mut BalanceReservation {
        self.balance_reservation_storage
            .get_mut_expected(reservation_id)
    }

    pub fn unreserve(
        &mut self,
        reservation_id: ReservationId,
        amount: Amount,
        client_or_order_id: &Option<ClientOrderId>,
    ) -> Result<()> {
        let reservation = match self.get_reservation(reservation_id) {
            Some(reservation) => reservation,
            None => {
                let reservation_ids = self.balance_reservation_storage.get_reservation_ids();
                if self.is_call_from_clone || amount.is_zero() {
                    // Due to async nature of our trading engine we may receive in Clone reservation_ids which are already removed,
                    // so we need to ignore them instead of throwing an exception
                    log::error!(
                        "Can't find reservation {reservation_id} ({}) for BalanceReservationManager::unreserve {amount} in list: {}",
                        self.is_call_from_clone,
                        reservation_ids.iter().join(", ")
                    );
                    return Ok(());
                }

                bail!("Can't find reservation_id={reservation_id} for BalanceReservationManager::unreserve({amount}) attempt in list: {}", reservation_ids.iter().join(", "));
            }
        };

        let amount_to_unreserve = reservation
            .symbol
            .round_to_remove_amount_precision_error(amount);

        if amount_to_unreserve.is_zero() && !reservation.amount.is_zero() {
            // to prevent error logging in case when amount == 0
            if amount != amount_to_unreserve {
                log::info!("UnReserveInner {} != {}", amount, amount_to_unreserve);
            }
            return Ok(());
        }

        if !self
            .exchanges_by_id()
            .contains_key(&reservation.exchange_account_id)
        {
            log::error!(
                "Trying to BalanceReservationManager::unreserve for not existing exchange {}",
                reservation.exchange_account_id
            );
            return Ok(());
        }

        let balance_params = ReserveParameters::from_reservation(reservation, dec!(0));

        let old_balance = self.get_available_balance(&balance_params, true, &mut None);

        log::info!("VirtualBalanceHolder {}", old_balance);

        self.unreserve_not_approved_part(reservation_id, client_or_order_id, amount_to_unreserve)
            .context("failed unreserve not approved part")?;

        let reservation = self.get_reservation_expected(reservation_id);
        let balance_request = BalanceRequest::from_reservation(reservation);
        self.add_reserved_amount(&balance_request, reservation_id, -amount_to_unreserve, true)?;

        let new_balance = self.get_available_balance(&balance_params, true, &mut None);
        log::info!("VirtualBalanceHolder {}", new_balance);

        let mut reservation = self.get_reservation_expected(reservation_id).clone();
        if reservation.unreserved_amount < dec!(0)
            || reservation.is_amount_within_symbol_margin_error(reservation.unreserved_amount)
        {
            self.balance_reservation_storage.remove(reservation_id);

            if !self.is_call_from_clone {
                log::info!(
                    "Removed balance reservation {} on {}",
                    reservation_id,
                    reservation.exchange_account_id
                );
            }

            if !reservation.unreserved_amount.is_zero() {
                log::error!(
                    "AmountLeft {} != 0 for {} {:?} {} {} {:?}",
                    reservation.unreserved_amount,
                    reservation_id,
                    reservation.symbol.amount_precision,
                    old_balance,
                    new_balance,
                    reservation
                );

                let amount_diff_in_amount_currency = -reservation.unreserved_amount;

                // Compensate amount
                BalanceReservationManager::add_reserved_amount_by_reservation(
                    &balance_request,
                    &mut self.virtual_balance_holder,
                    &mut reservation,
                    &mut self.reserved_amount_in_amount_currency,
                    amount_diff_in_amount_currency,
                    true,
                )?;
            }

            if !self.is_call_from_clone {
                log::info!(
                    "Unreserved {} from {} {} {} {:?} {} {} {} {} {:?} {} {}",
                    amount_to_unreserve,
                    reservation_id,
                    reservation.exchange_account_id,
                    reservation.reservation_currency_code,
                    reservation.order_side,
                    reservation.price,
                    reservation.amount,
                    reservation.not_approved_amount,
                    reservation.unreserved_amount,
                    client_or_order_id,
                    old_balance,
                    new_balance
                );
            }
        }
        Ok(())
    }

    pub fn unreserve_expected(
        &mut self,
        reservation_id: ReservationId,
        amount: Amount,
        client_or_order_id: &Option<ClientOrderId>,
    ) {
        self.unreserve(reservation_id, amount, client_or_order_id)
            .with_expect(|| {
                format!(
                "Failed to unreserve: reservation_id = {}, amount = {}, client_or_order_id = {:?}",
                reservation_id, amount, client_or_order_id
            )
            });
    }

    fn get_available_balance(
        &self,
        parameters: &ReserveParameters,
        include_free_amount: bool,
        explanation: &mut Option<Explanation>,
    ) -> Amount {
        self.try_get_available_balance(
            parameters.configuration_descriptor,
            parameters.exchange_account_id,
            parameters.symbol.clone(),
            parameters.order_side,
            parameters.price,
            include_free_amount,
            false,
            explanation,
        )
        .unwrap_or(dec!(0))
    }

    pub fn try_get_available_balance_with_unknown_side(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        currency_code: CurrencyCode,
        price: Price,
    ) -> Option<Amount> {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            if symbol.get_trade_code(side, BeforeAfter::Before) == currency_code {
                return self.try_get_available_balance(
                    configuration_descriptor,
                    exchange_account_id,
                    symbol,
                    side,
                    price,
                    true,
                    false,
                    &mut None,
                );
            }
        }

        let request = BalanceRequest::new(
            configuration_descriptor,
            exchange_account_id,
            symbol.currency_pair(),
            currency_code,
        );
        self.virtual_balance_holder
            .get_virtual_balance(&request, symbol, Some(price), &mut None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_get_available_balance(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
        price: Price,
        include_free_amount: bool,
        is_leveraged: bool,
        explanation: &mut Option<Explanation>,
    ) -> Option<Amount> {
        let currency_code = symbol.get_trade_code(side, BeforeAfter::Before);
        let request = BalanceRequest::new(
            configuration_descriptor,
            exchange_account_id,
            symbol.currency_pair(),
            currency_code,
        );
        let balance_in_currency_code = self.virtual_balance_holder.get_virtual_balance(
            &request,
            symbol.clone(),
            Some(price),
            explanation,
        );

        explanation.with_reason(|| {
            format!(
                "balance_in_currency_code_raw = {:?}",
                balance_in_currency_code
            )
        });

        let mut balance_in_currency_code = balance_in_currency_code?;

        let leverage = self.get_leverage(exchange_account_id, symbol.currency_pair());

        explanation.with_reason(|| format!("leverage = {:?}", leverage));

        if symbol.is_derivative {
            if include_free_amount {
                let free_amount_in_amount_currency_code = self
                    .get_unreserved_position_in_amount_currency_code(
                        exchange_account_id,
                        symbol.clone(),
                        side,
                    );

                explanation.with_reason(|| {
                    format!(
                        "free_amount_in_amount_currency_code with leverage and amount_multiplier = {}",
                        free_amount_in_amount_currency_code
                    )
                });

                let mut free_amount_in_currency_code = symbol
                    .convert_amount_from_amount_currency_code(
                        currency_code,
                        free_amount_in_amount_currency_code,
                        price,
                    );
                free_amount_in_currency_code /= leverage;
                free_amount_in_currency_code *= symbol.amount_multiplier;

                explanation.with_reason(|| {
                    format!(
                        "free_amount_in_currency_code = {}",
                        free_amount_in_currency_code
                    )
                });

                balance_in_currency_code += free_amount_in_currency_code;

                explanation.with_reason(|| {
                    format!(
                        "balance_in_currency_code with free amount: {}",
                        balance_in_currency_code
                    )
                });
            }

            balance_in_currency_code -= BalanceReservationManager::get_untouchable_amount(
                symbol.clone(),
                balance_in_currency_code,
            );

            explanation.with_reason(|| {
                format!(
                    "balance_in_currency_code without untouchable: {}",
                    balance_in_currency_code
                )
            });
        }
        if self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request)
            .is_some()
        {
            balance_in_currency_code = self.get_balance_with_applied_limits(
                &request,
                symbol.clone(),
                side,
                balance_in_currency_code,
                price,
                leverage,
                explanation,
            );
        }

        explanation.with_reason(|| {
            format!("balance_in_currency_code with limit: {balance_in_currency_code}")
        });

        // isLeveraged is used when we need to know how much funds we can use for orders
        if is_leveraged {
            balance_in_currency_code *= leverage;
            balance_in_currency_code /= symbol.amount_multiplier;

            explanation.with_reason(|| format!("balance_in_currency_code with leverage and multiplier: {balance_in_currency_code}"));
        }
        Some(balance_in_currency_code)
    }

    pub fn get_position_in_amount_currency_code(
        &self,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> Decimal {
        if !symbol.is_derivative {
            return dec!(0);
        }

        let current_position = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, symbol.currency_pair())
            .unwrap_or(dec!(0));
        match side {
            OrderSide::Buy => dec!(0).max(-current_position),
            OrderSide::Sell => dec!(0).max(current_position),
        }
    }

    fn get_unreserved_position_in_amount_currency_code(
        &self,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> Decimal {
        let position = self.get_position_in_amount_currency_code(exchange_account_id, symbol, side);

        let taken_amount = self
            .balance_reservation_storage
            .get_all_raw_reservations()
            .iter()
            .filter(|(_, balance_reservation)| balance_reservation.order_side == side)
            .map(|(_, balance_reservation)| balance_reservation.taken_free_amount)
            .sum::<Amount>();

        dec!(0).max(position - taken_amount)
    }

    #[allow(clippy::too_many_arguments)]
    fn get_balance_with_applied_limits(
        &self,
        request: &BalanceRequest,
        symbol: Arc<Symbol>,
        side: OrderSide,
        mut balance_in_currency_code: Amount,
        price: Price,
        leverage: Decimal,
        explanation: &mut Option<Explanation>,
    ) -> Amount {
        let position = self.get_position_values(
            request.configuration_descriptor,
            request.exchange_account_id,
            symbol.clone(),
            side,
        );

        let position_amount_in_amount_currency = position.position;
        explanation.with_reason(|| {
            format!("position_amount_in_amount_currency: {position_amount_in_amount_currency}")
        });

        let reserved_amount_in_amount_currency = self
            .reserved_amount_in_amount_currency
            .get_by_balance_request(request)
            .unwrap_or(dec!(0));

        explanation.with_reason(|| {
            format!("reserved_amount_in_amount_currency: {reserved_amount_in_amount_currency}")
        });

        let reservation_with_fills_in_amount_currency =
            reserved_amount_in_amount_currency + position_amount_in_amount_currency;
        explanation.with_reason(|| {
            format!("reservation_with_fills_in_amount_currency: {reservation_with_fills_in_amount_currency}")
        });

        let total_amount_limit_in_amount_currency = position.limit.unwrap_or(dec!(0));
        explanation.with_reason(|| {
            format!(
                "total_amount_limit_in_amount_currency: {total_amount_limit_in_amount_currency}"
            )
        });

        let limit_left_in_amount_currency =
            total_amount_limit_in_amount_currency - reservation_with_fills_in_amount_currency;
        explanation.with_reason(|| {
            format!("limit_left_in_amount_currency: {limit_left_in_amount_currency}")
        });

        //AmountLimit is applied to full amount
        balance_in_currency_code *= leverage;
        balance_in_currency_code /= symbol.amount_multiplier;
        explanation.with_reason(|| {
            format!(
                "balance_in_currency_code with leverage and multiplier: {balance_in_currency_code}"
            )
        });

        let balance_in_amount_currency = symbol.convert_amount_into_amount_currency_code(
            request.currency_code,
            balance_in_currency_code,
            price,
        );
        explanation.with_reason(|| {
            format!("balance_in_amount_currency with leverage and multiplier: {balance_in_amount_currency}")
        });

        let limited_balance_in_amount_currency =
            balance_in_amount_currency.min(limit_left_in_amount_currency);
        explanation.with_reason(|| {
            format!("limited_balance_in_amount_currency: {limited_balance_in_amount_currency}")
        });

        let mut limited_balance_in_currency_code = symbol.convert_amount_from_amount_currency_code(
            request.currency_code,
            limited_balance_in_amount_currency,
            price,
        );
        explanation.with_reason(|| {
            format!("limited_balance_in_currency_code: {limited_balance_in_currency_code}")
        });

        //converting back to pure balance
        limited_balance_in_currency_code /= leverage;
        limited_balance_in_currency_code *= symbol.amount_multiplier;
        explanation.with_reason(|| {
            format!("limited_balance_in_currency_code without leverage and multiplier: {limited_balance_in_currency_code}")
        });

        if limited_balance_in_currency_code < dec!(0) {
            log::warn!("Balance {limited_balance_in_currency_code} < 0 ({total_amount_limit_in_amount_currency} - ({reserved_amount_in_amount_currency} + {position_amount_in_amount_currency}) {balance_in_amount_currency} for {request:?} {symbol:?}");
        };

        dec!(0).max(limited_balance_in_currency_code)
    }

    fn get_untouchable_amount(symbol: Arc<Symbol>, amount: Amount) -> Amount {
        // We want to keep the trading engine from reserving all the balance for derivatives as so far we don't take into account
        // many derivative nuances (commissions, funding, probably something else
        match symbol.is_derivative {
            true => amount * dec!(0.05),
            false => dec!(0),
        }
    }

    fn get_leverage(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
    ) -> Decimal {
        *self
            .exchanges_by_id()
            .get(&exchange_account_id)
            .with_expect(|| format!("failed to get exchange {exchange_account_id}"))
            .leverage_by_currency_pair
            .get(&currency_pair)
            .as_deref()
            .expect("failed to get leverage")
    }

    fn get_position_values(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> BalancePositionModel {
        let currency_code = symbol.get_trade_code(side, BeforeAfter::Before);
        let request = BalanceRequest::new(
            configuration_descriptor,
            exchange_account_id,
            symbol.currency_pair(),
            currency_code,
        );
        let total_amount_limit_in_amount_currency = self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request);

        let position = self.get_position(exchange_account_id, symbol.currency_pair(), side);

        BalancePositionModel {
            position,
            limit: total_amount_limit_in_amount_currency,
        }
    }

    pub fn get_position(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        side: OrderSide,
    ) -> Decimal {
        let symbol = self
            .currency_pair_to_symbol_converter
            .get_symbol(exchange_account_id, currency_pair);

        let currency_code = symbol.get_trade_code(side, BeforeAfter::Before);
        let mut position_in_amount_currency = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, currency_pair)
            .unwrap_or(dec!(0));

        match (
            symbol.is_derivative,
            currency_code == symbol.base_currency_code,
        ) {
            (true, true) => position_in_amount_currency.inverse_sign(),
            (false, false) => position_in_amount_currency.inverse_sign(),
            _ => nothing_to_do(),
        }

        position_in_amount_currency
    }

    fn unreserve_not_approved_part(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &Option<ClientOrderId>,
        amount_to_unreserve: Amount,
    ) -> Result<()> {
        let reservation = self.get_mut_reservation_expected(reservation_id);
        let client_order_id = match client_order_id {
            Some(client_order_id) => client_order_id,
            None => {
                reservation.not_approved_amount -= amount_to_unreserve;
                // this case will be handled by UnReserve itself
                if reservation.not_approved_amount < dec!(0)
                    && reservation.unreserved_amount > amount_to_unreserve
                {
                    bail!("Possibly BalanceReservationManager::unreserve_not_approved_part {reservation_id} should be called with clientOrderId parameter");
                }
                return Ok(());
            }
        };

        let approved_part = match reservation.approved_parts.get_mut(client_order_id) {
            Some(approved_part) => approved_part,
            None => {
                log::warn!("unreserve({reservation_id}, {amount_to_unreserve}) called with clientOrderId {client_order_id} for reservation without the approved part {reservation:?}");
                reservation.not_approved_amount -= amount_to_unreserve;
                if reservation.not_approved_amount < dec!(0) {
                    log::error!("not_approved_amount for {reservation_id} was unreserved for the missing order {client_order_id} and now < 0 {reservation:?}");
                }
                return Ok(());
            }
        };

        let new_unreserved_amount_for_approved_part =
            approved_part.unreserved_amount - amount_to_unreserve;
        if new_unreserved_amount_for_approved_part < dec!(0) {
            bail!("Attempt to unreserve more than was approved for order {client_order_id} ({reservation_id}): {amount_to_unreserve} > {}", approved_part.unreserved_amount);
        }
        approved_part.unreserved_amount = new_unreserved_amount_for_approved_part;
        Ok(())
    }

    fn add_reserved_amount_by_reservation(
        request: &BalanceRequest,
        virtual_balance_holder: &mut VirtualBalanceHolder,
        reservation: &mut BalanceReservation,
        reserved_amount_in_amount_currency: &mut ServiceValueTree,
        amount_diff_in_amount_currency: Amount,
        update_balance: bool,
    ) -> Result<()> {
        if update_balance {
            let cost = reservation
                .get_proportional_cost_amount(amount_diff_in_amount_currency)
                .with_context(|| format!("Failed to get proportional cost amount form {reservation:?} with {amount_diff_in_amount_currency}"))?;

            virtual_balance_holder.add_balance_by_symbol(
                request,
                reservation.symbol.clone(),
                -cost,
                reservation.price,
            );
        }

        reservation.unreserved_amount += amount_diff_in_amount_currency;

        // global reservation indicator
        let res_amount_request = BalanceRequest::new(
            request.configuration_descriptor,
            request.exchange_account_id,
            request.currency_pair,
            reservation.reservation_currency_code,
        );

        reserved_amount_in_amount_currency
            .add_by_request(&res_amount_request, amount_diff_in_amount_currency);
        Ok(())
    }

    fn add_reserved_amount(
        &mut self,
        balance_request: &BalanceRequest,
        reservation_id: ReservationId,
        amount_diff_in_amount_currency: Amount,
        update_balance: bool,
    ) -> Result<()> {
        BalanceReservationManager::add_reserved_amount_by_reservation(
            balance_request,
            &mut self.virtual_balance_holder,
            self.balance_reservation_storage
                .get_mut_expected(reservation_id),
            &mut self.reserved_amount_in_amount_currency,
            amount_diff_in_amount_currency,
            update_balance,
        )
    }

    fn add_reserved_amount_expected(
        &mut self,
        balance_request: &BalanceRequest,
        reservation_id: ReservationId,
        amount_diff_in_amount_currency: Amount,
        update_balance: bool,
    ) {
        self.add_reserved_amount(
            balance_request,
            reservation_id,
            amount_diff_in_amount_currency,
            update_balance,
        )
        .with_expect(|| format!("failed to add reserved amount {balance_request:?} {reservation_id} {amount_diff_in_amount_currency}"));
    }

    pub fn get_state(&self) -> Balances {
        Balances::new(
            self.virtual_balance_holder
                .get_raw_exchange_balances()
                .clone(),
            time_manager::now(),
            self.virtual_balance_holder
                .get_virtual_balance_diffs()
                .clone(),
            self.reserved_amount_in_amount_currency.clone(),
            self.position_by_fill_amount_in_amount_currency.clone(),
            self.amount_limits_in_amount_currency.clone(),
            self.balance_reservation_storage
                .get_all_raw_reservations()
                .clone(),
        )
    }

    pub(crate) fn restore_fill_amount_position(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        new_position: Decimal,
    ) -> Result<()> {
        if !symbol.is_derivative {
            bail!("restore_fill_amount_position is available only for derivative exchanges");
        }
        let previous_value = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, symbol.currency_pair());

        let now = time_manager::now();

        self.position_by_fill_amount_in_amount_currency.set(
            exchange_account_id,
            symbol.currency_pair(),
            previous_value,
            new_position,
            None,
            now,
        );
        Ok(())
    }

    pub fn get_last_position_change_before_period(
        &self,
        market_account_id: &MarketAccountId,
        start_of_period: DateTime,
    ) -> Option<PositionChange> {
        self.position_by_fill_amount_in_amount_currency
            .get_last_position_change_before_period(market_account_id, start_of_period)
    }

    pub fn get_fill_amount_position_percent(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> Decimal {
        let position =
            self.get_position_values(configuration_descriptor, exchange_account_id, symbol, side);

        let limit = position
            .limit
            .expect("failed to get_fill_amount_position_percent, limit is None");

        dec!(1).min(dec!(0).max(position.position / limit))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn handle_position_fill_amount_change(
        &mut self,
        side: OrderSide,
        before_after: BeforeAfter,
        client_order_fill_id: &Option<ClientOrderFillId>,
        fill_amount: Amount,
        price: Price,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
    ) -> (Amount, CurrencyCode) {
        let mut change_amount_in_currency = dec!(0);

        let currency_code = symbol.get_trade_code(side, before_after);
        let request = BalanceRequest::new(
            configuration_descriptor,
            exchange_account_id,
            symbol.currency_pair(),
            currency_code,
        );

        if !symbol.is_derivative {
            self.virtual_balance_holder.add_balance_by_symbol(
                &request,
                symbol.clone(),
                -fill_amount,
                price,
            );

            change_amount_in_currency =
                symbol.convert_amount_from_amount_currency_code(currency_code, fill_amount, price);
        }
        if symbol.amount_currency_code == currency_code {
            let mut position_change = fill_amount;
            if symbol.is_derivative {
                let free_amount = self.get_position_in_amount_currency_code(
                    exchange_account_id,
                    symbol.clone(),
                    side,
                );
                let move_amount = fill_amount.abs();
                let (add_amount, sub_amount) = if free_amount - move_amount >= dec!(0) {
                    (move_amount, dec!(0))
                } else {
                    (free_amount, (free_amount - move_amount).abs())
                };

                let leverage = self.get_leverage(exchange_account_id, symbol.currency_pair());
                let diff_in_amount_currency =
                    (add_amount - sub_amount) / leverage * symbol.amount_multiplier;
                self.virtual_balance_holder.add_balance_by_symbol(
                    &request,
                    symbol.clone(),
                    diff_in_amount_currency,
                    price,
                );

                change_amount_in_currency = symbol.convert_amount_from_amount_currency_code(
                    currency_code,
                    diff_in_amount_currency,
                    price,
                );

                // reversed derivative
                if symbol.amount_currency_code == symbol.base_currency_code() {
                    position_change.inverse_sign();
                }
            }
            let now = time_manager::now();
            self.position_by_fill_amount_in_amount_currency.add(
                request.exchange_account_id,
                request.currency_pair,
                position_change,
                client_order_fill_id.clone(),
                now,
            );
            self.validate_position_and_limits(&request);
        }
        (change_amount_in_currency, currency_code)
    }

    fn validate_position_and_limits(&self, request: &BalanceRequest) {
        let limit = match self
            .amount_limits_in_amount_currency
            .get_by_balance_request(request)
        {
            Some(limit) => limit,
            None => return,
        };

        let position = match self
            .position_by_fill_amount_in_amount_currency
            .get(request.exchange_account_id, request.currency_pair)
        {
            Some(position) => position,
            None => return,
        };

        if position.abs() > limit {
            log::error!(
                "Position > Limit: outstanding situation {position} > {limit} ({request:?})"
            );
        }
    }

    pub fn cancel_approved_reservation(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &ClientOrderId,
    ) {
        let reservation = match self.get_mut_reservation(reservation_id) {
            Some(reservation_id) => reservation_id,
            None => {
                log::error!(
                    "Can't find reservation {reservation_id} in {}",
                    self.balance_reservation_storage
                        .get_reservation_ids()
                        .iter()
                        .join(", ")
                );
                return;
            }
        };

        let approved_part = match reservation.approved_parts.get_mut(client_order_id) {
            Some(approved_part) => approved_part,
            None => {
                log::error!("There is no approved part for order {client_order_id}");
                return;
            }
        };

        if approved_part.is_canceled {
            panic!("Approved part was already canceled for {client_order_id} {reservation_id}");
        }

        reservation.not_approved_amount += approved_part.unreserved_amount;
        approved_part.is_canceled = true;
        log::info!(
            "Canceled approved part for order {client_order_id} with {}",
            approved_part.unreserved_amount
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn handle_position_fill_amount_change_commission(
        &mut self,
        commission_currency_code: CurrencyCode,
        commission_amount: Amount,
        converted_commission_currency_code: CurrencyCode,
        converted_commission_amount: Amount,
        price: Price,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
    ) {
        let leverage = self.get_leverage(exchange_account_id, symbol.currency_pair());
        if !symbol.is_derivative || symbol.balance_currency_code == Some(commission_currency_code) {
            let request = BalanceRequest::new(
                configuration_descriptor,
                exchange_account_id,
                symbol.currency_pair(),
                commission_currency_code,
            );
            let res_commission_amount = commission_amount / leverage;
            self.virtual_balance_holder
                .add_balance(&request, -res_commission_amount);
        } else {
            let request = BalanceRequest::new(
                configuration_descriptor,
                exchange_account_id,
                symbol.currency_pair(),
                converted_commission_currency_code,
            );
            let commission_in_amount_currency = symbol.convert_amount_into_amount_currency_code(
                converted_commission_currency_code,
                converted_commission_amount,
                price,
            );
            let res_commission_amount_in_amount_currency = commission_in_amount_currency / leverage;
            self.virtual_balance_holder.add_balance_by_symbol(
                &request,
                symbol,
                -res_commission_amount_in_amount_currency,
                price,
            );
        }
    }

    pub fn approve_reservation(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &ClientOrderId,
        amount: Amount,
    ) -> Result<()> {
        let approve_time = time_manager::now();
        let reservation = match self.get_mut_reservation(reservation_id) {
            Some(reservation) => reservation,
            None => {
                log::error!(
                    "Can't find reservation {reservation_id} in {}",
                    self.balance_reservation_storage
                        .get_reservation_ids()
                        .iter()
                        .join(", ")
                );
                return Ok(());
            }
        };

        if reservation.approved_parts.contains_key(client_order_id) {
            log::error!("Order {client_order_id} cannot be approved multiple times");
            return Ok(());
        }

        reservation.not_approved_amount -= amount;

        if reservation.not_approved_amount < dec!(0)
            && !reservation.is_amount_within_symbol_margin_error(reservation.not_approved_amount)
        {
            log::error!("RestApprovedAmount < 0 for order {client_order_id} {reservation_id} {amount} {reservation:?}");
            bail!("RestApprovedAmount < 0 for order {client_order_id} {reservation_id} {amount}");
        }
        reservation.approved_parts.insert(
            client_order_id.clone(),
            ApprovedPart::new(approve_time, client_order_id.clone(), amount),
        );

        log::info!("Order {client_order_id} was approved with {amount}");
        Ok(())
    }

    pub fn try_transfer_reservation(
        &mut self,
        src_reservation_id: ReservationId,
        dst_reservation_id: ReservationId,
        amount: Amount,
        client_order_id: &Option<ClientOrderId>,
    ) -> bool {
        let src_reservation = self.get_reservation_expected(src_reservation_id);

        let dst_reservation = self.get_reservation_expected(dst_reservation_id);

        if src_reservation.configuration_descriptor != dst_reservation.configuration_descriptor
            || src_reservation.exchange_account_id != dst_reservation.exchange_account_id
            || src_reservation.symbol != dst_reservation.symbol
            || src_reservation.order_side != dst_reservation.order_side
        {
            panic!("Reservations {src_reservation:?} and {dst_reservation:?} are from different sources");
        }

        let amount_to_move = src_reservation
            .symbol
            .round_to_remove_amount_precision_error_expected(amount);
        if amount_to_move.is_zero() {
            log::warn!(
                "Can't transfer zero amount from {src_reservation_id} to {dst_reservation_id}"
            );
            return false;
        }

        if src_reservation.price != dst_reservation.price {
            // special case for derivatives because balance for AmountCurrency is auto-calculated
            if src_reservation.symbol.is_derivative {
                // check if we have enough balance for the operation
                let add_amount = src_reservation.convert_in_reservation_currency(amount_to_move);
                let sub_amount = dst_reservation.convert_in_reservation_currency(amount_to_move);

                let balance_diff_amount = add_amount - sub_amount;

                let available_balance = self
                    .try_get_available_balance(
                        dst_reservation.configuration_descriptor,
                        dst_reservation.exchange_account_id,
                        dst_reservation.symbol.clone(),
                        dst_reservation.order_side,
                        dst_reservation.price,
                        true,
                        false,
                        &mut None,
                    )
                    .with_expect(|| {
                        format!("failed to get available balance for {dst_reservation:?}")
                    });
                if available_balance + balance_diff_amount < dec!(0) {
                    log::warn!("Can't transfer {amount_to_move} because there will be insufficient balance ({src_reservation_id} => {dst_reservation_id})");
                    return false;
                }
            }
        }

        // we can safely move amount ignoring price because of check that have been done before
        self.transfer_amount(
            src_reservation_id,
            dst_reservation_id,
            amount_to_move,
            client_order_id,
        );
        true
    }

    fn transfer_amount(
        &mut self,
        src_reservation_id: ReservationId,
        dst_reservation_id: ReservationId,
        amount_to_move: Amount,
        client_order_id: &Option<ClientOrderId>,
    ) {
        let src_reservation = self.get_reservation_expected(src_reservation_id);
        let new_src_unreserved_amount = src_reservation.unreserved_amount - amount_to_move;
        log::info!("trying to update src unreserved amount for transfer: {src_reservation:?} {new_src_unreserved_amount} {client_order_id:?}");
        let src_cost_diff = self.update_unreserved_amount_for_transfer(
            src_reservation_id,
            new_src_unreserved_amount,
            client_order_id,
            true,
            dec!(0),
        );

        let dst_reservation = self.get_reservation_expected(dst_reservation_id);
        let new_dst_unreserved_amount = dst_reservation.unreserved_amount + amount_to_move;
        log::info!("trying to update dst unreserved amount for transfer: {dst_reservation:?} {new_dst_unreserved_amount} {client_order_id:?}");
        let _ = self.update_unreserved_amount_for_transfer(
            dst_reservation_id,
            new_dst_unreserved_amount,
            client_order_id,
            false,
            -src_cost_diff,
        );

        log::info!("Successfully transferred {amount_to_move} from {src_reservation_id} to {dst_reservation_id}");
    }

    fn update_unreserved_amount_for_transfer(
        &mut self,
        reservation_id: ReservationId,
        new_unreserved_amount: Amount,
        client_order_id: &Option<ClientOrderId>,
        is_src_request: bool,
        target_cost_diff: Decimal,
    ) -> Decimal {
        let approve_time = time_manager::now();
        let reservation = self.get_mut_reservation_expected(reservation_id);
        // we should check the case when we have insignificant calculation errors
        if new_unreserved_amount < dec!(0)
            && !reservation.is_amount_within_symbol_margin_error(new_unreserved_amount)
        {
            panic!("Can't set {new_unreserved_amount} amount to reservation {reservation_id}");
        }

        let reservation_amount_diff = new_unreserved_amount - reservation.unreserved_amount;
        if let Some(client_order_id) = client_order_id {
            if let Some(approved_part) = reservation.approved_parts.get(client_order_id) {
                let new_amount = approved_part.unreserved_amount + reservation_amount_diff;
                if reservation.is_amount_within_symbol_margin_error(new_amount) {
                    let _ = reservation.approved_parts.remove(client_order_id);
                } else if new_amount < dec!(0) {
                    panic!(
                        "Attempt to transfer more amount ({reservation_amount_diff}) than we have ({}) for approved part by ClientOrderId {client_order_id}",
                        reservation
                            .approved_parts
                            .get_mut(client_order_id)
                            .expect("fix me").unreserved_amount);
                } else {
                    let approved_part = reservation
                        .approved_parts
                        .get_mut(client_order_id)
                        .expect("failed to get approved part");
                    approved_part.unreserved_amount = new_amount;
                    approved_part.amount += reservation_amount_diff;
                }
            } else {
                if is_src_request {
                    panic!("Can't find approved part {client_order_id} for {reservation_id}");
                }

                reservation.approved_parts.insert(
                    client_order_id.clone(),
                    ApprovedPart::new(
                        approve_time,
                        client_order_id.clone(),
                        reservation_amount_diff,
                    ),
                );
            }
        } else {
            reservation.not_approved_amount += reservation_amount_diff;
        }

        let balance_request = BalanceRequest::from_reservation(reservation);

        self.add_reserved_amount_expected(
            &balance_request,
            reservation_id,
            reservation_amount_diff,
            false,
        );
        let reservation = self.get_mut_reservation_expected(reservation_id);

        let cost_diff = if is_src_request {
            reservation
                .get_proportional_cost_amount(reservation_amount_diff)
                .expect("Failed to get proportional cost amount")
        } else {
            target_cost_diff
        };
        let buff_price = reservation.price;
        let buff_symbol = reservation.symbol.clone();

        self.virtual_balance_holder.add_balance_by_symbol(
            &balance_request,
            buff_symbol,
            -cost_diff,
            buff_price,
        );
        let reservation = self.get_mut_reservation_expected(reservation_id);

        reservation.cost += cost_diff;
        reservation.amount += reservation_amount_diff;
        let reservation = self.get_reservation_expected(reservation_id).clone();

        if reservation.is_amount_within_symbol_margin_error(new_unreserved_amount) {
            self.balance_reservation_storage.remove(reservation_id);

            if !new_unreserved_amount.is_zero() {
                log::error!(
                    "Transfer: AmountLeft {} != 0 for {reservation_id} {reservation:?}",
                    reservation.unreserved_amount,
                );
            }
        }
        log::info!(
            "Updated reservation {reservation_id} {} {} {:?} {} {} {reservation_amount_diff}",
            reservation.exchange_account_id,
            reservation.reservation_currency_code,
            reservation.order_side,
            reservation.price,
            reservation.amount,
        );
        cost_diff
    }

    pub fn try_reserve_multiple(
        &mut self,
        reserve_parameters: &[ReserveParameters],
        explanation: &mut Option<Explanation>,
    ) -> Option<Vec<ReservationId>> {
        let successful_reservations = reserve_parameters
            .iter()
            .filter_map(|rp| self.try_reserve(rp, explanation).map(|id| (id, rp)))
            .collect_vec();

        if successful_reservations.len() != reserve_parameters.len() {
            for (res_id, res_params) in successful_reservations {
                self.unreserve_expected(res_id, res_params.amount, &None);
            }
            return None;
        }
        let result_vec = successful_reservations.iter().map(|x| x.0).collect_vec();

        Some(result_vec)
    }

    pub fn try_reserve(
        &mut self,
        reserve_parameters: &ReserveParameters,
        explanation: &mut Option<Explanation>,
    ) -> Option<ReservationId> {
        let can_reserve_result = self.can_reserve_core(reserve_parameters, explanation);
        if !can_reserve_result.can_reserve {
            log::info!(
                "Failed to reserve {} {} {:?} {} {} {reserve_parameters:?}",
                can_reserve_result.preset.reservation_currency_code,
                can_reserve_result
                    .preset
                    .amount_in_reservation_currency_code,
                can_reserve_result.potential_position,
                can_reserve_result.old_balance,
                can_reserve_result.new_balance,
            );
            return None;
        }

        let request = BalanceRequest::new(
            reserve_parameters.configuration_descriptor,
            reserve_parameters.exchange_account_id,
            reserve_parameters.symbol.currency_pair(),
            can_reserve_result.preset.reservation_currency_code,
        );
        let reservation = BalanceReservation::new(
            reserve_parameters.configuration_descriptor,
            reserve_parameters.exchange_account_id,
            reserve_parameters.symbol.clone(),
            reserve_parameters.order_side,
            reserve_parameters.price,
            reserve_parameters.amount,
            can_reserve_result
                .preset
                .taken_free_amount_in_amount_currency_code,
            can_reserve_result.preset.cost_in_amount_currency_code,
            can_reserve_result.preset.reservation_currency_code,
        );

        let reservation_id = ReservationId::generate();
        log::info!(
            "Trying to reserve {reservation_id:?} {} {} {:?} {} {} {reservation:?}",
            can_reserve_result.preset.reservation_currency_code,
            can_reserve_result
                .preset
                .amount_in_reservation_currency_code,
            can_reserve_result.potential_position,
            can_reserve_result.old_balance,
            can_reserve_result.new_balance,
        );

        self.balance_reservation_storage
            .add(reservation_id, reservation);
        self.add_reserved_amount_expected(
            &request,
            reservation_id,
            reserve_parameters.amount,
            true,
        );

        log::info!("Reserved successfully");
        Some(reservation_id)
    }

    fn can_reserve_core(
        &self,
        reserve_parameters: &ReserveParameters,
        explanation: &mut Option<Explanation>,
    ) -> CanReserveResult {
        let preset = self.get_currency_code_and_reservation_amount(reserve_parameters, explanation);
        //We set includeFreeAmount to false because we already took FreeAmount into consideration while calculating the preset
        //Otherwise we would count FreeAmount twice which is wrong
        let old_balance = self.get_available_balance(reserve_parameters, false, explanation);

        let preset_cost = preset.cost_in_reservation_currency_code;

        let new_balance = old_balance - preset_cost;

        explanation.with_reason(|| {
            format!(
                "old_balance: {old_balance} preset_cost: {preset_cost} new_balance: {new_balance}"
            )
        });

        let (can_reserve, potential_position) = self.can_reserve_with_limit(reserve_parameters);

        if !can_reserve {
            return CanReserveResult {
                can_reserve: false,
                preset,
                potential_position,
                old_balance,
                new_balance,
            };
        }

        //Spot trading might need a more precise solution
        let rounded_balance = reserve_parameters
            .symbol
            .round_to_remove_amount_precision_error_expected(new_balance);
        CanReserveResult {
            can_reserve: rounded_balance >= dec!(0),
            preset,
            potential_position,
            old_balance,
            new_balance,
        }
    }

    /// The sign of returned Decimal value calculate over ReserveParameters::order_side.
    /// for example if side is 'Sell' and we have more filled amount for 'Sell' orders the sign will be positive
    /// and negative if 'Sell' amount is less than 'Buy'. The same for 'Buy' order if we bought more than sold
    /// the sign will be positive otherwise - negative.
    ///     Example:
    ///         position is 0 and we trying to reserve order for Buy 10 amount(ReserveParameters::order_side = OrderSide::Buy)
    ///         the function will return - (bool, Some(10))
    ///         next step we trying to reserve order for Sell 1 amount(ReserveParameters::order_side = OrderSide::Sell)
    ///         the function will return - (bool, Some(-9))
    fn can_reserve_with_limit(
        &self,
        reserve_parameters: &ReserveParameters,
    ) -> (bool, Option<Decimal>) {
        let reservation_currency_code = reserve_parameters
            .symbol
            .get_trade_code(reserve_parameters.order_side, BeforeAfter::Before);

        let request = BalanceRequest::new(
            reserve_parameters.configuration_descriptor,
            reserve_parameters.exchange_account_id,
            reserve_parameters.symbol.currency_pair(),
            reservation_currency_code,
        );

        let limit = match self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request)
        {
            Some(limit) => limit,
            None => {
                return (true, None);
            }
        };

        let reserved_amount = self
            .reserved_amount_in_amount_currency
            .get_by_balance_request(&request)
            .unwrap_or(dec!(0));
        let new_reserved_amount = reserved_amount + reserve_parameters.amount;

        // The sign depends on reserve_parameters.order_side look comment for this function
        let position = self.get_position(
            request.exchange_account_id,
            request.currency_pair,
            reserve_parameters.order_side,
        );

        let potential_position = position + new_reserved_amount;

        let potential_position_abs = potential_position.abs();
        if potential_position_abs <= limit {
            // position is within limit range
            return (true, Some(potential_position));
        }

        // we are out of limit range there, so it is okay if we are moving to the limit
        (
            potential_position_abs < position.abs(),
            Some(potential_position),
        )
    }

    fn get_currency_code_and_reservation_amount(
        &self,
        reserve_parameters: &ReserveParameters,
        explanation: &mut Option<Explanation>,
    ) -> BalanceReservationPreset {
        let price = reserve_parameters.price;
        let amount = reserve_parameters.amount;
        let symbol = reserve_parameters.symbol.clone();

        let reservation_currency_code = self
            .exchanges_by_id()
            .get(&reserve_parameters.exchange_account_id)
            .expect("failed to get exchange")
            .get_balance_reservation_currency_code(symbol.clone(), reserve_parameters.order_side);

        let amount_in_reservation_currency_code = symbol.convert_amount_from_amount_currency_code(
            reservation_currency_code,
            amount,
            price,
        );

        let (cost_in_amount_currency_code, taken_free_amount) =
            self.calculate_reservation_cost(reserve_parameters);
        let cost_in_reservation_currency_code = symbol.convert_amount_from_amount_currency_code(
            reservation_currency_code,
            cost_in_amount_currency_code,
            price,
        );

        explanation.with_reason(|| {
            format!("cost_in_reservation_currency_code: {cost_in_reservation_currency_code} taken_free_amount: {taken_free_amount}")
        });

        BalanceReservationPreset::new(
            reservation_currency_code,
            amount_in_reservation_currency_code,
            taken_free_amount,
            cost_in_reservation_currency_code,
            cost_in_amount_currency_code,
        )
    }

    fn calculate_reservation_cost(
        &self,
        reserve_parameters: &ReserveParameters,
    ) -> (Amount, Amount) {
        if !reserve_parameters.symbol.is_derivative {
            return (reserve_parameters.amount, dec!(0));
        }

        let free_amount = self.get_unreserved_position_in_amount_currency_code(
            reserve_parameters.exchange_account_id,
            reserve_parameters.symbol.clone(),
            reserve_parameters.order_side,
        );

        let amount_to_pay_for = dec!(0).max(reserve_parameters.amount - free_amount);

        let taken_free_amount = reserve_parameters.amount - amount_to_pay_for;

        // TODO: use full formula (with fee and etc)
        let leverage = self.get_leverage(
            reserve_parameters.exchange_account_id,
            reserve_parameters.symbol.currency_pair(),
        );

        (
            amount_to_pay_for * reserve_parameters.symbol.amount_multiplier / leverage,
            taken_free_amount,
        )
    }

    pub fn try_update_reservation_price(
        &mut self,
        reservation_id: ReservationId,
        new_price: Price,
    ) -> bool {
        let reservation = match self.get_reservation(reservation_id) {
            Some(reservation) => reservation,
            None => {
                log::error!(
                    "Can't find reservation {reservation_id} in {}",
                    self.balance_reservation_storage
                        .get_reservation_ids()
                        .iter()
                        .join(", ")
                );
                return false;
            }
        };

        let approved_sum: Decimal = reservation
            .approved_parts
            .iter()
            .filter(|(_, approved_part)| approved_part.is_canceled)
            .map(|(_, approved_part)| approved_part.unreserved_amount)
            .sum();

        let new_raw_rest_amount = reservation.amount - approved_sum;
        let new_rest_amount_in_reservation_currency =
            reservation.symbol.convert_amount_from_amount_currency_code(
                reservation.reservation_currency_code,
                new_raw_rest_amount,
                new_price,
            );
        let not_approved_amount_in_reservation_currency =
            reservation.convert_in_reservation_currency(reservation.not_approved_amount);

        let reservation_amount_diff_in_reservation_currency =
            new_rest_amount_in_reservation_currency - not_approved_amount_in_reservation_currency;

        let old_balance = self
            .try_get_available_balance(
                reservation.configuration_descriptor,
                reservation.exchange_account_id,
                reservation.symbol.clone(),
                reservation.order_side,
                new_price,
                true,
                false,
                &mut None,
            )
            .with_expect(|| {
                format!(
                    "failed to get available balance from {:?} for {}",
                    reservation, new_price
                )
            });

        let new_balance = old_balance - reservation_amount_diff_in_reservation_currency;
        if new_balance < dec!(0) {
            log::info!(
                "Failed to update reservation {} {} {} {:?} {} {} {} {} {}",
                reservation_id,
                reservation.exchange_account_id,
                reservation.reservation_currency_code,
                reservation.order_side,
                reservation.price,
                new_price,
                reservation.amount,
                old_balance,
                new_balance
            );
            return false;
        }

        let balance_request = BalanceRequest::from_reservation(reservation);

        let reservation = self.get_mut_reservation_expected(reservation_id);
        reservation.price = new_price;

        let reservation_amount_diff = reservation.symbol.convert_amount_into_amount_currency_code(
            reservation.reservation_currency_code,
            reservation_amount_diff_in_reservation_currency,
            reservation.price,
        );

        reservation.unreserved_amount -= reservation_amount_diff; // it will be compensated later

        self.add_reserved_amount(
            &balance_request,
            reservation_id,
            reservation_amount_diff,
            true,
        )
        .with_expect(|| {
            format!(
                "failed to reserve amount for {:?} {} {}",
                balance_request, reservation_id, reservation_amount_diff,
            )
        });

        let reservation = self.get_mut_reservation_expected(reservation_id);
        reservation.not_approved_amount = new_raw_rest_amount;

        log::info!(
            "Updated reservation {} {} {} {:?} {} {} {} {} {}",
            reservation_id,
            reservation.exchange_account_id,
            reservation.reservation_currency_code,
            reservation.order_side,
            reservation.price,
            new_price,
            reservation.amount,
            old_balance,
            new_balance
        );
        true
    }

    pub fn can_reserve(
        &self,
        reserve_parameters: &ReserveParameters,
        explanation: &mut Option<Explanation>,
    ) -> bool {
        self.can_reserve_core(reserve_parameters, explanation)
            .can_reserve
    }

    pub fn get_available_leveraged_balance(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        side: OrderSide,
        price: Price,
        explanation: &mut Option<Explanation>,
    ) -> Option<Amount> {
        self.try_get_available_balance(
            configuration_descriptor,
            exchange_account_id,
            symbol,
            side,
            price,
            true,
            true,
            explanation,
        )
    }

    pub fn set_target_amount_limit(
        &mut self,
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        limit: Amount,
    ) {
        for currency_code in [symbol.base_currency_code, symbol.quote_currency_code()] {
            let request = BalanceRequest::new(
                configuration_descriptor,
                exchange_account_id,
                symbol.currency_pair(),
                currency_code,
            );
            self.amount_limits_in_amount_currency
                .set_by_balance_request(&request, limit);
        }
    }
}
