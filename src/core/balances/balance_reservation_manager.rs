use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use chrono::Utc;
use itertools::Itertools;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::core::balance_manager::approved_part::ApprovedPart;
use crate::core::balance_manager::balance_position_by_fill_amount::BalancePositionByFillAmount;
use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::balance_manager::balance_reservation::BalanceReservation;
use crate::core::balance_manager::balances::Balances;
use crate::core::balance_manager::position_change::PositionChange;
use crate::core::balances::balance_position_model::BalancePositionModel;
use crate::core::balances::{
    balance_reservation_storage::BalanceReservationStorage,
    virtual_balance_holder::VirtualBalanceHolder,
};
use crate::core::exchanges::common::{
    Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, TradePlaceAccount,
};
use crate::core::exchanges::general::currency_pair_metadata::{BeforeAfter, CurrencyPairMetadata};
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::explanation::Explanation;
use crate::core::misc::reserve_parameters::ReserveParameters;
use crate::core::misc::service_value_tree::ServiceValueTree;
use crate::core::orders::order::ReservationId;
use crate::core::orders::order::{ClientOrderFillId, ClientOrderId, OrderSide};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::core::DateTime;

use super::balance_reservation_preset::BalanceReservationPreset;

#[derive(Clone)]
pub(crate) struct BalanceReservationManager {
    exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,

    // private readonly ICurrencyPairToSymbolConverter _currencyPairToSymbolConverter;
    // private readonly IDateTimeService _dateTimeService;
    // private readonly ILogger _logger = Log.ForContext<BalanceReservationManager>();
    reserved_amount_in_amount_currency: ServiceValueTree,
    amount_limits_in_amount_currency: ServiceValueTree,

    position_by_fill_amount_in_amount_currency: BalancePositionByFillAmount,
    reservation_id: ReservationId, // Utils.GetCurrentMiliseconds();

    pub virtual_balance_holder: VirtualBalanceHolder,
    pub balance_reservation_storage: BalanceReservationStorage,

    pub(crate) is_call_from_clone: bool,
}

impl BalanceReservationManager {
    pub fn update_reserved_balances(
        &mut self,
        reserved_balances_by_id: &HashMap<ReservationId, BalanceReservation>,
    ) {
        self.balance_reservation_storage.clear();
        for (reservation_id, reservation) in reserved_balances_by_id {
            self.balance_reservation_storage
                .add(reservation_id.clone(), reservation.clone());
        }
        self.sync_reservation_amounts();
    }

    pub fn sync_reservation_amounts(&mut self) {
        let reservations = self
            .balance_reservation_storage
            .get_all_raw_reservations()
            .values()
            .collect_vec();

        let mut reserved_by_request: HashMap<BalanceRequest, Decimal> = HashMap::new();
        for reservation in reservations {
            let balance_request = BalanceRequest::new(
                reservation.configuration_descriptor.clone(),
                reservation.exchange_account_id.clone(),
                reservation.currency_pair_metadata.currency_pair(),
                reservation
                    .currency_pair_metadata
                    .get_trade_code(reservation.order_side, BeforeAfter::Before),
            );
            if let Some(grouped_reservations) = reserved_by_request.get_mut(&balance_request) {
                *grouped_reservations += reservation.unreserved_amount;
            } else {
                reserved_by_request.insert(balance_request, reservation.unreserved_amount);
            }
        }

        let mut svt = ServiceValueTree::new();
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

    pub fn get_reservation(&self, reservation_id: &ReservationId) -> Option<&BalanceReservation> {
        self.balance_reservation_storage.try_get(reservation_id)
    }

    pub fn get_mut_reservation(
        &mut self,
        reservation_id: &ReservationId,
    ) -> Option<&mut BalanceReservation> {
        self.balance_reservation_storage.try_get_mut(reservation_id)
    }

    pub fn unreserve(
        &mut self,
        reservation_id: ReservationId,
        amount: Decimal,
        client_or_order_id: &Option<ClientOrderId>,
    ) -> Result<()> {
        let reservation = match self.get_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                let reservation_ids = self.balance_reservation_storage.get_reservation_ids();
                if self.is_call_from_clone || amount == dec!(0) {
                    log::error!(
                        "Can't find reservation {} ({}) for UnReserve {} in list: {:?}",
                        reservation_id,
                        self.is_call_from_clone,
                        amount,
                        reservation_ids
                    );
                    return Ok(());
                }

                bail!(
                    "Can't find reservation_id={} for UnReserve({}) attempt in list: {:?}",
                    reservation_id,
                    amount,
                    reservation_ids
                )
            }
        };

        let amount_to_unreserve = match reservation
            .currency_pair_metadata
            .round_to_remove_amount_precision_error(amount)
        {
            Ok(amount_to_unreserve) => amount_to_unreserve,
            Err(error) => bail!("Can't get amount_to_unreserve: {:?}", error),
        };

        if amount_to_unreserve == dec!(0) && reservation.amount != dec!(0) {
            // to prevent error logging in case when amount == 0
            if amount != amount_to_unreserve {
                log::info!("UnReserveInner {} != {}", amount, amount_to_unreserve);
            }
            return Ok(());
        }

        if !self
            .exchanges_by_id
            .contains_key(&reservation.exchange_account_id)
        {
            log::error!(
                "Trying to UnReserve for not existing exchange {}",
                reservation.exchange_account_id
            );
            return Ok(());
        }

        let balance_params = ReserveParameters::new(
            reservation.configuration_descriptor.clone(),
            reservation.exchange_account_id.clone(),
            reservation.currency_pair_metadata.clone(),
            reservation.order_side,
            reservation.price,
            dec!(0),
        );

        let old_balance = self.get_available_balance(&balance_params, true, &mut None);

        log::info!("VirtualBalanceHolder {}", old_balance);

        match self.unreserve_not_approved_part(
            reservation_id,
            client_or_order_id,
            amount_to_unreserve,
        ) {
            Ok(_) => (),
            Err(error) => bail!("failed unreserve not approved part: {:?}", error),
        };

        let reservation = self.easy_get_reservation(reservation_id)?;

        let balance_request = BalanceRequest::new(
            reservation.configuration_descriptor.clone(),
            reservation.exchange_account_id.clone(),
            reservation.currency_pair_metadata.currency_pair(),
            reservation.reservation_currency_code.clone(),
        );

        self.add_reserved_amount(&balance_request, reservation_id, -amount_to_unreserve, true)?;
        let new_balance = self.get_available_balance(&balance_params, true, &mut None);

        log::info!("VirtualBalanceHolder {}", new_balance);

        let reservation = self.easy_get_reservation(reservation_id)?;
        if reservation.unreserved_amount < dec!(0)
            || reservation.is_amount_within_symbol_margin_error(reservation.unreserved_amount)
        {
            self.balance_reservation_storage.remove(reservation_id);
            let reservation = self.easy_get_reservation(reservation_id)?;

            if self.is_call_from_clone {
                log::info!(
                    "Removed balance reservation {} on {}",
                    reservation_id,
                    reservation.exchange_account_id
                );
            }

            if reservation.unreserved_amount != dec!(0) {
                log::error!(
                    "AmountLeft {} != 0 for {} {} {} {:?}",
                    reservation.unreserved_amount,
                    reservation_id,
                    //     reservation.currency_pair_metadata,get_amount_tick() TODO: grays uncomment me after implemented
                    old_balance,
                    new_balance,
                    reservation
                );

                let amount_diff_in_amount_currency = -reservation.unreserved_amount.clone();
                // Compensate amount
                self.add_reserved_amount(
                    &balance_request,
                    reservation_id,
                    amount_diff_in_amount_currency,
                    true,
                )?;
            }

            if self.is_call_from_clone {
                let reservation = self.easy_get_reservation(reservation_id)?;
                log::info!(
                    "Unreserved {} from {} {} {} {} {} {} {} {} {} {} {}",
                    amount_to_unreserve,
                    reservation_id,
                    reservation.exchange_account_id,
                    reservation.reservation_currency_code,
                    reservation.order_side,
                    reservation.price,
                    reservation.amount,
                    reservation.not_approved_amount,
                    reservation.unreserved_amount,
                    client_or_order_id
                        .clone()
                        .unwrap_or(ClientOrderId::new("<order id is not set>".into())),
                    old_balance,
                    new_balance
                );
            }
        }
        Ok(())
    }

    fn get_available_balance(
        &self,
        parameters: &ReserveParameters,
        include_free_amount: bool,
        explanation: &mut Option<Explanation>,
    ) -> Decimal {
        self.try_get_available_balance(
            &parameters.configuration_descriptor,
            &parameters.exchange_account_id,
            parameters.currency_pair_metadata.clone(),
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
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        currency_code: &CurrencyCode,
        price: Decimal,
    ) -> Option<Decimal> {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            if &currency_pair_metadata.get_trade_code(side, BeforeAfter::Before) == currency_code {
                return self.try_get_available_balance(
                    configuration_descriptor,
                    exchange_account_id,
                    currency_pair_metadata,
                    side,
                    price,
                    true,
                    false,
                    &mut None,
                );
            }
        }

        let request = BalanceRequest::new(
            configuration_descriptor.clone(),
            exchange_account_id.clone(),
            currency_pair_metadata.currency_pair(),
            currency_code.clone(),
        );
        self.virtual_balance_holder.get_virtual_balance(
            &request,
            currency_pair_metadata,
            Some(price),
            &mut None,
        )
    }

    pub fn try_get_available_balance(
        &self,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        trade_side: OrderSide,
        price: Decimal,
        include_free_amount: bool,
        is_leveraged: bool,
        explanation: &mut Option<Explanation>,
    ) -> Option<Decimal> {
        let currency_code = currency_pair_metadata.get_trade_code(trade_side, BeforeAfter::Before);
        let request = BalanceRequest::new(
            configuration_descriptor.clone(),
            exchange_account_id.clone(),
            currency_pair_metadata.currency_pair(),
            currency_code.clone(),
        );
        let mut balance_in_currency_code = self.virtual_balance_holder.get_virtual_balance(
            &request,
            currency_pair_metadata.clone(),
            Some(price),
            explanation,
        )?;

        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "balance_in_currency_code_raw = {}",
                balance_in_currency_code
            ));
        }

        let leverage =
            self.try_get_leverage(exchange_account_id, &currency_pair_metadata.currency_pair())?;

        if let Some(explanation) = explanation {
            explanation.add_reason(format!("leverage = {}", leverage));
        }

        if currency_pair_metadata.is_derivative {
            if include_free_amount {
                let free_amount_in_amount_currency_code = self
                    .get_unreserved_position_in_amount_currency_code(
                        exchange_account_id,
                        currency_pair_metadata.clone(),
                        trade_side,
                    );

                if let Some(explanation) = explanation {
                    explanation.add_reason(format!(
                        "free_amount_in_amount_currency_code with leverage and amount_multiplier = {}",
                        free_amount_in_amount_currency_code
                    ));
                }

                let mut free_amount_in_currency_code = match currency_pair_metadata
                    .convert_amount_from_amount_currency_code(
                        &currency_code,
                        free_amount_in_amount_currency_code,
                        price,
                    ) {
                    Ok(free_amount_in_currency_code) => free_amount_in_currency_code,
                    Err(error) => {
                        log::error!(
                            "failed to convert amount from amount currnecy code: {:?}",
                            error
                        );
                        return None;
                    }
                };
                free_amount_in_currency_code /= leverage;
                free_amount_in_currency_code *= currency_pair_metadata.amount_multiplier;

                if let Some(explanation) = explanation {
                    explanation.add_reason(format!(
                        "free_amount_in_currency_code = {}",
                        free_amount_in_currency_code
                    ));
                }

                balance_in_currency_code += free_amount_in_currency_code;

                if let Some(explanation) = explanation {
                    explanation.add_reason(format!(
                        "balance_in_currency_code with free amount: {}",
                        balance_in_currency_code
                    ));
                }
            }

            balance_in_currency_code -= BalanceReservationManager::get_untouchable_amount(
                currency_pair_metadata.clone(),
                balance_in_currency_code,
            );
            if let Some(explanation) = explanation {
                explanation.add_reason(format!(
                    "balance_in_currency_code without untouchable: {}",
                    balance_in_currency_code
                ));
            }
        }

        if self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request)
            .is_none()
        {
            balance_in_currency_code = self.get_balance_with_applied_limits(
                &request,
                currency_pair_metadata.clone(),
                trade_side,
                balance_in_currency_code,
                price,
                leverage,
                explanation,
            )?;
        }

        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "balance_in_currency_code with limit: {}",
                balance_in_currency_code
            ));
        }

        // isLeveraged is used when we need to know how much funds we can use for orders
        if is_leveraged {
            balance_in_currency_code *= leverage;
            balance_in_currency_code /= currency_pair_metadata.amount_multiplier;

            if let Some(explanation) = explanation {
                explanation.add_reason(format!(
                    "balance_in_currency_code with leverage and multiplier: {}",
                    balance_in_currency_code
                ));
            }
        }
        Some(balance_in_currency_code)
    }

    pub fn get_position_in_amount_currency_code(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        trade_side: OrderSide,
    ) -> Decimal {
        if currency_pair_metadata.is_derivative {
            return dec!(0);
        }

        if let Some(current_position) = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, &currency_pair_metadata.currency_pair())
        {
            match trade_side {
                OrderSide::Buy => return std::cmp::max(dec!(0), -current_position),
                OrderSide::Sell => return std::cmp::max(dec!(0), current_position),
            }
        }
        dec!(0) // TODO: delete me
    }

    fn get_unreserved_position_in_amount_currency_code(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        trade_side: OrderSide,
    ) -> Decimal {
        let position = self.get_position_in_amount_currency_code(
            exchange_account_id,
            currency_pair_metadata,
            trade_side,
        );

        let reservation = self.balance_reservation_storage.get_all_raw_reservations();

        let taken_amount = reservation
            .iter()
            .map(|(_, balance_reservation)| {
                if balance_reservation.order_side == trade_side {
                    return balance_reservation.taken_free_amount;
                }
                dec!(0)
            })
            .sum::<Decimal>();

        std::cmp::max(dec!(0), position - taken_amount)
    }

    fn get_balance_with_applied_limits(
        &self,
        request: &BalanceRequest,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        trade_side: OrderSide,
        mut balance_in_currency_code: Decimal,
        price: Decimal,
        leverage: Decimal,
        explanation: &mut Option<Explanation>,
    ) -> Option<Decimal> {
        let position = self.get_position_values(
            &request.configuration_descriptor,
            &request.exchange_account_id,
            currency_pair_metadata.clone(),
            trade_side,
        )?;

        let position_amount_in_amount_currency = position.position;
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "position_amount_in_amount_currency: {}",
                position_amount_in_amount_currency
            ));
        }

        let reserved_amount_in_amount_currency = self
            .reserved_amount_in_amount_currency
            .get_by_balance_request(request)
            .unwrap_or(dec!(0));
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "reserved_amount_in_amount_currency: {}",
                reserved_amount_in_amount_currency
            ));
        }

        let reservation_with_fills_in_amount_currency =
            reserved_amount_in_amount_currency + position_amount_in_amount_currency;
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "reservation_with_fills_in_amount_currency: {}",
                reservation_with_fills_in_amount_currency
            ));
        }

        let total_amount_limit_in_amount_currency = position.limit.unwrap_or(dec!(0));
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "total_amount_limit_in_amount_currency: {}",
                total_amount_limit_in_amount_currency
            ));
        }

        let limit_left_in_amount_currency =
            total_amount_limit_in_amount_currency - reservation_with_fills_in_amount_currency;
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "limit_left_in_amount_currency: {}",
                limit_left_in_amount_currency
            ));
        }

        //AmountLimit is applied to full amount
        balance_in_currency_code *= leverage;
        balance_in_currency_code /= currency_pair_metadata.amount_multiplier;
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "balance_in_currency_code with leverage and multiplier: {}",
                balance_in_currency_code
            ));
        }

        let balance_in_amount_currency = match currency_pair_metadata
            .convert_amount_into_amount_currency_code(
                &request.currency_code,
                balance_in_currency_code,
                price,
            ) {
            Ok(balance_in_amount_currency) => balance_in_amount_currency,
            Err(error) => {
                log::error!(
                    "failed to convert amount into amount currnecy code: {:?}",
                    error
                );
                return None;
            }
        };
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "balance_in_amount_currency with leverage and multiplier: {}",
                balance_in_amount_currency
            ));
        }

        let limited_balance_in_amount_currency =
            std::cmp::min(balance_in_amount_currency, limit_left_in_amount_currency);
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "limited_balance_in_amount_currency: {}",
                limited_balance_in_amount_currency
            ));
        }

        let mut limited_balance_in_currency_code = match currency_pair_metadata
            .convert_amount_from_amount_currency_code(
                &request.currency_code,
                limited_balance_in_amount_currency,
                price,
            ) {
            Ok(balance_in_amount_currency) => balance_in_amount_currency,
            Err(error) => {
                log::error!(
                    "failed to convert amount from amount currnecy code: {:?}",
                    error
                );
                return None;
            }
        };
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "limited_balance_in_currency_code: {}",
                limited_balance_in_currency_code
            ));
        }

        //converting back to pure balance
        limited_balance_in_currency_code /= leverage;
        limited_balance_in_currency_code *= currency_pair_metadata.amount_multiplier;
        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "limited_balance_in_currency_code without leverage and multiplier: {}",
                limited_balance_in_currency_code
            ));
        }

        if limited_balance_in_currency_code < dec!(0) {
            log::warn!(
                "Balance {} < 0 ({} - ({} + {}) {} for {:?} {:?}",
                limited_balance_in_currency_code,
                total_amount_limit_in_amount_currency,
                reserved_amount_in_amount_currency,
                position_amount_in_amount_currency,
                balance_in_amount_currency,
                request,
                currency_pair_metadata
            );
        }; // TODO: grays fixe me  {@request} {@symbol}

        Some(std::cmp::max(dec!(0), limited_balance_in_currency_code))
    }

    fn get_untouchable_amount(
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        amount: Decimal,
    ) -> Decimal {
        if currency_pair_metadata.is_derivative {
            return amount * dec!(0.05);
        }
        return dec!(0);
    }

    fn try_get_leverage(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
    ) -> Option<Decimal> {
        let exchange = self.exchanges_by_id.get(exchange_account_id)?;
        exchange
            .leverage_by_currency_pair
            .get(currency_pair)
            .as_deref()
            .cloned()
    }

    fn get_position_values(
        &self,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        trade_side: OrderSide,
    ) -> Option<BalancePositionModel> {
        let currency_code = currency_pair_metadata.get_trade_code(trade_side, BeforeAfter::Before);
        let request = BalanceRequest::new(
            configuration_descriptor.clone(),
            exchange_account_id.clone(),
            currency_pair_metadata.currency_pair(),
            currency_code,
        );
        let total_amount_limit_in_amount_currency = self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request);

        let position = self.get_position(
            exchange_account_id,
            &currency_pair_metadata.currency_pair(),
            trade_side,
        )?;

        Some(BalancePositionModel {
            position,
            limit: total_amount_limit_in_amount_currency,
        })
    }

    pub fn get_position(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        trade_side: OrderSide,
    ) -> Option<Decimal> {
        let exchange = self.exchanges_by_id.get(exchange_account_id)?;

        let currency_pair_metadata = match exchange.get_currency_pair_metadata(currency_pair) {
            Ok(currency_pair_metadata) => currency_pair_metadata,
            Err(error) => {
                log::error!(
                    "failed to get_currency_pair_metadata from exchange with account id {:?} for currency pair {}: {:?}",
                    exchange.exchange_account_id,
                    currency_pair,
                    error
                );
                return None;
            }
        };

        let currency_code = currency_pair_metadata.get_trade_code(trade_side, BeforeAfter::Before);
        let mut position_in_amount_currency = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, currency_pair)?;

        if currency_code == currency_pair_metadata.base_currency_code {
            //sell
            position_in_amount_currency *= dec!(-1);
        }

        Some(position_in_amount_currency)
    }

    fn easy_get_mut_reservation(
        &mut self,
        reservation_id: ReservationId,
    ) -> Result<&mut BalanceReservation> {
        let res = match self.get_mut_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                bail!("Can't find reservation_id = {}", reservation_id,)
            }
        };
        Ok(res)
    }

    fn easy_get_reservation(&self, reservation_id: ReservationId) -> Result<&BalanceReservation> {
        let res = match self.get_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                bail!("Can't find reservation_id = {}", reservation_id,)
            }
        };
        Ok(res)
    }

    fn unreserve_not_approved_part(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &Option<ClientOrderId>,
        amount_to_unreserve: Decimal,
    ) -> Result<()> {
        let reservation = self.easy_get_mut_reservation(reservation_id)?;
        if let Some(client_order_id) = client_order_id {
            if let Some(approved_part) = reservation.approved_parts.get_mut(client_order_id) {
                let new_unreserved_amount_for_approved_part =
                    approved_part.unreserved_amount - amount_to_unreserve;
                if new_unreserved_amount_for_approved_part < dec!(0) {
                    bail!(
                        "Attempt to UnReserve more than was approved for order {} ({}): {} > {}",
                        client_order_id,
                        reservation_id,
                        amount_to_unreserve,
                        approved_part.unreserved_amount
                    )
                }
                approved_part.unreserved_amount = new_unreserved_amount_for_approved_part
            } else {
                log::warn!("UnReserve({}, {}) called with clientOrderId {} for reservation without the approved part {:?}",
                    reservation_id, amount_to_unreserve, client_order_id, reservation);
                reservation.not_approved_amount -= amount_to_unreserve;
                if reservation.not_approved_amount < dec!(0) {
                    log::error!("NotApprovedAmount for {} was unreserved for the missing order {} and now < 0 {:?}",
                        reservation_id, client_order_id, reservation);
                }
            }
        } else {
            reservation.not_approved_amount -= amount_to_unreserve;
            // this case will be handled by UnReserve itself
            if reservation.not_approved_amount < dec!(0)
                && reservation.unreserved_amount > amount_to_unreserve
            {
                bail!(
                    "Possibly UnReserve {} should be called with clientOrderId parameter",
                    reservation_id
                )
            }
        }
        Ok(())
    }

    fn add_reserved_amount(
        &mut self,
        request: &BalanceRequest,
        reservation_id: ReservationId,
        amount_diff_in_amount_currency: Decimal,
        update_balance: bool,
    ) -> Result<()> {
        let reservation = self.easy_get_mut_reservation(reservation_id)?;
        if update_balance {
            let cost =
                match reservation.get_proportional_cost_amount(amount_diff_in_amount_currency) {
                    Ok(cost) => cost,
                    Err(error) => {
                        bail!(
                            "Failed to get proportional cost amount form {:?} with {}: {:?}",
                            reservation,
                            amount_diff_in_amount_currency,
                            error
                        )
                    }
                };
            self.add_virtual_balance(request, reservation_id, -cost)?;
        }

        let reservation = self.easy_get_mut_reservation(reservation_id)?;
        reservation.unreserved_amount += amount_diff_in_amount_currency;

        // global reservation indicator
        let res_amount_request = BalanceRequest::new(
            request.configuration_descriptor.clone(),
            request.exchange_account_id.clone(),
            request.currency_pair.clone(),
            reservation.reservation_currency_code.clone(),
        );

        self.reserved_amount_in_amount_currency
            .add_by_request(&res_amount_request, amount_diff_in_amount_currency);
        Ok(())
    }

    fn add_virtual_balance(
        &mut self,
        request: &BalanceRequest,
        reservation_id: ReservationId,
        diff_in_amount_currency: Decimal,
    ) -> Result<()> {
        let reservation = self.easy_get_reservation(reservation_id)?;
        // this is https://github.com/rust-lang/rust/issues/59159 explanation of these two variables
        let currency_pair_metadata = reservation.currency_pair_metadata.clone();
        let price = reservation.price;
        self.add_virtual_balance_by_currency_pair_metadata(
            request,
            currency_pair_metadata,
            diff_in_amount_currency,
            price,
        )
    }

    fn add_virtual_balance_by_currency_pair_metadata(
        &mut self,
        request: &BalanceRequest,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        diff_in_amount_currency: Decimal,
        price: Decimal,
    ) -> Result<()> {
        if currency_pair_metadata.is_derivative {
            let diff_in_request_currency = currency_pair_metadata
                .convert_amount_from_amount_currency_code(
                    &request.currency_code,
                    diff_in_amount_currency,
                    price,
                )?;
            self.virtual_balance_holder
                .add_balance(request, diff_in_request_currency);
        } else {
            let balance_currency_code_request = match &currency_pair_metadata.balance_currency_code
            {
                Some(balance_currency_code) => BalanceRequest::new(
                    request.configuration_descriptor.clone(),
                    request.exchange_account_id.clone(),
                    request.currency_pair.clone(),
                    balance_currency_code.clone(),
                ),
                None => {
                    bail!("currency_pair_metadata.balance_currency_code should be non None")
                }
            };
            let diff_in_balance_currency_code = currency_pair_metadata
                .convert_amount_from_amount_currency_code(
                    &balance_currency_code_request.currency_code,
                    diff_in_amount_currency,
                    price,
                )?;
            self.virtual_balance_holder.add_balance(
                &balance_currency_code_request,
                diff_in_balance_currency_code,
            );
        }
        Ok(())
    }

    pub fn get_state(&self) -> Balances {
        Balances::new(
            self.virtual_balance_holder
                .get_raw_exchange_balances()
                .clone(),
            self.virtual_balance_holder
                .get_virtual_balance_diffs()
                .clone(),
            self.reserved_amount_in_amount_currency.clone(),
            self.position_by_fill_amount_in_amount_currency.clone(),
            self.amount_limits_in_amount_currency.clone(),
            self.balance_reservation_storage
                .get_all_raw_reservations()
                .clone(),
            None,
        )
    }

    pub(crate) fn restore_fill_amount_position(
        &mut self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        new_position: Decimal,
    ) -> Result<()> {
        if currency_pair_metadata.is_derivative {
            bail!("restore_fill_amount_position is available only for derivative exchanges")
        }
        let previous_value = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, &currency_pair_metadata.currency_pair());

        // let now = self.date_time_service.utc_now
        let now = Utc::now(); // TODO: fix me after adding date_time_service

        self.position_by_fill_amount_in_amount_currency.set(
            exchange_account_id,
            &currency_pair_metadata.currency_pair(),
            previous_value,
            new_position,
            None,
            now,
        )?;
        Ok(())
    }

    pub fn get_last_position_change_before_period(
        &self,
        trade_place: &TradePlaceAccount,
        start_of_period: DateTime,
    ) -> Option<PositionChange> {
        self.position_by_fill_amount_in_amount_currency
            .get_last_position_change_before_period(trade_place, start_of_period)
    }

    pub fn get_fill_amount_position_percent(
        &self,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> Option<Decimal> {
        let position = self.get_position_values(
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata.clone(),
            side,
        )?;
        Some(std::cmp::min(
            dec!(1),
            std::cmp::max(dec!(0), position.position / position.limit?),
        ))
    }

    pub fn handle_position_fill_amount_change(
        &mut self,
        trade_side: OrderSide,
        client_order_fill_id: &Option<ClientOrderFillId>,
        fill_amount: Decimal,
        price: Decimal,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        currency_code: &mut CurrencyCode,
        change_amount_in_currency: &mut Amount,
    ) -> Result<()> {
        let request = BalanceRequest::new(
            configuration_descriptor.clone(),
            exchange_account_id.clone(),
            currency_pair_metadata.currency_pair(),
            currency_code.clone(),
        );

        if !currency_pair_metadata.is_derivative() {
            self.add_virtual_balance_by_currency_pair_metadata(
                &request,
                currency_pair_metadata.clone(),
                -fill_amount,
                price,
            )?;
            *change_amount_in_currency = currency_pair_metadata
                .convert_amount_from_amount_currency_code(currency_code, fill_amount, price)?;
        }
        if currency_pair_metadata.amount_currency_code == *currency_code {
            let mut position_change = fill_amount;
            if currency_pair_metadata.is_derivative {
                let free_amount = self.get_position_in_amount_currency_code(
                    exchange_account_id,
                    currency_pair_metadata.clone(),
                    trade_side,
                );
                let move_amount = fill_amount.abs();
                let (add_amount, sub_amount) = if free_amount - move_amount >= dec!(0) {
                    (move_amount, dec!(0))
                } else {
                    (free_amount, (free_amount - move_amount).abs())
                };

                let leverage = match self
                    .try_get_leverage(exchange_account_id, &currency_pair_metadata.currency_pair())
                {
                    Some(leverage) => leverage,
                    None => bail!(
                        "Failed to get leverage for {} from {:?}",
                        exchange_account_id,
                        currency_pair_metadata
                    ),
                };
                let diff_in_amount_currency =
                    (add_amount - sub_amount) / leverage * currency_pair_metadata.amount_multiplier;
                self.add_virtual_balance_by_currency_pair_metadata(
                    &request,
                    currency_pair_metadata.clone(),
                    diff_in_amount_currency,
                    price,
                )?;

                // reversed derivative
                if currency_pair_metadata.amount_currency_code
                    == currency_pair_metadata.base_currency_code()
                {
                    position_change *= dec!(-1);
                }
            }
            let now = Utc::now();
            self.position_by_fill_amount_in_amount_currency.add(
                &request.exchange_account_id,
                &request.currency_pair,
                position_change,
                client_order_fill_id.clone(),
                now,
            )?;
            self.validate_position_and_limits(&request);
        }
        Ok(())
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
            .get(&request.exchange_account_id, &request.currency_pair)
        {
            Some(position) => position,
            None => return,
        };

        if position.abs() > limit {
            log::error!(
                "Position > Limit: outstanding situation {} > {} ({:?})",
                position,
                limit,
                request
            );
        }
    }

    pub fn cancel_approved_reservation(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &ClientOrderId,
    ) -> Result<()> {
        let reservation = match self.get_mut_reservation(&reservation_id) {
            Some(reservation_id) => reservation_id,
            None => {
                log::error!(
                    "Can't find reservation {} in {:?}",
                    reservation_id,
                    self.balance_reservation_storage.get_reservation_ids()
                );
                return Ok(());
            }
        };

        let approved_part = match reservation.approved_parts.get_mut(client_order_id) {
            Some(approved_part) => approved_part,
            None => {
                log::error!("There is no approved part for order {}", client_order_id);
                return Ok(());
            }
        };

        if approved_part.is_canceled {
            bail!(
                "Approved part was already canceled for {} {}",
                client_order_id,
                reservation_id
            );
        }

        reservation.not_approved_amount += approved_part.unreserved_amount;
        approved_part.is_canceled = true;
        log::info!(
            "Canceled approved part for order {} with {}",
            client_order_id,
            approved_part.unreserved_amount
        );
        Ok(())
    }

    pub fn handle_position_fill_amount_change_commission(
        &mut self,
        commission_currency_code: CurrencyCode,
        commission_amount: Amount,
        converted_commission_currency_code: CurrencyCode,
        converted_commission_amount: Amount,
        price: Decimal,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
    ) {
        let leverage = self
            .try_get_leverage(exchange_account_id, &currency_pair_metadata.currency_pair())
            .expect(
                format!(
                    "failed to get leverage for {} and {}",
                    exchange_account_id,
                    currency_pair_metadata.currency_pair()
                )
                .as_str(),
            );
        if currency_pair_metadata.is_derivative
            || currency_pair_metadata.base_currency_code() == commission_currency_code
        {
            let request = BalanceRequest::new(
                configuration_descriptor.clone(),
                exchange_account_id.clone(),
                currency_pair_metadata.currency_pair(),
                commission_currency_code,
            );
            let res_commission_amount = commission_amount / leverage;
            self.virtual_balance_holder
                .add_balance(&request, -res_commission_amount);
        } else {
            let request = BalanceRequest::new(
                configuration_descriptor.clone(),
                exchange_account_id.clone(),
                currency_pair_metadata.currency_pair(),
                converted_commission_currency_code.clone(),
            );
            let commission_in_amount_currency = currency_pair_metadata
                .convert_amount_into_amount_currency_code(
                    &converted_commission_currency_code,
                    converted_commission_amount,
                    price,
                )
                .expect(
                    format!(
                        "failed to convert amount into amount currency code for {} {} {}",
                        converted_commission_currency_code, converted_commission_amount, price
                    )
                    .as_str(),
                );
            let res_commission_amount_in_amount_currency = commission_in_amount_currency / leverage;
            self.add_virtual_balance_by_currency_pair_metadata(
                &request,
                currency_pair_metadata.clone(),
                -res_commission_amount_in_amount_currency,
                price,
            )
            .expect(
                format!(
                    "failed to add virtual balance with {:?} {:?} {} {}",
                    request,
                    currency_pair_metadata,
                    -res_commission_amount_in_amount_currency,
                    price
                )
                .as_str(),
            );
        }
    }

    pub fn approve_reservation(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &ClientOrderId,
        amount: Amount,
    ) -> Result<()> {
        let reservation = match self.get_mut_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                log::error!(
                    "Can't find reservation {} in {:?}",
                    reservation_id,
                    self.balance_reservation_storage.get_reservation_ids()
                );
                return Ok(());
            }
        };

        if reservation.approved_parts.contains_key(client_order_id) {
            log::error!(
                "Order {} cannot be approved multiple times",
                client_order_id
            );
            return Ok(());
        }

        reservation.not_approved_amount -= amount;

        if reservation.not_approved_amount < dec!(0)
            && !reservation.is_amount_within_symbol_margin_error(reservation.not_approved_amount)
        {
            log::error!(
                "RestApprovedAmount < 0 for order {} {} {} {:?}",
                client_order_id,
                reservation_id,
                amount,
                reservation
            );
            bail!(
                "RestApprovedAmount < 0 for order {} {} {}",
                client_order_id,
                reservation_id,
                amount
            )
        }
        let date_time = Utc::now();
        match reservation.approved_parts.get_mut(client_order_id) {
            Some(approved_part) => {
                *approved_part = ApprovedPart::new(date_time, client_order_id.clone(), amount);
            }
            None => bail!(
                "failed to get approved part for {} from {:?}",
                client_order_id,
                reservation.approved_parts
            ),
        }

        log::info!("Order {} was approved with {}", client_order_id, amount);
        Ok(())
    }

    pub fn try_transfer_reservation(
        &mut self,
        src_reservation_id: ReservationId,
        dst_reservation_id: ReservationId,
        amount: Amount,
        client_order_id: &Option<ClientOrderId>,
    ) -> bool {
        let src_reservation = self
            .get_reservation(&src_reservation_id)
            .expect(format!("Reservation for {} not found", src_reservation_id).as_str());

        let dst_reservation = self
            .get_reservation(&dst_reservation_id)
            .expect(format!("Reservation for {} not found", dst_reservation_id).as_str());

        if src_reservation.configuration_descriptor == dst_reservation.configuration_descriptor
            || src_reservation.exchange_account_id != dst_reservation.exchange_account_id
            || src_reservation.currency_pair_metadata != dst_reservation.currency_pair_metadata
            || src_reservation.order_side != dst_reservation.order_side
        {
            std::panic!(
                "Reservations {:?} and {:?} are from different sources",
                src_reservation,
                dst_reservation
            );
        }

        let amount_to_move = src_reservation
            .currency_pair_metadata
            .round_to_remove_amount_precision_error(amount)
            .expect(
                format!(
                    "failed to round to remove amount precision error from {:?} for {}",
                    src_reservation.currency_pair_metadata, amount
                )
                .as_str(),
            );
        if amount_to_move == dec!(0) {
            log::warn!(
                "Can't transfer zero amount from {} to {}",
                src_reservation_id,
                dst_reservation_id
            );
            return false;
        }

        if src_reservation.price != dst_reservation.price {
            // special case for derivatives because balance for AmountCurrency is auto-calculated
            if src_reservation.currency_pair_metadata.is_derivative {
                // check if we have enough balance for the operation
                let add_amount = src_reservation
                    .convert_in_reservation_currency(amount_to_move)
                    .expect(
                        format!(
                            "src_reservation: failed to convert in reservation currency: {}",
                            amount_to_move
                        )
                        .as_str(),
                    );
                let sub_amount = dst_reservation
                    .convert_in_reservation_currency(amount_to_move)
                    .expect(
                        format!(
                            "dst_reservation: failed to convert in reservation currency: {}",
                            amount_to_move
                        )
                        .as_str(),
                    );

                let balance_diff_amount = add_amount - sub_amount;

                let available_balance = self
                    .try_get_available_balance(
                        &dst_reservation.configuration_descriptor,
                        &dst_reservation.exchange_account_id,
                        dst_reservation.currency_pair_metadata.clone(),
                        dst_reservation.order_side,
                        dst_reservation.price,
                        true,
                        false,
                        &mut None,
                    )
                    .expect(
                        format!("failed to get available balance for {:?}", dst_reservation)
                            .as_str(),
                    );
                if available_balance + balance_diff_amount < dec!(0) {
                    log::warn!(
                        "Can't transfer {} because there will be insufficient balance ({} => {})",
                        amount_to_move,
                        src_reservation_id,
                        dst_reservation_id
                    );
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
        true //delete me

        // we can safely move amount ignoring price because of check that have been done before
        // TransferAmount(
        //     sourceReservationId,
        //     sourceReservation,
        //     targetReservationId,
        //     targetReservation,
        //     amountToMove,
        //     clientOrderId);

        // return true;
    }

    fn transfer_amount(
        &mut self,
        src_reservation_id: ReservationId,
        dst_reservation_id: ReservationId,
        amount_to_move: Amount,
        client_order_id: &Option<ClientOrderId>,
    ) {
        let src_reservation = self
            .get_reservation(&src_reservation_id)
            .expect(format!("Reservation for {} not found", src_reservation_id).as_str());
        let new_src_unreserved_amount = src_reservation.unreserved_amount - amount_to_move;
        let src_cost_diff = &mut dec!(0);
        log::info!(
            "trying to update src unreserved amount for transfer: {:?} {} {:?}",
            src_reservation,
            new_src_unreserved_amount,
            client_order_id
        );
        self.update_unreserved_amount_for_transfer(
            src_reservation_id,
            new_src_unreserved_amount,
            client_order_id,
            true,
            dec!(0),
            src_cost_diff,
        )
        .expect("failed to update src unreserved amount");

        let dst_reservation = self
            .get_reservation(&dst_reservation_id)
            .expect(format!("Reservation for {} not found", dst_reservation_id).as_str());
        let new_dst_unreserved_amount = dst_reservation.unreserved_amount + amount_to_move;
        log::info!(
            "trying to update dst unreserved amount for transfer: {:?} {} {:?}",
            dst_reservation,
            new_dst_unreserved_amount,
            client_order_id
        );
        self.update_unreserved_amount_for_transfer(
            dst_reservation_id,
            new_dst_unreserved_amount,
            client_order_id,
            false,
            -*src_cost_diff,
            &mut dec!(0),
        )
        .expect("failed to update dst unreserved amount");

        log::info!(
            "Successfully transferred {} from {} to {}",
            amount_to_move,
            src_reservation_id,
            dst_reservation_id
        );
    }

    fn update_unreserved_amount_for_transfer(
        &mut self,
        reservation_id: ReservationId,
        new_unreserved_amount: Amount,
        client_order_id: &Option<ClientOrderId>,
        is_src_request: bool,
        target_cost_diff: Decimal,
        cost_diff: &mut Decimal,
    ) -> Result<()> {
        let reservation = self.easy_get_mut_reservation(reservation_id)?;
        *cost_diff = dec!(0);
        // we should check the case when we have insignificant calculation errors
        if new_unreserved_amount < dec!(0)
            && !reservation.is_amount_within_symbol_margin_error(new_unreserved_amount)
        {
            bail!(
                "Can't set {} amount to reservation {}",
                new_unreserved_amount,
                reservation_id
            )
        }

        let reservation_amount_diff = new_unreserved_amount - reservation.unreserved_amount;
        if let Some(client_order_id) = client_order_id {
            if !reservation
                .approved_parts
                .get_mut(client_order_id)
                .is_none()
            {
                let new_amount = reservation
                    .approved_parts
                    .get_mut(client_order_id)
                    .expect("fix me") // TODO: grays fix me and next
                    .unreserved_amount
                    + reservation_amount_diff;
                if reservation.is_amount_within_symbol_margin_error(new_amount) {
                    reservation.approved_parts.remove(client_order_id);
                } else if new_amount < dec!(0) {
                    bail!(
                            "Attempt to transfer more amount ({}) than we have ({}) for approved part by ClientOrderId {}",
                            reservation_amount_diff,
                            reservation
                                .approved_parts
                                .get_mut(client_order_id)
                                .expect("fix me").unreserved_amount,
                            client_order_id
                        )
                } else {
                    reservation
                        .approved_parts
                        .get_mut(client_order_id)
                        .expect("fix me")
                        .unreserved_amount = new_amount; // TODO: grays fix me
                    reservation
                        .approved_parts
                        .get_mut(client_order_id)
                        .expect("fix me")
                        .amount += reservation_amount_diff;
                }
            } else {
                if is_src_request {
                    bail!(
                        "Can't find approved part {} for {}",
                        client_order_id,
                        reservation_id
                    )
                }

                match reservation.approved_parts.get_mut(client_order_id) {
                    Some(approved_part) => {
                        *approved_part = ApprovedPart::new(
                            Utc::now(),
                            client_order_id.clone(),
                            reservation_amount_diff,
                        );
                    }
                    None => bail!("failed to get mut approved part for {}", client_order_id),
                }
            }
        } else {
            reservation.not_approved_amount += reservation_amount_diff;
        }

        let balance_request = BalanceRequest::new(
            reservation.configuration_descriptor.clone(),
            reservation.exchange_account_id.clone(),
            reservation.currency_pair_metadata.currency_pair(),
            reservation.reservation_currency_code.clone(),
        );

        self.add_reserved_amount(
            &balance_request,
            reservation_id,
            reservation_amount_diff,
            false,
        )?;
        let reservation = self.easy_get_mut_reservation(reservation_id.clone())?;

        *cost_diff = if is_src_request {
            reservation.get_proportional_cost_amount(reservation_amount_diff)?
        } else {
            target_cost_diff
        };
        let buff_price = reservation.price;
        let buff_currency_pair_metadata = reservation.currency_pair_metadata.clone();
        self.add_virtual_balance_by_currency_pair_metadata(
            &balance_request,
            buff_currency_pair_metadata,
            -*cost_diff,
            buff_price,
        )?;
        let reservation = self.easy_get_mut_reservation(reservation_id.clone())?;

        reservation.cost += *cost_diff;
        reservation.amount += reservation_amount_diff;

        if reservation.is_amount_within_symbol_margin_error(new_unreserved_amount) {
            self.balance_reservation_storage
                .remove(reservation_id.clone());
            let reservation = self.easy_get_mut_reservation(reservation_id.clone())?;

            if new_unreserved_amount != dec!(0) {
                log::error!(
                    "Transfer: AmountLeft {} != 0 for {} {:?}",
                    reservation.unreserved_amount,
                    reservation_id,
                    reservation
                );
            }
        }
        let reservation = self.easy_get_mut_reservation(reservation_id.clone())?;
        log::info!(
            "Updated reservation {} {} {} {} {} {} {}",
            reservation_id,
            reservation.exchange_account_id,
            reservation.reservation_currency_code,
            reservation.order_side,
            reservation.price,
            reservation.amount,
            reservation_amount_diff
        );
        Ok(())
    }

    pub fn try_reserve_multiple(
        &mut self,
        reserve_parameters: &Vec<ReserveParameters>,
        explanation: &mut Option<Explanation>,
    ) -> (bool, Vec<ReservationId>) {
        let mut successful_reservations = HashMap::new();
        for reserve_parameter in reserve_parameters {
            let mut reservation_id = ReservationId::default();

            if self.try_reserve(reserve_parameter, &mut reservation_id, explanation) {
                successful_reservations.insert(reservation_id, reserve_parameter);
            }
        }

        if successful_reservations.len() != reserve_parameters.len() {
            for (res_id, res_params) in successful_reservations {
                self.unreserve(res_id, res_params.amount, &None).expect(
                    format!("failed to unreserve for {} {}", res_id, res_params.amount).as_str(),
                );
            }
            return (false, Vec::new());
        }
        return (true, successful_reservations.keys().cloned().collect_vec());
    }

    pub fn try_reserve(
        &mut self,
        reserve_parameters: &ReserveParameters,
        reservation_id: &mut ReservationId,
        explanation: &mut Option<Explanation>,
    ) -> bool {
        *reservation_id = ReservationId::default();

        let mut old_balance = Decimal::default();
        let mut new_balance = Decimal::default();
        let mut potential_position = Some(Decimal::default());
        let mut preset = BalanceReservationPreset::default();
        if !self.can_reserve_core(
            reserve_parameters,
            &mut old_balance,
            &mut new_balance,
            &mut potential_position,
            &mut preset,
            explanation,
        ) {
            log::info!(
                "Failed to reserve {} {} {:?} {} {} {:?}",
                preset.reservation_currency_code,
                preset.amount_in_reservation_currency_code,
                potential_position,
                old_balance,
                new_balance,
                reserve_parameters
            );
            return false;
        }

        let request = BalanceRequest::new(
            reserve_parameters.configuration_descriptor.clone(),
            reserve_parameters.exchange_account_id.clone(),
            reserve_parameters.currency_pair_metadata.currency_pair(),
            preset.reservation_currency_code.clone(),
        );
        let reservation = BalanceReservation::new(
            reserve_parameters.configuration_descriptor.clone(),
            reserve_parameters.exchange_account_id.clone(),
            reserve_parameters.currency_pair_metadata.clone(),
            reserve_parameters.order_side,
            reserve_parameters.price,
            reserve_parameters.amount,
            preset.taken_free_amount_in_amount_currency_code,
            preset.cost_in_amount_currency_code,
            preset.reservation_currency_code.clone(),
        );

        *reservation_id = self.reservation_id;
        log::info!(
            "Trying to reserve {:?} {} {} {:?} {} {} {:?}",
            self.reservation_id,
            preset.reservation_currency_code,
            preset.amount_in_reservation_currency_code,
            potential_position,
            old_balance,
            new_balance,
            reservation
        );
        self.balance_reservation_storage
            .add(self.reservation_id, reservation);
        self.add_reserved_amount(
            &request,
            self.reservation_id,
            reserve_parameters.amount,
            true,
        )
        .expect(
            format!(
                "failed to add reserved amount {:?} {} {}",
                request, self.reservation_id, reserve_parameters.amount,
            )
            .as_str(),
        );

        log::info!("Reserved successfully");
        true
    }

    fn can_reserve_core(
        &self,
        reserve_parameters: &ReserveParameters,
        old_balance: &mut Decimal,
        new_balance: &mut Decimal,
        potential_position: &mut Option<Decimal>,
        preset: &mut BalanceReservationPreset,
        explanation: &mut Option<Explanation>,
    ) -> bool {
        *preset = self.get_currency_code_and_reservation_amount(reserve_parameters, explanation);

        //We set includeFreeAmount to false because we already took FreeAmount into consideration while calculating the preset
        //Otherwise we would count FreeAmount twice which is wrong
        *old_balance = self.get_available_balance(reserve_parameters, false, explanation);

        let preset_cost = preset.cost_in_reservation_currency_code;

        *new_balance = *old_balance - preset_cost;

        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "oldBalance: {} presetCost: {} newBalance: {}",
                *old_balance, preset_cost, *new_balance
            ));
        }

        if !self.can_reserve_with_limit(reserve_parameters, potential_position) {
            return false;
        }

        //Added precision error handling for https://github.com/CryptoDreamTeam/CryptoLp/issues/1602
        //Spot trading might need a more precise solution
        reserve_parameters
            .currency_pair_metadata
            .round_to_remove_amount_precision_error(*new_balance)
            .expect(
                format!(
                    "failed to round to remove amount precision error from {:?} for {}",
                    reserve_parameters.currency_pair_metadata, *new_balance
                )
                .as_str(),
            )
            >= dec!(0)
    }

    fn can_reserve_with_limit(
        &self,
        reserve_parameters: &ReserveParameters,
        potential_position: &mut Option<Decimal>,
    ) -> bool {
        let reservation_currency_code = reserve_parameters
            .currency_pair_metadata
            .get_trade_code(reserve_parameters.order_side, BeforeAfter::Before);

        let request = BalanceRequest::new(
            reserve_parameters.configuration_descriptor.clone(),
            reserve_parameters.exchange_account_id.clone(),
            reserve_parameters.currency_pair_metadata.currency_pair(),
            reservation_currency_code,
        );

        let limit = match self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request)
        {
            Some(limit) => limit,
            None => {
                *potential_position = None;
                return true;
            }
        };

        let reserved_amount = self
            .reserved_amount_in_amount_currency
            .get_by_balance_request(&request)
            .unwrap_or(dec!(0));
        let new_reserved_amount = reserved_amount + reserve_parameters.amount;

        let position = self
            .position_by_fill_amount_in_amount_currency
            .get(&request.exchange_account_id, &request.currency_pair)
            .unwrap_or(dec!(0));
        *potential_position = if reserve_parameters.order_side == OrderSide::Buy {
            Some(position + new_reserved_amount)
        } else {
            Some(position - new_reserved_amount)
        };

        let potential_position_abs = potential_position.expect("Must be non None").abs();
        if potential_position_abs <= limit {
            // position is within limit range
            return true;
        }

        // we are out of limit range there, so it is okay if we are moving to the limit
        potential_position_abs < position.abs()
    }

    fn get_currency_code_and_reservation_amount(
        &self,
        reserve_parameters: &ReserveParameters,
        explanation: &mut Option<Explanation>,
    ) -> BalanceReservationPreset {
        let price = reserve_parameters.price;
        let amount = reserve_parameters.amount;
        let currency_pair_metadata = reserve_parameters.currency_pair_metadata.clone();

        let reservation_currency_code = currency_pair_metadata
            .get_trade_code(reserve_parameters.order_side, BeforeAfter::Before);

        let amount_in_reservation_currency_code = currency_pair_metadata
            .convert_amount_from_amount_currency_code(&reservation_currency_code, amount, price)
            .expect(
                format!(
                    "failed to conver amount from amount currency code {} {} {}",
                    reservation_currency_code, amount, price
                )
                .as_str(),
            );

        let (cost_in_amount_currency_code, taken_free_amount) =
            self.calculate_reservation_cost(reserve_parameters);
        let cost_in_reservation_currency_code = currency_pair_metadata
            .convert_amount_from_amount_currency_code(
                &reservation_currency_code,
                cost_in_amount_currency_code,
                price,
            )
            .expect(
                format!(
                    "failed to conver amount from amount currency code {} {} {}",
                    reservation_currency_code, cost_in_amount_currency_code, price
                )
                .as_str(),
            );

        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "costInReservationCurrencyCode: {} takenFreeAmount: {}",
                cost_in_reservation_currency_code, taken_free_amount
            ));
        }

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
        if reserve_parameters.currency_pair_metadata.is_derivative {
            return (reserve_parameters.amount, dec!(0));
        }

        let free_amount = self.get_unreserved_position_in_amount_currency_code(
            &reserve_parameters.exchange_account_id,
            reserve_parameters.currency_pair_metadata.clone(),
            reserve_parameters.order_side,
        );

        let amount_to_pay_for = std::cmp::max(dec!(0), reserve_parameters.amount - free_amount);

        let taken_free_amount = reserve_parameters.amount - amount_to_pay_for;

        // todo: use full formula (with fee and etc)
        let leverage = self
            .try_get_leverage(
                &reserve_parameters.exchange_account_id,
                &reserve_parameters.currency_pair_metadata.currency_pair(),
            )
            .expect("failed to get leverage");

        (
            amount_to_pay_for * reserve_parameters.currency_pair_metadata.amount_multiplier
                / leverage,
            taken_free_amount,
        )
    }

    pub fn try_update_reservation_price(
        &mut self,
        reservation_id: ReservationId,
        new_price: Decimal,
    ) -> bool {
        let reservation = match self.get_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                log::error!(
                    "Can't find reservation {} in {:?}",
                    reservation_id,
                    self.balance_reservation_storage.get_reservation_ids()
                );
                return false;
            }
        };

        let mut approved_sum = dec!(0);
        for (_, approved_part) in reservation.approved_parts.clone() {
            if !approved_part.is_canceled {
                approved_sum += approved_part.unreserved_amount;
            }
        }
        let new_raw_rest_amount = reservation.amount - approved_sum;
        let new_rest_amount_in_reservation_currency = reservation
            .currency_pair_metadata
            .convert_amount_from_amount_currency_code(
                &reservation.reservation_currency_code,
                new_raw_rest_amount,
                new_price,
            )
            .expect(
                format!(
                    "failed to convert amount from amount currency code {} {} {}",
                    reservation.reservation_currency_code, new_raw_rest_amount, new_price
                )
                .as_str(),
            );
        let not_approved_amount_in_reservation_currency = reservation
            .convert_in_reservation_currency(reservation.not_approved_amount)
            .expect(
                format!(
                    "failed to convert in reservation currency {}",
                    reservation.not_approved_amount
                )
                .as_str(),
            );

        let reservation_amount_diff_in_reservation_currency =
            new_rest_amount_in_reservation_currency - not_approved_amount_in_reservation_currency;

        let old_balance = self
            .try_get_available_balance(
                &reservation.configuration_descriptor,
                &reservation.exchange_account_id,
                reservation.currency_pair_metadata.clone(),
                reservation.order_side,
                new_price,
                true,
                false,
                &mut None,
            )
            .expect(
                format!(
                    "failed to get available balance from {:?} for {}",
                    reservation, new_price
                )
                .as_str(),
            );

        let new_balance = old_balance - reservation_amount_diff_in_reservation_currency;
        if new_balance < dec!(0) {
            log::info!(
                "Failed to update reservation {} {} {} {} {} {} {} {} {}",
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

        let balance_request = BalanceRequest::new(
            reservation.configuration_descriptor.clone(),
            reservation.exchange_account_id.clone(),
            reservation.currency_pair_metadata.currency_pair(),
            reservation.reservation_currency_code.clone(),
        );

        let reservation = self
            .easy_get_mut_reservation(reservation_id)
            .expect("must be non None");
        reservation.price = new_price;

        // let reservation = reservation.eas
        let reservation_amount_diff = reservation
            .currency_pair_metadata
            .convert_amount_into_amount_currency_code(
                &reservation.reservation_currency_code,
                reservation_amount_diff_in_reservation_currency,
                reservation.price,
            )
            .expect(
                format!(
                    "failed to convert amount into amount currency code for {:?} {}",
                    reservation.reservation_currency_code,
                    reservation_amount_diff_in_reservation_currency
                )
                .as_str(),
            );

        reservation.unreserved_amount -= reservation_amount_diff; // it will be compensated later

        self.add_reserved_amount(
            &balance_request,
            reservation_id,
            reservation_amount_diff,
            true,
        )
        .expect(
            format!(
                "failed to reserve amount for {:?} {} {}",
                balance_request, reservation_id, reservation_amount_diff,
            )
            .as_str(),
        );

        let reservation = self
            .easy_get_mut_reservation(reservation_id)
            .expect("must be non None");
        reservation.not_approved_amount = new_raw_rest_amount;

        log::info!(
            "Updated reservation {} {} {} {} {} {} {} {} {}",
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
        self.can_reserve_core(
            reserve_parameters,
            &mut Decimal::default(),
            &mut Decimal::default(),
            &mut Some(Decimal::default()),
            &mut BalanceReservationPreset::default(),
            explanation,
        )
    }

    pub fn get_available_leveraged_balance(
        &self,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        trade_side: OrderSide,
        price: Decimal,
        explanation: &mut Option<Explanation>,
    ) -> Option<Decimal> {
        self.try_get_available_balance(
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata.clone(),
            trade_side,
            price,
            true,
            true,
            explanation,
        )
    }

    pub fn set_target_amount_limit(
        &mut self,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        limit: Decimal,
    ) {
        for currency_code in [
            &currency_pair_metadata.base_currency_code,
            &currency_pair_metadata.quote_currency_code(),
        ] {
            let request = BalanceRequest::new(
                configuration_descriptor.clone(),
                exchange_account_id.clone(),
                currency_pair_metadata.currency_pair(),
                currency_code.clone(),
            );
            self.amount_limits_in_amount_currency
                .set_by_balance_request(&request, limit);
        }
    }
}
