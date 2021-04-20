use crate::core::{
    exchanges::common::Amount,
    exchanges::common::CurrencyCode,
    exchanges::common::CurrencyId,
    exchanges::common::{CurrencyPair, Price},
    orders::order::OrderSide,
};
use anyhow::{bail, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;

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

        Self::inner_round_by_tick(value, tick, round)
    }

    fn inner_round_by_tick(value: Price, tick: Price, round: Round) -> Result<Price> {
        let floor = (value / tick).floor() * tick;
        let ceil = (value / tick).ceil() * tick;
        match round {
            Round::Floor => Ok(floor),
            Round::Ceiling => Ok(ceil),
            Round::ToNearest => {
                let mut result = floor;
                if ceil - value <= value - floor {
                    result = ceil;
                }

                return Ok(result);
            }
        }
    }

    fn round_by_mantissa(value: Price, precision: i8, round: Round) -> Result<Price> {
        if value == dec!(0) {
            return Ok(dec!(0));
        }

        let floor_digits = Self::get_precision_digits_by_fractional(value, precision)?;

        Self::round_by_fraction(value, floor_digits, round)
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

    fn powi(value: Decimal, degree: i8) -> Decimal {
        match degree {
            std::i8::MIN..=-1 => {
                let mut result = dec!(1);
                let degree = -degree;

                for _ in 0..degree {
                    result *= value;
                }
                dec!(1) / result
            }
            _ => value.powi(degree as u64),
        }
    }

    fn round_by_fraction(value: Price, precision: i8, round: Round) -> Result<Price> {
        let multiplier = dec!(0.1);
        let pow_precision = Self::powi(multiplier, precision);

        Self::inner_round_by_tick(value, pow_precision, round)
    }

    pub fn get_commision_currency_code(&self, side: OrderSide) -> CurrencyCode {
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

        let gotten = currency_pair_metadata.get_commision_currency_code(OrderSide::Buy);
        assert_eq!(gotten, balance_currency_code);
    }

    mod custom_powi {
        use super::*;

        #[test]
        fn first() -> Result<()> {
            let value = dec!(0.1);
            let precision = -1;

            let powered = CurrencyPairMetadata::powi(value, precision);

            let right_value = dec!(10);
            assert_eq!(powered, right_value);

            Ok(())
        }

        #[test]
        fn second() -> Result<()> {
            let value = dec!(0.1);
            let precision = -6;

            let powered = CurrencyPairMetadata::powi(value, precision);

            let right_value = dec!(1000000);
            assert_eq!(powered, right_value);

            Ok(())
        }

        #[test]
        fn third() -> Result<()> {
            let value = dec!(1.6);
            let precision = 2;

            let powered = CurrencyPairMetadata::powi(value, precision);

            let right_value = dec!(2.56);
            assert_eq!(powered, right_value);

            Ok(())
        }
    }

    mod round_by_fraction {
        use super::*;

        mod floor {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Floor)?;

                let right_value = dec!(123.45);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(12.3456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Floor)?;

                let right_value = dec!(12.34);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(0);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Floor)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(0.01234);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Floor)?;

                let right_value = dec!(0.01);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0.01234);
                let precision = 3;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Floor)?;

                let right_value = dec!(0.012);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn sixth() -> Result<()> {
                let value = dec!(123.456);
                let precision = -1;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Floor)?;

                let right_value = dec!(120);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn seventh() -> Result<()> {
                let value = dec!(123.456);
                let precision = 0;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Floor)?;

                let right_value = dec!(123);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }

        mod ceiling {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Ceiling)?;

                let right_value = dec!(123.46);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(12.3456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Ceiling)?;

                let right_value = dec!(12.35);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(0);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Ceiling)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(0.01234);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Ceiling)?;

                let right_value = dec!(0.02);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0.01234);
                let precision = 3;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Ceiling)?;

                let right_value = dec!(0.013);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn sixth() -> Result<()> {
                let value = dec!(123.456);
                let precision = -1;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Ceiling)?;

                let right_value = dec!(130);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn seventh() -> Result<()> {
                let value = dec!(123.456);
                let precision = 0;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::Ceiling)?;

                let right_value = dec!(124);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }

        mod to_nearest {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::ToNearest)?;

                let right_value = dec!(123.46);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(12.3456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::ToNearest)?;

                let right_value = dec!(12.35);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(0);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::ToNearest)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(0.01234);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::ToNearest)?;

                let right_value = dec!(0.01);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0.01234);
                let precision = 3;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::ToNearest)?;

                let right_value = dec!(0.012);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn sixth() -> Result<()> {
                let value = dec!(123.456);
                let precision = -1;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::ToNearest)?;

                let right_value = dec!(120);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn seventh() -> Result<()> {
                let value = dec!(123.456);
                let precision = 0;

                let rounded =
                    CurrencyPairMetadata::round_by_fraction(value, precision, Round::ToNearest)?;

                let right_value = dec!(123);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }
    }

    mod round_by_mantissa {
        use super::*;

        mod floor {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(123.45);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(12.34567);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(12.345);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(0.0123456);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(0.012345);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(0.0123456);
                let precision = 1;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(0.01);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0.00123456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(0.0012);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn sixth() -> Result<()> {
                let value = dec!(123.456);
                let precision = 4;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(123.4);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn seventh() -> Result<()> {
                let value = dec!(123.456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(120);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn eighth() -> Result<()> {
                let value = dec!(0);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }

        mod ceiling {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(123.46);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(12.34567);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(12.346);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(0.0123456);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(0.012346);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(0.0123456);
                let precision = 1;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(0.02);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0.00123456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(0.0013);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn sixth() -> Result<()> {
                let value = dec!(123.456);
                let precision = 4;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(123.5);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn seventh() -> Result<()> {
                let value = dec!(123.456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(130);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn eighth() -> Result<()> {
                let value = dec!(0);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }

        mod to_nearest {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(123.46);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(12.34567);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(12.346);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(0.01234567);
                let precision = 5;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(0.012346);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(0.0123456);
                let precision = 1;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(0.01);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0.00123456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(0.0012);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn sixth() -> Result<()> {
                let value = dec!(123.456);
                let precision = 4;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(123.5);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn seventh() -> Result<()> {
                let value = dec!(123.456);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(120);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn eight() -> Result<()> {
                let value = dec!(0);
                let precision = 2;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }

        mod invalid_precision {
            use super::*;

            #[test]
            fn first() {
                let value = dec!(123.456);
                let precision = 0;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor);

                assert!(rounded.is_err())
            }

            #[test]
            fn second() {
                let value = dec!(123.456);
                let precision = -1;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Floor);

                assert!(rounded.is_err())
            }

            #[test]
            fn third() {
                let value = dec!(123.456);
                let precision = 0;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling);

                assert!(rounded.is_err())
            }

            #[test]
            fn fourth() {
                let value = dec!(123.456);
                let precision = -1;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::Ceiling);

                assert!(rounded.is_err())
            }

            #[test]
            fn fifth() {
                let value = dec!(123.456);
                let precision = 0;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest);

                assert!(rounded.is_err())
            }

            #[test]
            fn sixth() {
                let value = dec!(123.456);
                let precision = -1;

                let rounded =
                    CurrencyPairMetadata::round_by_mantissa(value, precision, Round::ToNearest);

                assert!(rounded.is_err())
            }
        }
    }

    mod round_by_tick {
        use super::*;

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

        mod floor {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.1);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Floor)?;

                let right_value = dec!(123.4);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.4);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Floor)?;

                let right_value = dec!(123.2);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.03);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Floor)?;

                let right_value = dec!(123.45);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(2);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Floor)?;

                let right_value = dec!(122);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0);
                let tick = dec!(0.03);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Floor)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }

        mod ceiling {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.1);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Ceiling)?;

                let right_value = dec!(123.5);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.4);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Ceiling)?;

                let right_value = dec!(123.6);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.03);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Ceiling)?;

                let right_value = dec!(123.48);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(2);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Ceiling)?;

                let right_value = dec!(124);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(0);
                let tick = dec!(0.03);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::Ceiling)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }

        mod to_nearest {
            use super::*;

            #[test]
            fn first() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.1);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::ToNearest)?;

                let right_value = dec!(123.5);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn second() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.4);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::ToNearest)?;

                let right_value = dec!(123.6);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn third() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.03);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::ToNearest)?;

                let right_value = dec!(123.45);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fourth() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(0.01);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::ToNearest)?;

                let right_value = dec!(123.46);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn fifth() -> Result<()> {
                let value = dec!(123.456);
                let tick = dec!(2);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::ToNearest)?;

                let right_value = dec!(124);
                assert_eq!(rounded, right_value);

                Ok(())
            }

            #[test]
            fn sixth() -> Result<()> {
                let value = dec!(0);
                let tick = dec!(0.03);

                let rounded = CurrencyPairMetadata::round_by_tick(value, tick, Round::ToNearest)?;

                let right_value = dec!(0);
                assert_eq!(rounded, right_value);

                Ok(())
            }
        }
    }
}
