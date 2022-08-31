use std::hash::{Hash, Hasher};

use crate::market::{powi, CurrencyCode, CurrencyId, CurrencyPair};
use crate::order::snapshot::OrderSide;
use crate::order::snapshot::{Amount, Price};
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use serde::Serialize;

pub enum Round {
    Floor,
    Ceiling,
    ToNearest,
}

// TODO Change to Maker-Taker
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BeforeAfter {
    Before,
    After,
}

/// Precision this is type that describes Decimal value rounding(now is using for rounding amount in orders)
/// NOTE: Old ByFraction variant can be written as tick == 0.1^by_fraction_precision
/// ```ignore
/// Precision::ByTick { tick: dec!(0.001) } // for AmountPrecision = 3 equal pow(0.1, 3)
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize)]
pub enum Precision {
    /// Rounding is performed to a number divisible to the specified tick
    /// Look at round_by_tick test below
    ByTick { tick: Decimal },
    /// Rounding is performed to a number of digits located on `precision` length to the right of start of mantissa
    /// Look at round_by_mantissa test below
    ByMantissa { precision: u8 },
}

impl Precision {
    /// Converting ticks from decimal places of a number
    pub fn tick_from_precision(precision: i8) -> Precision {
        Precision::ByTick {
            tick: dec!(0.1).powi(precision as i64),
        }
    }
}

/// Metadata for a currency pair
#[derive(Debug, Clone, Eq, Serialize)]
pub struct Symbol {
    pub is_derivative: bool,
    pub base_currency_id: CurrencyId,
    pub base_currency_code: CurrencyCode,
    pub quote_currency_id: CurrencyId,
    pub quote_currency_code: CurrencyCode,
    pub min_price: Option<Price>,
    pub max_price: Option<Price>,
    // min amount that Exchange can accept we can't change this field
    pub min_amount: Option<Amount>,
    // max amount that Exchange can accept we can't change this field
    pub max_amount: Option<Amount>,
    pub min_cost: Option<Price>,
    pub amount_currency_code: CurrencyCode,
    pub balance_currency_code: Option<CurrencyCode>,
    pub amount_multiplier: Decimal,

    pub price_precision: Precision,
    pub amount_precision: Precision,
}

impl Symbol {
    pub fn base_currency_code(&self) -> CurrencyCode {
        self.base_currency_code
    }

    pub fn quote_currency_code(&self) -> CurrencyCode {
        self.quote_currency_code
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        is_derivative: bool,
        base_currency_id: CurrencyId,
        base_currency_code: CurrencyCode,
        quote_currency_id: CurrencyId,
        quote_currency_code: CurrencyCode,
        min_price: Option<Price>,
        max_price: Option<Price>,
        min_amount: Option<Amount>,
        max_amount: Option<Amount>,
        min_cost: Option<Price>,
        amount_currency_code: CurrencyCode,
        balance_currency_code: Option<CurrencyCode>,
        price_precision: Precision,
        amount_precision: Precision,
    ) -> Self {
        Self {
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
            amount_multiplier: dec!(1),
            price_precision,
            amount_precision,
        }
    }

    // Currency pair in unified for crate format
    pub fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes(self.base_currency_code, self.quote_currency_code)
    }

    pub fn get_trade_code(&self, side: OrderSide, before_after: BeforeAfter) -> CurrencyCode {
        use BeforeAfter::*;
        use OrderSide::*;

        match (before_after, side) {
            (Before, Buy) => self.quote_currency_code,
            (Before, Sell) => self.base_currency_code,
            (After, Buy) => self.base_currency_code,
            (After, Sell) => self.quote_currency_code,
        }
    }

    pub fn is_derivative(&self) -> bool {
        self.is_derivative
    }

    pub fn price_round(&self, price: Price, round: Round) -> Price {
        match self.price_precision {
            Precision::ByTick { tick } => Self::round_by_tick(price, tick, round),
            Precision::ByMantissa { precision } => Self::round_by_mantissa(price, precision, round),
        }
    }

    pub fn amount_round(&self, amount: Amount, round: Round) -> Amount {
        match self.amount_precision {
            Precision::ByTick { tick } => Self::round_by_tick(amount, tick, round),
            Precision::ByMantissa { precision } => {
                self.amount_round_precision(amount, round, precision)
            }
        }
    }

    /// Rounding of order amount with specified precision
    pub fn amount_round_precision(
        &self,
        amount: Amount,
        round: Round,
        amount_precision: u8,
    ) -> Amount {
        match self.amount_precision {
            Precision::ByMantissa { precision: _ } => {
                Self::round_by_mantissa(amount, amount_precision, round)
            }
            Precision::ByTick { tick: _ } => {
                panic!("amount_round_precision cannot be called with Precision::ByTick variant")
            }
        }
    }

    pub fn round_to_remove_amount_precision_error(&self, amount: Amount) -> Amount {
        // allowed machine error that is less then 0.01 * amount precision
        match self.amount_precision {
            Precision::ByMantissa { precision } => {
                self.amount_round_precision(amount, Round::ToNearest, precision + 2u8)
            }
            Precision::ByTick { tick } => {
                Self::round_by_tick(amount, tick * dec!(0.01), Round::ToNearest)
            }
        }
    }

    pub fn round_to_remove_amount_precision_error_expected(&self, amount: Amount) -> Amount {
        self.round_to_remove_amount_precision_error(amount)
    }

    fn round_by_tick(value: Decimal, tick: Decimal, round: Round) -> Decimal {
        if tick <= dec!(0) {
            panic!("Too small tick: {}", tick)
        }

        Self::inner_round_by_tick(value, tick, round)
    }

    fn inner_round_by_tick(value: Decimal, tick: Decimal, round: Round) -> Decimal {
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

    fn round_by_mantissa(value: Price, precision: u8, round: Round) -> Price {
        if value.is_zero() {
            return dec!(0);
        }

        let floor_digits = Self::get_precision_digits_by_fractional(value, precision);

        Self::inner_round_by_tick(value, powi(dec!(0.1), floor_digits), round)
    }

    fn get_precision_digits_by_fractional(value: Price, precision: u8) -> i8 {
        if precision == 0 {
            panic!(
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

        precision as i8 - integral_digits
    }

    pub fn get_commission_currency_code(&self, side: OrderSide) -> CurrencyCode {
        self.balance_currency_code.unwrap_or(match side {
            OrderSide::Buy => self.base_currency_code,
            OrderSide::Sell => self.quote_currency_code,
        })
    }

    pub fn convert_amount_from_amount_currency_code(
        &self,
        to_currency_code: CurrencyCode,
        amount_in_amount_currency_code: Amount,
        currency_pair_price: Price,
    ) -> Amount {
        if to_currency_code == self.amount_currency_code {
            return amount_in_amount_currency_code;
        }

        if to_currency_code == self.base_currency_code {
            return amount_in_amount_currency_code / currency_pair_price;
        }

        if to_currency_code == self.quote_currency_code {
            return amount_in_amount_currency_code * currency_pair_price;
        }

        let currency_pair = self.currency_pair();
        panic!("Currency code {to_currency_code} outside currency pair {currency_pair} is not supported");
    }

    pub fn convert_amount_from_balance_currency_code(
        &self,
        to_currency_code: CurrencyCode,
        amount: Amount,
        currency_pair_price: Price,
    ) -> Amount {
        if Some(to_currency_code) == self.balance_currency_code {
            return amount;
        }
        if to_currency_code == self.base_currency_code {
            return amount / currency_pair_price;
        }

        if to_currency_code == self.quote_currency_code {
            return amount * currency_pair_price;
        }

        let currency_pair = self.currency_pair();
        panic!("Currency code {to_currency_code} outside currency pair {currency_pair} is not supported");
    }

    pub fn convert_amount_into_amount_currency_code(
        &self,
        from_currency_code: CurrencyCode,
        amount_in_from_currency_code: Decimal,
        currency_pair_price: Price,
    ) -> Decimal {
        if from_currency_code == self.amount_currency_code {
            return amount_in_from_currency_code;
        }

        if from_currency_code == self.base_currency_code() {
            return amount_in_from_currency_code * currency_pair_price;
        }

        if from_currency_code == self.quote_currency_code {
            return amount_in_from_currency_code / currency_pair_price;
        }

        panic!(
            "We don't currently support currency code {} outside currency pair {}",
            from_currency_code,
            self.currency_pair()
        );
    }

    pub fn get_min_amount(&self, price: Price) -> Result<Amount> {
        let min_cost = match self.min_cost {
            None => {
                let min_price = match self.min_price {
                    None => {
                        return self
                            .min_amount
                            .context("Can't calculate min amount: no data at all")
                    }
                    Some(v) => v,
                };

                let min_amount = self
                    .min_amount
                    .context("Can't calculate min amount: missing min_amount and min_cost");

                if self.is_derivative {
                    return min_amount;
                }

                min_price * min_amount?
            }
            Some(v) => v,
        };

        let min_amount_from_cost = match self.is_derivative {
            true => min_cost,
            false => min_cost / price,
        };

        let rounded_amount = self.amount_round(min_amount_from_cost, Round::Ceiling);

        Ok(match self.min_amount {
            None => rounded_amount,
            Some(min_amount) => min_amount.max(rounded_amount),
        })
    }

    pub fn get_amount_tick(&self) -> Decimal {
        match self.amount_precision {
            Precision::ByTick { tick } => tick,
            Precision::ByMantissa { precision: _ } => {
                panic!("get_amount_tick cannot be called with Precision::ByMantissa variant")
            }
        }
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Self) -> bool {
        self.currency_pair() == other.currency_pair()
    }
}

impl Hash for Symbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.currency_pair().hash(state)
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
        let price_tick = dec!(0.1);
        let is_derivative = false;
        let balance_currency_code = CurrencyCode::new("ETH");

        let symbol = Symbol::new(
            is_derivative,
            base_currency.into(),
            base_currency.into(),
            quote_currency.into(),
            quote_currency.into(),
            None,
            None,
            None,
            None,
            None,
            base_currency.into(),
            Some(balance_currency_code),
            Precision::ByTick { tick: price_tick },
            Precision::ByTick { tick: dec!(0) },
        );

        let gotten = symbol.get_commission_currency_code(OrderSide::Buy);
        assert_eq!(gotten, balance_currency_code);
    }

    use rstest::rstest;
    use rust_decimal::Decimal;

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
        #[case] precision: u8,
        #[case] round_to: Round,
        #[case] expected: Decimal,
    ) -> Result<()> {
        let rounded = Symbol::round_by_mantissa(value, precision, round_to);

        assert_eq!(rounded, expected);

        Ok(())
    }

    #[rstest]
    #[case(dec!(123.456), 0, Round::Floor)]
    #[case(dec!(123.456), 0, Round::Ceiling)]
    #[case(dec!(123.456), 0, Round::ToNearest)]
    #[should_panic]
    fn round_by_mantissa_invalid_precision(
        #[case] value: Decimal,
        #[case] precision: u8,
        #[case] round_to: Round,
    ) {
        Symbol::round_by_mantissa(value, precision, round_to);
    }

    #[test]
    #[should_panic]
    fn too_small_tick() {
        let value = dec!(123.456);
        let tick = dec!(-0.1);

        Symbol::round_by_tick(value, tick, Round::Floor);
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
        let rounded = Symbol::round_by_tick(value, tick, round_to);

        assert_eq!(rounded, expected);

        Ok(())
    }

    #[test]
    pub fn get_trade_code() {
        let base_currency = "PHB";
        let quote_currency = "BTC";
        let price_tick = dec!(0.1);
        let is_derivative = false;
        let balance_currency_code = CurrencyCode::new("ETH");

        let base_code = CurrencyCode::new(base_currency);
        let quote_code = CurrencyCode::new(quote_currency);
        let symbol = Symbol::new(
            is_derivative,
            base_currency.into(),
            base_code,
            quote_currency.into(),
            quote_code,
            None,
            None,
            None,
            None,
            None,
            base_code,
            Some(balance_currency_code),
            Precision::ByTick { tick: price_tick },
            Precision::ByTick { tick: dec!(0) },
        );

        assert_eq!(
            symbol.get_trade_code(OrderSide::Buy, BeforeAfter::After),
            base_code
        );
        assert_eq!(
            symbol.get_trade_code(OrderSide::Buy, BeforeAfter::Before),
            quote_code
        );
        assert_eq!(
            symbol.get_trade_code(OrderSide::Sell, BeforeAfter::After),
            quote_code
        );
        assert_eq!(
            symbol.get_trade_code(OrderSide::Sell, BeforeAfter::Before),
            base_code
        );
    }
}
