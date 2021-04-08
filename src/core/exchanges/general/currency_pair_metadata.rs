use std::sync::Arc;

use crate::core::{
    exchanges::common::Amount,
    exchanges::common::CurrencyCode,
    exchanges::common::CurrencyId,
    exchanges::common::SpecificCurrencyPair,
    exchanges::common::{CurrencyPair, Price},
    orders::order::OrderSide,
};
use anyhow::{bail, Result};
use rust_decimal_macros::dec;

use super::exchange::Exchange;

pub enum Round {
    Floor,
    Ceiling,
    ToNearest,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PrecisionType {
    ByFraction,
    ByMantissa,
}

// FIXME Strange name, need to fix
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BeforeAfter {
    Before,
    After,
}

pub const CURRENCY_PAIR_METADATA_DEFAULT_PRECISION: i8 = i8::MAX;

#[derive(Debug, Clone)]
pub struct CurrencyPairMetadata {
    pub is_active: bool,
    pub is_derivative: bool,
    pub base_currency_id: CurrencyId,
    pub base_currency_code: CurrencyCode,
    pub quote_currency_id: CurrencyId,
    pub quote_currency_code: CurrencyCode,
    // Currency pair in specific for exchange (which related to CurrencyPairMetadata)
    pub specific_currency_pair: SpecificCurrencyPair,
    pub min_price: Option<Price>,
    pub max_price: Option<Price>,
    pub price_precision: i8,
    pub price_precision_type: PrecisionType,
    pub price_tick: Option<Price>,
    pub amount_currency_code: CurrencyCode,
    pub min_amount: Option<Amount>,
    pub max_amount: Option<Amount>,
    pub amount_precision: i8,
    pub amount_precision_type: PrecisionType,
    pub amount_tick: Option<Amount>,
    pub min_cost: Option<Price>,
    pub balance_currency_code: Option<CurrencyCode>,
}

impl CurrencyPairMetadata {
    pub fn new(
        is_active: bool,
        is_derivative: bool,
        base_currency_id: CurrencyId,
        base_currency_code: CurrencyCode,
        quote_currency_id: CurrencyId,
        quote_currency_code: CurrencyCode,
        specific_currency_pair: SpecificCurrencyPair,
        min_price: Option<Price>,
        max_price: Option<Price>,
        price_precision: i8,
        price_precision_type: PrecisionType,
        price_tick: Option<Price>,
        amount_currency_code: CurrencyCode,
        min_amount: Option<Amount>,
        max_amount: Option<Amount>,
        amount_precision: i8,
        amount_precision_type: PrecisionType,
        amount_tick: Option<Amount>,
        min_cost: Option<Price>,
        balance_currency_code: Option<CurrencyCode>,
    ) -> Self {
        Self {
            is_active,
            is_derivative,
            base_currency_id,
            base_currency_code,
            quote_currency_id,
            quote_currency_code,
            specific_currency_pair,
            min_price,
            max_price,
            price_precision,
            price_precision_type,
            price_tick,
            amount_currency_code,
            min_amount,
            max_amount,
            amount_precision,
            amount_precision_type,
            amount_tick,
            min_cost,
            balance_currency_code,
        }
    }

    // Currency pair in unified for crate format
    pub fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_currency_codes(
            self.base_currency_code.clone(),
            self.quote_currency_code.clone(),
        )
    }

    pub fn get_trade_code(&self, side: OrderSide, before_after: BeforeAfter) -> CurrencyCode {
        use BeforeAfter::*;
        use OrderSide::*;

        match (before_after, side) {
            (Before, Buy) => self.quote_currency_code.clone(),
            (Before, Sell) => self.base_currency_code.clone(),
            (After, Buy) => self.base_currency_code.clone(),
            (After, Sell) => self.quote_currency_code.clone(),
        }
    }

    pub fn is_derivative(&self) -> bool {
        self.is_derivative
    }

    // TODO second params is round
    pub fn price_round(&self, price: Price, round: Round) -> Result<Price> {
        let tick = self.price_tick;
        match tick {
            Some(tick) => Ok(Self::round_by_tick(price, tick, round)?),
            None => {
                let price_precision = self.price_precision;
                let floored = match self.price_precision_type {
                    PrecisionType::ByFraction => {
                        Self::round_by_fraction(price, price_precision, round)?
                    }
                    PrecisionType::ByMantissa => {
                        Self::round_by_mantissa(price, price_precision, round)?
                    }
                };

                Ok(floored)
            }
        }
    }

    fn round_by_tick(value: Price, tick: Price, round: Round) -> Result<Price> {
        if tick <= dec!(0) {
            bail!("Too small tick: {}", tick)
        }

        Ok(value)
    }

    fn round_by_fraction(value: Price, _precision: i8, _round: Round) -> Result<Price> {
        // FIXME todo
        Ok(value)
    }

    fn round_by_mantissa(value: Price, _precision: i8, _round: Round) -> Result<Price> {
        // FIXME todo
        Ok(value)
    }

    // TODO is that appropriate return type?
    pub fn get_commision_currency_code(&self, _side: OrderSide) -> CurrencyCode {
        CurrencyCode::new("test".into())
    }

    // FIXME
    pub fn convert_amount_from_amount_currency_code(
        &self,
        _to_currency_code: CurrencyCode,
        amount_in_amount_currency_code: Amount,
        _currency_pair_price: Price,
    ) -> Amount {
        amount_in_amount_currency_code
    }
}

impl Exchange {
    pub fn get_currency_pair_metadata(
        &self,
        currency_pair: &CurrencyPair,
    ) -> Result<Arc<CurrencyPairMetadata>> {
        let specific_pair = self
            .exchange_client
            .get_specific_currency_pair(currency_pair);
        let currency_pairs = &self.symbols;
        match currency_pairs
            .lock()
            .iter()
            .find(|&current_pair| current_pair.specific_currency_pair == specific_pair)
        {
            Some(suitable_currency_pair_metadata) => Ok(suitable_currency_pair_metadata.clone()),
            None => bail!(
                "Unsupported currency pair on {} {:?}",
                self.exchange_account_id,
                currency_pair
            ),
        }
    }
}
