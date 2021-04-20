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

pub(crate) fn powi(value: Decimal, degree: i8) -> Decimal {
    if degree < 0 {
        let degree = -degree;

        let result = value.powi(degree as u64);
        dec!(1) / result
    } else {
        value.powi(degree as u64)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    mod custom_powi {
        use super::*;
        use rust_decimal_macros::dec;

        #[test]
        fn first() {
            let value = dec!(0.1);
            let degree = -1;

            let powered = powi(value, degree);

            let right_value = dec!(10);
            assert_eq!(powered, right_value);
        }

        #[test]
        fn second() {
            let value = dec!(0.1);
            let degree = -6;

            let powered = powi(value, degree);

            let right_value = dec!(1000000);
            assert_eq!(powered, right_value);
        }

        #[test]
        fn third() {
            let value = dec!(1.6);
            let degree = 2;

            let powered = powi(value, degree);

            let right_value = dec!(2.56);
            assert_eq!(powered, right_value);
        }
    }
}
