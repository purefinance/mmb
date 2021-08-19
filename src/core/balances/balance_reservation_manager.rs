use std::collections::HashMap;

use anyhow::{bail, Result};
use itertools::Itertools;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::core::balance_manager::balance_position_by_fill_amount::BalancePositionByFillAmount;
use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::balance_manager::balance_reservation::BalanceReservation;
use crate::core::balances::balance_position_model::BalancePositionModel;
use crate::core::balances::{
    balance_reservation_storage::BalanceReservationStorage,
    virtual_balance_holder::VirtualBalanceHolder,
};
use crate::core::exchanges::common::CurrencyPair;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::general::currency_pair_metadata::{BeforeAfter, CurrencyPairMetadata};
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::explanation::Explanation;
use crate::core::misc::reserve_parameters::ReserveParameters;
use crate::core::misc::service_value_tree::ServiceValueTree;
use crate::core::orders::order::{ClientOrderId, OrderSide};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

pub(crate) struct BalanceReservationManager {
    exchanges_by_id: HashMap<ExchangeAccountId, Exchange>,

    // private readonly ICurrencyPairToSymbolConverter _currencyPairToSymbolConverter;
    // private readonly IDateTimeService _dateTimeService;
    // private readonly ILogger _logger = Log.ForContext<BalanceReservationManager>();
    reserved_amount_in_amount_currency: ServiceValueTree,
    amount_limits_in_amount_currency: ServiceValueTree,

    position_by_fill_amount_in_amount_currency: BalancePositionByFillAmount,
    reservation_id: i64, // Utils.GetCurrentMiliseconds();

    pub virtual_balance_holder: VirtualBalanceHolder,
    pub balance_reservation_storage: BalanceReservationStorage,

    is_call_from_clone: bool,
}

impl BalanceReservationManager {
    pub fn update_reserved_balances(
        &mut self,
        reserved_balances_by_id: &HashMap<i64, BalanceReservation>,
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

    pub fn get_reservation(&self, reservation_id: &i64) -> Option<&BalanceReservation> {
        self.balance_reservation_storage.try_get(reservation_id)
    }

    pub fn get_mut_reservation(&mut self, reservation_id: &i64) -> Option<&mut BalanceReservation> {
        self.balance_reservation_storage.try_get_mut(reservation_id)
    }

    pub fn unreserve(
        &mut self,
        reservation_id: i64,
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
        parametrs: &ReserveParameters,
        include_free_amount: bool,
        explanation: &mut Option<Explanation>,
    ) -> Decimal {
        self.try_get_available_balance(
            &parametrs.configuration_descriptor,
            &parametrs.exchange_account_id,
            &parametrs.currency_pair_metadata,
            parametrs.order_side,
            parametrs.price,
            include_free_amount,
            false,
            explanation,
        )
        .unwrap_or(dec!(0))
    }

    pub fn try_get_available_balance(
        &self,
        configuration_descriptor: &ConfigurationDescriptor,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: &CurrencyPairMetadata,
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
            &currency_pair_metadata,
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
                        currency_pair_metadata,
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
                currency_pair_metadata,
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
                currency_pair_metadata,
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
        currency_pair_metadata: &CurrencyPairMetadata,
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
        currency_pair_metadata: &CurrencyPairMetadata,
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
        currency_pair_metadata: &CurrencyPairMetadata,
        trade_side: OrderSide,
        mut balance_in_currency_code: Decimal,
        price: Decimal,
        leverage: Decimal,
        explanation: &mut Option<Explanation>,
    ) -> Option<Decimal> {
        let position = self.get_position_values(
            &request.configuration_descriptor,
            &request.exchange_account_id,
            currency_pair_metadata,
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
        currency_pair_metadata: &CurrencyPairMetadata,
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
        currency_pair_metadata: &CurrencyPairMetadata,
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

    fn easy_get_mut_reservation(&mut self, reservation_id: i64) -> Result<&mut BalanceReservation> {
        let res = match self.get_mut_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                bail!("Can't find reservation_id = {}", reservation_id,)
            }
        };
        Ok(res)
    }

    fn easy_get_reservation(&self, reservation_id: i64) -> Result<&BalanceReservation> {
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
        reservation_id: i64,
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
        reservation_id: i64,
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
        reservation_id: i64,
        diff_in_amount_currency: Decimal,
    ) -> Result<()> {
        let reservation = self.easy_get_reservation(reservation_id)?;

        let (currency_pair_metadata, price) = (
            reservation.currency_pair_metadata.clone(),
            reservation.price,
        );

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
}
