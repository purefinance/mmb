use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub trait ConvertPercentToRate {
    fn percent_to_rate(&self) -> Decimal;
}

impl ConvertPercentToRate for Decimal {
    fn percent_to_rate(&self) -> Decimal {
        let proportion_multiplier = dec!(0.01);
        self * proportion_multiplier
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    use domain::market::powi;
    use rust_decimal_macros::dec;

    #[rstest]
    #[case(dec!(0.1), -1, dec!(10))]
    #[case(dec!(0.1), -6, dec!(1000000))]
    #[case(dec!(1.6), 2, dec!(2.56))]
    fn custom_powi(#[case] value: Decimal, #[case] degree: i8, #[case] expected: Decimal) {
        let powered = powi(value, degree);

        assert_eq!(powered, expected);
    }
}
