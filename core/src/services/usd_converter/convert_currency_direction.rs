use crate::exchanges::common::CurrencyCode;

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct ConvertCurrencyDirection {
    pub from: CurrencyCode,
    pub to: CurrencyCode,
}

impl ConvertCurrencyDirection {
    pub fn new(from: CurrencyCode, to: CurrencyCode) -> Self {
        Self { from, to }
    }
}
