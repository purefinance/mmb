use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};

pub struct CurrencyPriceSourceSettings {
    pub start_currency_code: CurrencyCode,
    pub end_currency_code: CurrencyCode,
    /// List of pairs ExchangeName and CurrencyCodePairs for translation currency with StartCurrencyCode to currency with EndCurrencyCode
    pub exchange_id_currency_code_pair_settings: Vec<ExchangeIdCurrencyCodePairSettings>,
}

pub struct ExchangeIdCurrencyCodePairSettings {
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
}
