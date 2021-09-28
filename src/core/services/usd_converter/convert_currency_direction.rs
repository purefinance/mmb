use crate::core::exchanges::common::CurrencyCode;

#[derive(Eq, PartialEq, Hash)]
pub(crate) struct ConvertCurrencyDirection {
    pub from_currency_code: CurrencyCode,
    pub to_currency_code: CurrencyCode,
}

impl ConvertCurrencyDirection {
    pub fn new(from_currency_code: CurrencyCode, to_currency_code: CurrencyCode) -> Self {
        Self {
            from_currency_code,
            to_currency_code,
        }
    }
}
