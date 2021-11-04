use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use mockall_double::double;
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
    Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price, TradePlaceAccount,
};
use crate::core::exchanges::general::currency_pair_metadata::{BeforeAfter, CurrencyPairMetadata};
use crate::core::exchanges::general::currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::explanation::{Explanation, OptionExplanationAddReasonExt};
use crate::core::misc::reserve_parameters::ReserveParameters;
use crate::core::misc::service_value_tree::ServiceValueTree;
#[double]
use crate::core::misc::time::time_manager;
use crate::core::misc::traits_ext::decimal_inverse_sign::DecimalInverseSign;
use crate::core::orders::order::{ClientOrderFillId, ClientOrderId, OrderSide};
use crate::core::orders::order::{ReservationId, ReservationIdVecToStringExt};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::core::DateTime;

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
    pub exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,

    pub currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
    reserved_amount_in_amount_currency: ServiceValueTree,
    amount_limits_in_amount_currency: ServiceValueTree,

    position_by_fill_amount_in_amount_currency: BalancePositionByFillAmount,
    reservation_id: ReservationId,

    pub virtual_balance_holder: VirtualBalanceHolder,
    pub balance_reservation_storage: BalanceReservationStorage,

    pub(crate) is_call_from_clone: bool,
}

impl BalanceReservationManager {
    pub fn new(
        exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
        currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
    ) -> Self {
        Self {
            exchanges_by_id: exchanges_by_id.clone(),
            currency_pair_to_metadata_converter,
            reserved_amount_in_amount_currency: ServiceValueTree::new(),
            amount_limits_in_amount_currency: ServiceValueTree::new(),
            position_by_fill_amount_in_amount_currency: BalancePositionByFillAmount::new(),
            reservation_id: ReservationId::generate(),
            virtual_balance_holder: VirtualBalanceHolder::new(exchanges_by_id),
            balance_reservation_storage: BalanceReservationStorage::new(),
            is_call_from_clone: false,
        }
    }

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
        fn make_balance_request(reservation: &BalanceReservation) -> BalanceRequest {
            BalanceRequest::new(
                reservation.configuration_descriptor.clone(),
                reservation.exchange_account_id,
                reservation.currency_pair_metadata.currency_pair(),
                reservation
                    .currency_pair_metadata
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

    pub fn try_get_reservation(
        &self,
        reservation_id: &ReservationId,
    ) -> Option<&BalanceReservation> {
        self.balance_reservation_storage.try_get(reservation_id)
    }

    pub fn get_reservation(&self, reservation_id: &ReservationId) -> &BalanceReservation {
        self.try_get_reservation(reservation_id)
            .expect("failed to get reservation for reservation_id: {}")
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
        amount: Amount,
        client_or_order_id: &Option<ClientOrderId>,
    ) -> Result<()> {
        let reservation = match self.try_get_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                let reservation_ids = self.balance_reservation_storage.get_reservation_ids();
                if self.is_call_from_clone || amount.is_zero() {
                    // Due to async nature of our trading engine we may receive in Clone reservation_ids which are already removed,
                    // so we need to ignore them instead of throwing an exception
                    log::error!(
                        "Can't find reservation {} ({}) for BalanceReservationManager::unreserve {} in list: {}",
                        reservation_id,
                        self.is_call_from_clone,
                        amount,
                        reservation_ids
                        .to_string()
                    );
                    return Ok(());
                }

                bail!(
                    "Can't find reservation_id={} for BalanceReservationManager::unreserve({}) attempt in list: {}",
                    reservation_id,
                    amount,
                    reservation_ids
                    .to_string()
                )
            }
        };

        let amount_to_unreserve = reservation
            .currency_pair_metadata
            .round_to_remove_amount_precision_error(amount)
            .context("Can't get amount_to_unreserve")?;

        if amount_to_unreserve.is_zero() && !reservation.amount.is_zero() {
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
                "Trying to BalanceReservationManager::unreserve for not existing exchange {}",
                reservation.exchange_account_id
            );
            return Ok(());
        }

        let balance_params = ReserveParameters::from_reservation(reservation, dec!(0));

        let old_balance = self.get_available_balance(&balance_params, true, &mut None);

        log::info!("VirtualBalanceHolder {}", old_balance);

        self.unreserve_not_approved_part(reservation_id, client_or_order_id, amount_to_unreserve)
            .with_context(|| format!("failed unreserve not approved part"))?;

        let reservation = self.get_reservation(&reservation_id);
        let balance_request = BalanceRequest::from_reservation(reservation);
        self.add_reserved_amount(&balance_request, reservation_id, -amount_to_unreserve, true)?;

        let new_balance = self.get_available_balance(&balance_params, true, &mut None);
        log::info!("VirtualBalanceHolder {}", new_balance);

        let mut reservation = self.get_reservation(&reservation_id).clone();
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
                    reservation.currency_pair_metadata.amount_precision,
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

    fn get_available_balance(
        &self,
        parameters: &ReserveParameters,
        include_free_amount: bool,
        explanation: &mut Option<Explanation>,
    ) -> Amount {
        self.try_get_available_balance(
            parameters.configuration_descriptor.clone(),
            parameters.exchange_account_id,
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
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        currency_code: CurrencyCode,
        price: Price,
    ) -> Option<Amount> {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            if currency_pair_metadata.get_trade_code(side, BeforeAfter::Before) == currency_code {
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
            exchange_account_id,
            currency_pair_metadata.currency_pair(),
            currency_code,
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
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
        price: Price,
        include_free_amount: bool,
        is_leveraged: bool,
        explanation: &mut Option<Explanation>,
    ) -> Option<Amount> {
        let currency_code = currency_pair_metadata.get_trade_code(side, BeforeAfter::Before);
        let request = BalanceRequest::new(
            configuration_descriptor.clone(),
            exchange_account_id,
            currency_pair_metadata.currency_pair(),
            currency_code,
        );
        let balance_in_currency_code = self.virtual_balance_holder.get_virtual_balance(
            &request,
            currency_pair_metadata.clone(),
            Some(price),
            explanation,
        );

        explanation.add_reason(format!(
            "balance_in_currency_code_raw = {:?}",
            balance_in_currency_code
        ));

        let mut balance_in_currency_code = balance_in_currency_code?;

        let leverage =
            self.get_leverage(exchange_account_id, currency_pair_metadata.currency_pair());

        explanation.add_reason(format!("leverage = {:?}", leverage));

        if currency_pair_metadata.is_derivative {
            if include_free_amount {
                let free_amount_in_amount_currency_code = self
                    .get_unreserved_position_in_amount_currency_code(
                        exchange_account_id,
                        currency_pair_metadata.clone(),
                        side,
                    );

                explanation.add_reason(format!(
                    "free_amount_in_amount_currency_code with leverage and amount_multiplier = {}",
                    free_amount_in_amount_currency_code
                ));

                let mut free_amount_in_currency_code = currency_pair_metadata
                    .convert_amount_from_amount_currency_code(
                        currency_code,
                        free_amount_in_amount_currency_code,
                        price,
                    );
                free_amount_in_currency_code /= leverage;
                free_amount_in_currency_code *= currency_pair_metadata.amount_multiplier;

                explanation.add_reason(format!(
                    "free_amount_in_currency_code = {}",
                    free_amount_in_currency_code
                ));

                balance_in_currency_code += free_amount_in_currency_code;

                explanation.add_reason(format!(
                    "balance_in_currency_code with free amount: {}",
                    balance_in_currency_code
                ));
            }

            balance_in_currency_code -= BalanceReservationManager::get_untouchable_amount(
                currency_pair_metadata.clone(),
                balance_in_currency_code,
            );

            explanation.add_reason(format!(
                "balance_in_currency_code without untouchable: {}",
                balance_in_currency_code
            ));
        }
        if !self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request)
            .is_none()
        {
            balance_in_currency_code = self.get_balance_with_applied_limits(
                &request,
                currency_pair_metadata.clone(),
                side,
                balance_in_currency_code,
                price,
                leverage,
                explanation,
            );
        }

        explanation.add_reason(format!(
            "balance_in_currency_code with limit: {}",
            balance_in_currency_code
        ));

        // isLeveraged is used when we need to know how much funds we can use for orders
        if is_leveraged {
            balance_in_currency_code *= leverage;
            balance_in_currency_code /= currency_pair_metadata.amount_multiplier;

            explanation.add_reason(format!(
                "balance_in_currency_code with leverage and multiplier: {}",
                balance_in_currency_code
            ));
        }
        Some(balance_in_currency_code)
    }

    pub fn get_position_in_amount_currency_code(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> Decimal {
        if !currency_pair_metadata.is_derivative {
            return dec!(0);
        }

        let current_position = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, currency_pair_metadata.currency_pair())
            .unwrap_or(dec!(0));
        match side {
            OrderSide::Buy => dec!(0).max(-current_position),
            OrderSide::Sell => dec!(0).max(current_position),
        }
    }

    fn get_unreserved_position_in_amount_currency_code(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> Decimal {
        let position = self.get_position_in_amount_currency_code(
            exchange_account_id,
            currency_pair_metadata,
            side,
        );

        let taken_amount = self
            .balance_reservation_storage
            .get_all_raw_reservations()
            .iter()
            .filter(|(_, balance_reservation)| balance_reservation.order_side == side)
            .map(|(_, balance_reservation)| balance_reservation.taken_free_amount)
            .sum::<Amount>();

        dec!(0).max(position - taken_amount)
    }

    fn get_balance_with_applied_limits(
        &self,
        request: &BalanceRequest,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
        mut balance_in_currency_code: Amount,
        price: Price,
        leverage: Decimal,
        explanation: &mut Option<Explanation>,
    ) -> Amount {
        let position = self.get_position_values(
            request.configuration_descriptor.clone(),
            request.exchange_account_id,
            currency_pair_metadata.clone(),
            side,
        );

        let position_amount_in_amount_currency = position.position;
        explanation.add_reason(format!(
            "position_amount_in_amount_currency: {}",
            position_amount_in_amount_currency
        ));

        let reserved_amount_in_amount_currency = self
            .reserved_amount_in_amount_currency
            .get_by_balance_request(request)
            .unwrap_or(dec!(0));
        explanation.add_reason(format!(
            "reserved_amount_in_amount_currency: {}",
            reserved_amount_in_amount_currency
        ));

        let reservation_with_fills_in_amount_currency =
            reserved_amount_in_amount_currency + position_amount_in_amount_currency;
        explanation.add_reason(format!(
            "reservation_with_fills_in_amount_currency: {}",
            reservation_with_fills_in_amount_currency
        ));

        let total_amount_limit_in_amount_currency = position.limit.unwrap_or(dec!(0));
        explanation.add_reason(format!(
            "total_amount_limit_in_amount_currency: {}",
            total_amount_limit_in_amount_currency
        ));

        let limit_left_in_amount_currency =
            total_amount_limit_in_amount_currency - reservation_with_fills_in_amount_currency;
        explanation.add_reason(format!(
            "limit_left_in_amount_currency: {}",
            limit_left_in_amount_currency
        ));

        //AmountLimit is applied to full amount
        balance_in_currency_code *= leverage;
        balance_in_currency_code /= currency_pair_metadata.amount_multiplier;
        explanation.add_reason(format!(
            "balance_in_currency_code with leverage and multiplier: {}",
            balance_in_currency_code
        ));

        let balance_in_amount_currency = currency_pair_metadata
            .convert_amount_into_amount_currency_code(
                request.currency_code,
                balance_in_currency_code,
                price,
            );
        explanation.add_reason(format!(
            "balance_in_amount_currency with leverage and multiplier: {}",
            balance_in_amount_currency
        ));

        let limited_balance_in_amount_currency =
            balance_in_amount_currency.min(limit_left_in_amount_currency);
        explanation.add_reason(format!(
            "limited_balance_in_amount_currency: {}",
            limited_balance_in_amount_currency
        ));

        let mut limited_balance_in_currency_code = currency_pair_metadata
            .convert_amount_from_amount_currency_code(
                request.currency_code,
                limited_balance_in_amount_currency,
                price,
            );
        explanation.add_reason(format!(
            "limited_balance_in_currency_code: {}",
            limited_balance_in_currency_code
        ));

        //converting back to pure balance
        limited_balance_in_currency_code /= leverage;
        limited_balance_in_currency_code *= currency_pair_metadata.amount_multiplier;
        explanation.add_reason(format!(
            "limited_balance_in_currency_code without leverage and multiplier: {}",
            limited_balance_in_currency_code
        ));

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
        };

        dec!(0).max(limited_balance_in_currency_code)
    }

    fn get_untouchable_amount(
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        amount: Amount,
    ) -> Amount {
        // We want to keep the trading engine from reserving all the balance for derivatives as so far we don't take into account
        // many derivative nuances (commissions, funding, probably something else
        match currency_pair_metadata.is_derivative {
            true => amount * dec!(0.05),
            false => dec!(0),
        }
    }

    fn get_leverage(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
    ) -> Decimal {
        self.exchanges_by_id
            .get(&exchange_account_id)
            .expect("failed to get exchange")
            .leverage_by_currency_pair
            .get(&currency_pair)
            .as_deref()
            .expect("failed to get leverage")
            .clone()
    }

    fn get_position_values(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> BalancePositionModel {
        let currency_code = currency_pair_metadata.get_trade_code(side, BeforeAfter::Before);
        let request = BalanceRequest::new(
            configuration_descriptor.clone(),
            exchange_account_id,
            currency_pair_metadata.currency_pair(),
            currency_code,
        );
        let total_amount_limit_in_amount_currency = self
            .amount_limits_in_amount_currency
            .get_by_balance_request(&request);

        let position = self.get_position(
            exchange_account_id,
            currency_pair_metadata.currency_pair(),
            side,
        );

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
        let currency_pair_metadata = self
            .currency_pair_to_metadata_converter
            .get_currency_pair_metadata(exchange_account_id, currency_pair);

        let currency_code = currency_pair_metadata.get_trade_code(side, BeforeAfter::Before);
        let mut position_in_amount_currency = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, currency_pair)
            .unwrap_or(dec!(0));

        if currency_code == currency_pair_metadata.base_currency_code {
            //sell
            position_in_amount_currency.inverse_sign();
        }

        position_in_amount_currency
    }

    fn unreserve_not_approved_part(
        &mut self,
        reservation_id: ReservationId,
        client_order_id: &Option<ClientOrderId>,
        amount_to_unreserve: Amount,
    ) -> Result<()> {
        let reservation = self
            .get_mut_reservation(&reservation_id)
            .expect("Failed to get mut reservation");
        let client_order_id = match client_order_id {
            Some(client_order_id) => client_order_id,
            None => {
                reservation.not_approved_amount -= amount_to_unreserve;
                // this case will be handled by UnReserve itself
                if reservation.not_approved_amount < dec!(0)
                    && reservation.unreserved_amount > amount_to_unreserve
                {
                    bail!(
                        "Possibly BalanceReservationManager::unreserve_not_approved_part {} should be called with clientOrderId parameter",
                        reservation_id
                    )
                }
                return Ok(());
            }
        };

        let approved_part = match reservation.approved_parts.get_mut(client_order_id) {
            Some(approved_part) => approved_part,
            None => {
                log::warn!("unreserve({}, {}) called with clientOrderId {} for reservation without the approved part {:?}",
                reservation_id, amount_to_unreserve, client_order_id, reservation);
                reservation.not_approved_amount -= amount_to_unreserve;
                if reservation.not_approved_amount < dec!(0) {
                    log::error!("not_approved_amount for {} was unreserved for the missing order {} and now < 0 {:?}",
                    reservation_id, client_order_id, reservation);
                }
                return Ok(());
            }
        };

        let new_unreserved_amount_for_approved_part =
            approved_part.unreserved_amount - amount_to_unreserve;
        if new_unreserved_amount_for_approved_part < dec!(0) {
            bail!(
                "Attempt to unreserve more than was approved for order {} ({}): {} > {}",
                client_order_id,
                reservation_id,
                amount_to_unreserve,
                approved_part.unreserved_amount
            )
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
                .with_context(|| {
                    format!(
                        "Failed to get proportional cost amount form {:?} with {}",
                        reservation, amount_diff_in_amount_currency
                    )
                })?;
            virtual_balance_holder.add_balance_by_currency_pair_metadata(
                request,
                reservation.currency_pair_metadata.clone(),
                -cost,
                reservation.price,
            );
        }

        reservation.unreserved_amount += amount_diff_in_amount_currency;

        // global reservation indicator
        let res_amount_request = BalanceRequest::new(
            request.configuration_descriptor.clone(),
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
            &balance_request,
            &mut self.virtual_balance_holder,
            self.balance_reservation_storage
                .try_get_mut(&reservation_id)
                .expect("Failed to get reservation"),
            &mut self.reserved_amount_in_amount_currency,
            amount_diff_in_amount_currency,
            update_balance,
        )
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
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        new_position: Decimal,
    ) -> Result<()> {
        if !currency_pair_metadata.is_derivative {
            bail!("restore_fill_amount_position is available only for derivative exchanges")
        }
        let previous_value = self
            .position_by_fill_amount_in_amount_currency
            .get(exchange_account_id, currency_pair_metadata.currency_pair());

        let now = time_manager::now();

        self.position_by_fill_amount_in_amount_currency.set(
            exchange_account_id,
            currency_pair_metadata.currency_pair(),
            previous_value,
            new_position,
            None,
            now,
        );
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
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> Decimal {
        let position = self.get_position_values(
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata.clone(),
            side,
        );

        let limit = position
            .limit
            .expect("failed to get_fill_amount_position_percent, limit is None");

        dec!(1).min(dec!(0).max(position.position / limit))
    }

    pub fn handle_position_fill_amount_change(
        &mut self,
        side: OrderSide,
        before_after: BeforeAfter,
        client_order_fill_id: &Option<ClientOrderFillId>,
        fill_amount: Amount,
        price: Price,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
    ) -> (Amount, CurrencyCode) {
        let mut change_amount_in_currency = dec!(0);

        let currency_code = currency_pair_metadata.get_trade_code(side, before_after);
        let request = BalanceRequest::new(
            configuration_descriptor.clone(),
            exchange_account_id,
            currency_pair_metadata.currency_pair(),
            currency_code,
        );

        if !currency_pair_metadata.is_derivative {
            self.virtual_balance_holder
                .add_balance_by_currency_pair_metadata(
                    &request,
                    currency_pair_metadata.clone(),
                    -fill_amount,
                    price,
                );

            change_amount_in_currency = currency_pair_metadata
                .convert_amount_from_amount_currency_code(currency_code, fill_amount, price);
        }
        if currency_pair_metadata.amount_currency_code == currency_code {
            let mut position_change = fill_amount;
            if currency_pair_metadata.is_derivative {
                let free_amount = self.get_position_in_amount_currency_code(
                    exchange_account_id,
                    currency_pair_metadata.clone(),
                    side,
                );
                let move_amount = fill_amount.abs();
                let (add_amount, sub_amount) = if free_amount - move_amount >= dec!(0) {
                    (move_amount, dec!(0))
                } else {
                    (free_amount, (free_amount - move_amount).abs())
                };

                let leverage =
                    self.get_leverage(exchange_account_id, currency_pair_metadata.currency_pair());
                let diff_in_amount_currency =
                    (add_amount - sub_amount) / leverage * currency_pair_metadata.amount_multiplier;
                self.virtual_balance_holder
                    .add_balance_by_currency_pair_metadata(
                        &request,
                        currency_pair_metadata.clone(),
                        diff_in_amount_currency,
                        price,
                    );

                change_amount_in_currency = currency_pair_metadata
                    .convert_amount_from_amount_currency_code(
                        currency_code,
                        diff_in_amount_currency,
                        price,
                    );

                // reversed derivative
                if currency_pair_metadata.amount_currency_code
                    == currency_pair_metadata.base_currency_code()
                {
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
    ) {
        let reservation = match self.get_mut_reservation(&reservation_id) {
            Some(reservation_id) => reservation_id,
            None => {
                log::error!(
                    "Can't find reservation {} in {}",
                    reservation_id,
                    self.balance_reservation_storage
                        .get_reservation_ids()
                        .to_string()
                );
                return ();
            }
        };

        let approved_part = match reservation.approved_parts.get_mut(client_order_id) {
            Some(approved_part) => approved_part,
            None => {
                log::error!("There is no approved part for order {}", client_order_id);
                return ();
            }
        };

        if approved_part.is_canceled {
            panic!(
                "Approved part was already canceled for {} {}",
                client_order_id, reservation_id
            )
        }

        reservation.not_approved_amount += approved_part.unreserved_amount;
        approved_part.is_canceled = true;
        log::info!(
            "Canceled approved part for order {} with {}",
            client_order_id,
            approved_part.unreserved_amount
        );
    }

    pub fn handle_position_fill_amount_change_commission(
        &mut self,
        commission_currency_code: CurrencyCode,
        commission_amount: Amount,
        converted_commission_currency_code: CurrencyCode,
        converted_commission_amount: Amount,
        price: Price,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
    ) {
        let leverage =
            self.get_leverage(exchange_account_id, currency_pair_metadata.currency_pair());
        if !currency_pair_metadata.is_derivative
            || currency_pair_metadata.balance_currency_code == Some(commission_currency_code)
        {
            let request = BalanceRequest::new(
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata.currency_pair(),
                commission_currency_code,
            );
            let res_commission_amount = commission_amount / leverage;
            self.virtual_balance_holder
                .add_balance(&request, -res_commission_amount);
        } else {
            let request = BalanceRequest::new(
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata.currency_pair(),
                converted_commission_currency_code,
            );
            let commission_in_amount_currency = currency_pair_metadata
                .convert_amount_into_amount_currency_code(
                    converted_commission_currency_code,
                    converted_commission_amount,
                    price,
                );
            let res_commission_amount_in_amount_currency = commission_in_amount_currency / leverage;
            self.virtual_balance_holder
                .add_balance_by_currency_pair_metadata(
                    &request,
                    currency_pair_metadata.clone(),
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
        let reservation = match self.get_mut_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                log::error!(
                    "Can't find reservation {} in {}",
                    reservation_id,
                    self.balance_reservation_storage
                        .get_reservation_ids()
                        .to_string()
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
        reservation.approved_parts.insert(
            client_order_id.clone(),
            ApprovedPart::new(approve_time, client_order_id.clone(), amount),
        );

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
        let src_reservation = self.get_reservation(&src_reservation_id);

        let dst_reservation = self.get_reservation(&dst_reservation_id);

        if src_reservation.configuration_descriptor != dst_reservation.configuration_descriptor
            || src_reservation.exchange_account_id != dst_reservation.exchange_account_id
            || src_reservation.currency_pair_metadata != dst_reservation.currency_pair_metadata
            || src_reservation.order_side != dst_reservation.order_side
        {
            panic!(
                "Reservations {:?} and {:?} are from different sources",
                src_reservation, dst_reservation
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
        if amount_to_move.is_zero() {
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
                let add_amount = src_reservation.convert_in_reservation_currency(amount_to_move);
                let sub_amount = dst_reservation.convert_in_reservation_currency(amount_to_move);

                let balance_diff_amount = add_amount - sub_amount;

                let available_balance = self
                    .try_get_available_balance(
                        dst_reservation.configuration_descriptor.clone(),
                        dst_reservation.exchange_account_id,
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
        true
    }

    fn transfer_amount(
        &mut self,
        src_reservation_id: ReservationId,
        dst_reservation_id: ReservationId,
        amount_to_move: Amount,
        client_order_id: &Option<ClientOrderId>,
    ) {
        let src_reservation = self.get_reservation(&src_reservation_id);
        let new_src_unreserved_amount = src_reservation.unreserved_amount - amount_to_move;
        log::info!(
            "trying to update src unreserved amount for transfer: {:?} {} {:?}",
            src_reservation,
            new_src_unreserved_amount,
            client_order_id
        );
        let src_cost_diff = self.update_unreserved_amount_for_transfer(
            src_reservation_id,
            new_src_unreserved_amount,
            client_order_id,
            true,
            dec!(0),
        );

        let dst_reservation = self.get_reservation(&dst_reservation_id);
        let new_dst_unreserved_amount = dst_reservation.unreserved_amount + amount_to_move;
        log::info!(
            "trying to update dst unreserved amount for transfer: {:?} {} {:?}",
            dst_reservation,
            new_dst_unreserved_amount,
            client_order_id
        );
        let _ = self.update_unreserved_amount_for_transfer(
            dst_reservation_id,
            new_dst_unreserved_amount,
            client_order_id,
            false,
            -src_cost_diff,
        );

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
    ) -> Decimal {
        let approve_time = time_manager::now();
        let reservation = self
            .get_mut_reservation(&reservation_id)
            .expect("Failed to get mut reservation");
        // we should check the case when we have insignificant calculation errors
        if new_unreserved_amount < dec!(0)
            && !reservation.is_amount_within_symbol_margin_error(new_unreserved_amount)
        {
            panic!(
                "Can't set {} amount to reservation {}",
                new_unreserved_amount, reservation_id
            )
        }

        let reservation_amount_diff = new_unreserved_amount - reservation.unreserved_amount;
        if let Some(client_order_id) = client_order_id {
            if let Some(approved_part) = reservation.approved_parts.get(client_order_id) {
                let new_amount = approved_part.unreserved_amount + reservation_amount_diff;
                if reservation.is_amount_within_symbol_margin_error(new_amount) {
                    let _ = reservation.approved_parts.remove(client_order_id);
                } else if new_amount < dec!(0) {
                    panic!(
                            "Attempt to transfer more amount ({}) than we have ({}) for approved part by ClientOrderId {}",
                            reservation_amount_diff,
                            reservation
                                .approved_parts
                                .get_mut(client_order_id)
                                .expect("fix me").unreserved_amount,
                            client_order_id
                        )
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
                    panic!(
                        "Can't find approved part {} for {}",
                        client_order_id, reservation_id
                    )
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

        self.add_reserved_amount(
            &balance_request,
            reservation_id,
            reservation_amount_diff,
            false,
        )
        .expect("failed to add reserved amount");
        let reservation = self
            .get_mut_reservation(&reservation_id)
            .expect("Failed to get mut reservation");

        let cost_diff = if is_src_request {
            reservation
                .get_proportional_cost_amount(reservation_amount_diff)
                .expect("Failed to get proportional cost amount")
        } else {
            target_cost_diff
        };
        let buff_price = reservation.price;
        let buff_currency_pair_metadata = reservation.currency_pair_metadata.clone();

        self.virtual_balance_holder
            .add_balance_by_currency_pair_metadata(
                &balance_request,
                buff_currency_pair_metadata,
                -cost_diff,
                buff_price,
            );
        let reservation = self
            .get_mut_reservation(&reservation_id)
            .expect("Failed to get mut reservation");

        reservation.cost += cost_diff;
        reservation.amount += reservation_amount_diff;
        let reservation = self.get_reservation(&reservation_id).clone();

        if reservation.is_amount_within_symbol_margin_error(new_unreserved_amount) {
            self.balance_reservation_storage
                .remove(reservation_id.clone());

            if !new_unreserved_amount.is_zero() {
                log::error!(
                    "Transfer: AmountLeft {} != 0 for {} {:?}",
                    reservation.unreserved_amount,
                    reservation_id,
                    reservation
                );
            }
        }
        log::info!(
            "Updated reservation {} {} {} {:?} {} {} {}",
            reservation_id,
            reservation.exchange_account_id,
            reservation.reservation_currency_code,
            reservation.order_side,
            reservation.price,
            reservation.amount,
            reservation_amount_diff
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
                self.unreserve(res_id, res_params.amount, &None).expect(
                    format!("failed to unreserve for {} {}", res_id, res_params.amount).as_str(),
                );
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
                "Failed to reserve {} {} {:?} {} {} {:?}",
                can_reserve_result.preset.reservation_currency_code,
                can_reserve_result
                    .preset
                    .amount_in_reservation_currency_code,
                can_reserve_result.potential_position,
                can_reserve_result.old_balance,
                can_reserve_result.new_balance,
                reserve_parameters
            );
            return None;
        }

        let request = BalanceRequest::new(
            reserve_parameters.configuration_descriptor.clone(),
            reserve_parameters.exchange_account_id,
            reserve_parameters.currency_pair_metadata.currency_pair(),
            can_reserve_result.preset.reservation_currency_code,
        );
        let reservation = BalanceReservation::new(
            reserve_parameters.configuration_descriptor.clone(),
            reserve_parameters.exchange_account_id,
            reserve_parameters.currency_pair_metadata.clone(),
            reserve_parameters.order_side,
            reserve_parameters.price,
            reserve_parameters.amount,
            can_reserve_result
                .preset
                .taken_free_amount_in_amount_currency_code,
            can_reserve_result.preset.cost_in_amount_currency_code,
            can_reserve_result.preset.reservation_currency_code,
        );

        self.reservation_id = ReservationId::generate();
        log::info!(
            "Trying to reserve {:?} {} {} {:?} {} {} {:?}",
            self.reservation_id,
            can_reserve_result.preset.reservation_currency_code,
            can_reserve_result
                .preset
                .amount_in_reservation_currency_code,
            can_reserve_result.potential_position,
            can_reserve_result.old_balance,
            can_reserve_result.new_balance,
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
        Some(self.reservation_id)
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

        explanation.add_reason(format!(
            "old_balance: {} preset_cost: {} new_balance: {}",
            old_balance, preset_cost, new_balance
        ));

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
            .currency_pair_metadata
            .round_to_remove_amount_precision_error(new_balance)
            .expect(
                format!(
                    "failed to round to remove amount precision error from {:?} for {}",
                    reserve_parameters.currency_pair_metadata, new_balance
                )
                .as_str(),
            );
        CanReserveResult {
            can_reserve: rounded_balance >= dec!(0),
            preset,
            potential_position,
            old_balance,
            new_balance,
        }
    }

    fn can_reserve_with_limit(
        &self,
        reserve_parameters: &ReserveParameters,
    ) -> (bool, Option<Decimal>) {
        let reservation_currency_code = reserve_parameters
            .currency_pair_metadata
            .get_trade_code(reserve_parameters.order_side, BeforeAfter::Before);

        let request = BalanceRequest::new(
            reserve_parameters.configuration_descriptor.clone(),
            reserve_parameters.exchange_account_id,
            reserve_parameters.currency_pair_metadata.currency_pair(),
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

        let position = self
            .position_by_fill_amount_in_amount_currency
            .get(request.exchange_account_id, request.currency_pair)
            .unwrap_or(dec!(0));

        let potential_position = match reserve_parameters.order_side {
            OrderSide::Buy => position + new_reserved_amount,
            OrderSide::Sell => position - new_reserved_amount,
        };

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
        let currency_pair_metadata = reserve_parameters.currency_pair_metadata.clone();

        let reservation_currency_code = self
            .exchanges_by_id
            .get(&reserve_parameters.exchange_account_id)
            .expect("failed to get exchange")
            .get_balance_reservation_currency_code(
                currency_pair_metadata.clone(),
                reserve_parameters.order_side,
            );

        let amount_in_reservation_currency_code = currency_pair_metadata
            .convert_amount_from_amount_currency_code(reservation_currency_code, amount, price);

        let (cost_in_amount_currency_code, taken_free_amount) =
            self.calculate_reservation_cost(reserve_parameters);
        let cost_in_reservation_currency_code = currency_pair_metadata
            .convert_amount_from_amount_currency_code(
                reservation_currency_code,
                cost_in_amount_currency_code,
                price,
            );

        explanation.add_reason(format!(
            "cost_in_reservation_currency_code: {} taken_free_amount: {}",
            cost_in_reservation_currency_code, taken_free_amount
        ));

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
        if !reserve_parameters.currency_pair_metadata.is_derivative {
            return (reserve_parameters.amount, dec!(0));
        }

        let free_amount = self.get_unreserved_position_in_amount_currency_code(
            reserve_parameters.exchange_account_id,
            reserve_parameters.currency_pair_metadata.clone(),
            reserve_parameters.order_side,
        );

        let amount_to_pay_for = dec!(0).max(reserve_parameters.amount - free_amount);

        let taken_free_amount = reserve_parameters.amount - amount_to_pay_for;

        // TODO: use full formula (with fee and etc)
        let leverage = self.get_leverage(
            reserve_parameters.exchange_account_id,
            reserve_parameters.currency_pair_metadata.currency_pair(),
        );

        (
            amount_to_pay_for * reserve_parameters.currency_pair_metadata.amount_multiplier
                / leverage,
            taken_free_amount,
        )
    }

    pub fn try_update_reservation_price(
        &mut self,
        reservation_id: ReservationId,
        new_price: Price,
    ) -> bool {
        let reservation = match self.try_get_reservation(&reservation_id) {
            Some(reservation) => reservation,
            None => {
                log::error!(
                    "Can't find reservation {} in {}",
                    reservation_id,
                    self.balance_reservation_storage
                        .get_reservation_ids()
                        .to_string()
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
        let new_rest_amount_in_reservation_currency = reservation
            .currency_pair_metadata
            .convert_amount_from_amount_currency_code(
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
                reservation.configuration_descriptor.clone(),
                reservation.exchange_account_id,
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

        let reservation = self
            .get_mut_reservation(&reservation_id)
            .expect("Failed to get mut reservation");
        reservation.price = new_price;

        let reservation_amount_diff = reservation
            .currency_pair_metadata
            .convert_amount_into_amount_currency_code(
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
        .expect(
            format!(
                "failed to reserve amount for {:?} {} {}",
                balance_request, reservation_id, reservation_amount_diff,
            )
            .as_str(),
        );

        let reservation = self
            .get_mut_reservation(&reservation_id)
            .expect("Failed to get mut reservation");
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
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
        price: Price,
        explanation: &mut Option<Explanation>,
    ) -> Option<Amount> {
        self.try_get_available_balance(
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata.clone(),
            side,
            price,
            true,
            true,
            explanation,
        )
    }

    pub fn set_target_amount_limit(
        &mut self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        limit: Amount,
    ) {
        for currency_code in [
            currency_pair_metadata.base_currency_code,
            currency_pair_metadata.quote_currency_code(),
        ] {
            let request = BalanceRequest::new(
                configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair_metadata.currency_pair(),
                currency_code,
            );
            self.amount_limits_in_amount_currency
                .set_by_balance_request(&request, limit);
        }
    }
}
