use std::sync::Arc;

use anyhow::{bail, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::core::{
    exchanges::common::Amount,
    exchanges::common::CurrencyCode,
    exchanges::common::CurrencyId,
    exchanges::common::{CurrencyPair, Price},
    math::powi,
    orders::order::OrderSide,
};

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

// TODO Change to Maker-Taker
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BeforeAfter {
    Before,
    After,
}

pub const CURRENCY_PAIR_METADATA_DEFAULT_PRECISION: i8 = i8::MAX;

enum Precision {
    ByTick { tick: Decimal },
    ByMantisa { precesion: i8 },
    ByFraction { precesion: i8 },
}

#[derive(Debug, Clone)]
pub struct CurrencyPairMetadata {
    pub is_active: bool,
    pub is_derivative: bool,
    pub base_currency_id: CurrencyId,
    pub base_currency_code: CurrencyCode,
    pub quote_currency_id: CurrencyId,
    pub quote_currency_code: CurrencyCode,
    pub min_price: Option<Price>,
    pub max_price: Option<Price>,
    pub amount_currency_code: CurrencyCode,
    pub min_amount: Option<Amount>,
    pub max_amount: Option<Amount>,
    pub min_cost: Option<Price>,
    pub balance_currency_code: Option<CurrencyCode>,
    pub price_precision: Precision,
    pub amount_precision: Precision,
}

impl CurrencyPairMetadata {
    pub fn base_currency_code(&self) -> CurrencyCode {
        self.base_currency_code.clone()
    }

    pub fn quote_currency_code(&self) -> CurrencyCode {
        self.quote_currency_code.clone()
    }

    pub fn new(
        is_active: bool,
        is_derivative: bool,
        base_currency_id: CurrencyId,
        base_currency_code: CurrencyCode,
        quote_currency_id: CurrencyId,
        quote_currency_code: CurrencyCode,
        min_price: Option<Price>,
        max_price: Option<Price>,
        amount_currency_code: CurrencyCode,
        min_amount: Option<Amount>,
        max_amount: Option<Amount>,
        min_cost: Option<Price>,
        balance_currency_code: Option<CurrencyCode>,
        price_precision: Price,
        amount_precision: Amount,
    ) -> Self {
        Self {
            is_active,
            is_derivative,
            base_currency_id,
            base_currency_code,
            quote_currency_id,
            quote_currency_code,
            min_price,
            max_price,
            amount_currency_code,
            min_amount,
            max_amount,
            min_cost,
            balance_currency_code,
            price_precision,
            amount_precision,
        }
    }

    // Currency pair in unified for crate format
    pub fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes(
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

    pub fn price_round(&self, price: Price, round: Round) -> Result<Price> {
        if let Some(tick) = self.price_tick {
            return Self::round_by_tick(price, tick, round);
        }

        let price_precision = self.price_precision;
        match self.price_precision_type {
            PrecisionType::ByFraction => Ok(Self::round_by_fraction(price, price_precision, round)),
            PrecisionType::ByMantissa => Self::round_by_mantissa(price, price_precision, round),
        }
    }

    pub fn amount_round(&self, amount: Amount, round: Round) -> Result<Amount> {
        match self.amount_tick {
            Some(tick) => Self::round_by_tick(amount, tick, round),
            None => self.amount_round_pr(amount, round, self.amount_precision),
        }
    }

    /// Rounding of order amount with specified precision
    pub fn amount_round_pr(
        &self,
        amount: Amount,
        round: Round,
        amount_precision: i8,
    ) -> Result<Amount> {
        match self.amount_precision_type {
            PrecisionType::ByFraction => {
                Ok(Self::round_by_fraction(amount, amount_precision, round))
            }
            PrecisionType::ByMantissa => Self::round_by_mantissa(amount, amount_precision, round),
        }
    }

    pub fn round_to_remove_amount_precision_error(&self, amount: Amount) -> Result<Amount> {
        // allowed machine error that is less then 0.01 * amount precision
        self.amount_round_pr(amount, Round::ToNearest, self.amount_precision + 2i8)
    }

    fn round_by_tick(value: Price, tick: Price, round: Round) -> Result<Price> {
        if tick <= dec!(0) {
            bail!("Too small tick: {}", tick)
        }

        Ok(Self::inner_round_by_tick(value, tick, round))
    }

    fn inner_round_by_tick(value: Price, tick: Price, round: Round) -> Price {
        let floor = (value / tick).floor() * tick;
        let ceil = (value / tick).ceil() * tick;

        match round {
            Round::Floor => floor,
            Round::Ceiling => ceil,
            Round::ToNearest => {
                if ceil - value <= value - floor {
                    ceil
                } else {
                    floor
                }
            }
        }
    }

    fn round_by_mantissa(value: Price, precision: i8, round: Round) -> Result<Price> {
        if value == dec!(0) {
            return Ok(dec!(0));
        }

        let floor_digits = Self::get_precision_digits_by_fractional(value, precision)?;

        Ok(Self::round_by_fraction(value, floor_digits, round))
    }

    fn get_precision_digits_by_fractional(value: Price, precision: i8) -> Result<i8> {
        if precision <= 0 {
            bail!(
                "Count of precision digits cannot be less 1 but got {}",
                precision
            )
        }

        let mut integral_digits;
        if value >= dec!(1) {
            integral_digits = 1;
            let mut tmp = value * dec!(0.1);
            while tmp >= dec!(1) {
                tmp *= dec!(0.1);
                integral_digits += 1;
            }
        } else {
            integral_digits = 0;
            let mut tmp = value * dec!(10);
            while tmp < dec!(1) {
                tmp *= dec!(10);
                integral_digits -= 1;
            }
        }

        let floor_digits = precision - integral_digits;

        Ok(floor_digits)
    }

    fn round_by_fraction(value: Price, precision: i8, round: Round) -> Price {
        let multiplier = dec!(0.1);
        let pow_precision = powi(multiplier, precision);

        Self::inner_round_by_tick(value, pow_precision, round)
    }

    pub fn get_commission_currency_code(&self, side: OrderSide) -> CurrencyCode {
        match &self.balance_currency_code {
            Some(balance_currency_code) => balance_currency_code.clone(),
            None => match side {
                OrderSide::Buy => self.base_currency_code.clone(),
                OrderSide::Sell => self.quote_currency_code.clone(),
            },
        }
    }

    pub fn convert_amount_from_amount_currency_code(
        &self,
        to_currency_code: CurrencyCode,
        amount_in_amount_currency_code: Amount,
        currency_pair_price: Price,
    ) -> Result<Amount> {
        if to_currency_code == self.amount_currency_code {
            return Ok(amount_in_amount_currency_code);
        }

        if to_currency_code == self.base_currency_code {
            return Ok(amount_in_amount_currency_code / currency_pair_price);
        }

        if to_currency_code == self.quote_currency_code {
            return Ok(amount_in_amount_currency_code * currency_pair_price);
        }

        bail!("Currency code outside currency pair is not supported yet")
    }

    pub fn get_min_amount(&self, price: Price) -> Result<Amount> {
        let min_cost = match self.min_cost {
            None => {
                let min_price = match self.min_price {
                    None => match self.min_amount {
                        None => bail!("Can't calculate min amount: no data at all"),
                        Some(min_amount) => return Ok(min_amount),
                    },
                    Some(v) => v,
                };

                let min_amount = match self.min_amount {
                    None => bail!("Can't calculate min amount: missing min_amount and min_cost"),
                    Some(v) => v,
                };

                min_price * min_amount
            }
            Some(v) => v,
        };

        let min_amount_from_cost = min_cost / price;
        let rounded_amount = self.amount_round(min_amount_from_cost, Round::Ceiling)?;

        Ok(match self.min_amount {
            None => rounded_amount,
            Some(min_amount) => min_amount.max(rounded_amount),
        })
    }
}

impl Exchange {
    pub fn get_currency_pair_metadata(
        &self,
        currency_pair: &CurrencyPair,
    ) -> Result<Arc<CurrencyPairMetadata>> {
        let maybe_currency_pair_metadata = self.symbols.get(currency_pair);
        match maybe_currency_pair_metadata {
            Some(suitable_currency_pair_metadata) => Ok(suitable_currency_pair_metadata.clone()),
            None => bail!(
                "Unsupported currency pair on {} {:?}",
                self.exchange_account_id,
                currency_pair
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn get_commission_currency_code_from_balance() {
        let base_currency = "PHB";
        let quote_currency = "PHB";
        let price_precision = 0;
        let amount_precision = 0;
        let price_tick = dec!(0.1);
        let is_derivative = false;
        let balance_currency_code = CurrencyCode::new("ETH".into());

        let currency_pair_metadata = CurrencyPairMetadata::new(
            false,
            is_derivative,
            base_currency.into(),
            base_currency.into(),
            quote_currency.into(),
            quote_currency.into(),
            None,
            None,
            price_precision,
            PrecisionType::ByFraction,
            Some(price_tick),
            base_currency.into(),
            None,
            None,
            amount_precision,
            PrecisionType::ByFraction,
            None,
            None,
            Some(balance_currency_code.clone()),
        );

        let gotten = currency_pair_metadata.get_commission_currency_code(OrderSide::Buy);
        assert_eq!(gotten, balance_currency_code);
    }

    use rstest::rstest;
    use rust_decimal::Decimal;

    #[rstest]
    #[case(dec!(123.456), 2, Round::Floor, dec!(123.45))]
    #[case(dec!(12.3456), 2, Round::Floor, dec!(12.34))]
    #[case(dec!(0), 2, Round::Floor, dec!(0))]
    #[case(dec!(0.01234), 2, Round::Floor, dec!(0.01))]
    #[case(dec!(0.01234), 3, Round::Floor, dec!(0.012))]
    #[case(dec!(123.456), -1, Round::Floor, dec!(120))]
    #[case(dec!(123.456), 0, Round::Floor, dec!(123))]
    #[case(dec!(123.456), 2, Round::Ceiling, dec!(123.46))]
    #[case(dec!(12.3456), 2, Round::Ceiling, dec!(12.35))]
    #[case(dec!(0), 2, Round::Ceiling, dec!(0))]
    #[case(dec!(0.01234), 2, Round::Ceiling, dec!(0.02))]
    #[case(dec!(0.01234), 3, Round::Ceiling, dec!(0.013))]
    #[case(dec!(123.456), -1, Round::Ceiling, dec!(130))]
    #[case(dec!(123.456), 0, Round::Ceiling, dec!(124))]
    #[case(dec!(123.456), 2, Round::ToNearest, dec!(123.46))]
    #[case(dec!(12.3456), 2, Round::ToNearest, dec!(12.35))]
    #[case(dec!(0), 2, Round::ToNearest, dec!(0))]
    #[case(dec!(0.01234), 2, Round::ToNearest, dec!(0.01))]
    #[case(dec!(0.01234), 3, Round::ToNearest, dec!(0.012))]
    #[case(dec!(123.456), -1, Round::ToNearest, dec!(120))]
    #[case(dec!(123.456), 0, Round::ToNearest, dec!(123))]
    fn round_by_fraction(
        #[case] value: Decimal,
        #[case] precision: i8,
        #[case] round_to: Round,
        #[case] expected: Decimal,
    ) {
        let rounded = CurrencyPairMetadata::round_by_fraction(value, precision, round_to);
        assert_eq!(rounded, expected);
    }

    #[rstest]
    #[case(dec!(123.456), 5, Round::Floor, dec!(123.45))]
    #[case(dec!(12.34567), 5, Round::Floor, dec!(12.345))]
    #[case(dec!(0.0123456), 5, Round::Floor, dec!(0.012345))]
    #[case(dec!(0.0123456), 1, Round::Floor, dec!(0.01))]
    #[case(dec!(0.00123456), 2, Round::Floor, dec!(0.0012))]
    #[case(dec!(123.456), 4, Round::Floor, dec!(123.4))]
    #[case(dec!(123.456), 2, Round::Floor, dec!(120))]
    #[case(dec!(0), 5, Round::Floor, dec!(0))]
    #[case(dec!(123.456), 5, Round::Ceiling, dec!(123.46))]
    #[case(dec!(12.34567), 5, Round::Ceiling, dec!(12.346))]
    #[case(dec!(0.0123456), 5, Round::Ceiling, dec!(0.012346))]
    #[case(dec!(0.0123456), 1, Round::Ceiling, dec!(0.02))]
    #[case(dec!(0.00123456), 2, Round::Ceiling, dec!(0.0013))]
    #[case(dec!(123.456), 4, Round::Ceiling, dec!(123.5))]
    #[case(dec!(123.456), 2, Round::Ceiling, dec!(130))]
    #[case(dec!(0), 5, Round::Ceiling, dec!(0))]
    #[case(dec!(123.456), 5, Round::ToNearest, dec!(123.46))]
    #[case(dec!(12.34567), 5, Round::ToNearest, dec!(12.346))]
    #[case(dec!(0.0123456), 5, Round::ToNearest, dec!(0.012346))]
    #[case(dec!(0.0123456), 1, Round::ToNearest, dec!(0.01))]
    #[case(dec!(0.00123456), 2, Round::ToNearest, dec!(0.0012))]
    #[case(dec!(123.456), 4, Round::ToNearest, dec!(123.5))]
    #[case(dec!(123.456), 2, Round::ToNearest, dec!(120))]
    #[case(dec!(0), 5, Round::ToNearest, dec!(0))]
    fn round_by_mantissa(
        #[case] value: Decimal,
        #[case] precision: i8,
        #[case] round_to: Round,
        #[case] expected: Decimal,
    ) -> Result<()> {
        let rounded = CurrencyPairMetadata::round_by_mantissa(value, precision, round_to)?;

        assert_eq!(rounded, expected);

        Ok(())
    }

    #[rstest]
    #[case(dec!(123.456), 0, Round::Floor)]
    #[case(dec!(123.456), -1, Round::Floor)]
    #[case(dec!(123.456), 0, Round::Ceiling)]
    #[case(dec!(123.456), -1, Round::Ceiling)]
    #[case(dec!(123.456), 0, Round::ToNearest)]
    #[case(dec!(123.456), -1, Round::ToNearest)]
    fn round_by_mantissa_invalid_precision(
        #[case] value: Decimal,
        #[case] precision: i8,
        #[case] round_to: Round,
    ) {
        let rounded = CurrencyPairMetadata::round_by_mantissa(value, precision, round_to);

        assert!(rounded.is_err());
    }

    #[test]
    fn too_small_tick() {
        let value = dec!(123.456);
        let tick = dec!(-0.1);

        let maybe_error = CurrencyPairMetadata::round_by_tick(value, tick, Round::Floor);

        match maybe_error {
            Ok(_) => assert!(false),
            Err(error) => {
                assert_eq!("Too small tick: -0.1", &error.to_string()[..20]);
            }
        }
    }

    #[rstest]
    #[case(dec!(123.456), dec!(0.1), Round::Floor, dec!(123.4))]
    #[case(dec!(123.456), dec!(0.4), Round::Floor, dec!(123.2))]
    #[case(dec!(123.456), dec!(0.03), Round::Floor, dec!(123.45))]
    #[case(dec!(123.456), dec!(2), Round::Floor, dec!(122))]
    #[case(dec!(0), dec!(0.03), Round::Floor, dec!(0))]
    #[case(dec!(123.456), dec!(0.1), Round::Ceiling, dec!(123.5))]
    #[case(dec!(123.456), dec!(0.4), Round::Ceiling, dec!(123.6))]
    #[case(dec!(123.456), dec!(0.03), Round::Ceiling, dec!(123.48))]
    #[case(dec!(123.456), dec!(2), Round::Ceiling, dec!(124))]
    #[case(dec!(0), dec!(0.03), Round::Ceiling, dec!(0))]
    #[case(dec!(123.456), dec!(0.1), Round::ToNearest, dec!(123.5))]
    #[case(dec!(123.456), dec!(0.4), Round::ToNearest, dec!(123.6))]
    #[case(dec!(123.456), dec!(0.03), Round::ToNearest, dec!(123.45))]
    #[case(dec!(123.456), dec!(0.01), Round::ToNearest, dec!(123.46))]
    #[case(dec!(123.456), dec!(2), Round::ToNearest, dec!(124))]
    #[case(dec!(0), dec!(0.03), Round::ToNearest, dec!(0))]
    fn round_by_tick(
        #[case] value: Decimal,
        #[case] tick: Decimal,
        #[case] round_to: Round,
        #[case] expected: Decimal,
    ) -> Result<()> {
        let rounded = CurrencyPairMetadata::round_by_tick(value, tick, round_to)?;

        assert_eq!(rounded, expected);

        Ok(())
    }
}
