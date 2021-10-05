use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};

pub struct CurrencyPriceSourceSettings {
    pub start_currency_code: CurrencyCode,
    pub end_currency_code: CurrencyCode,
    /// List of pairs ExchangeId and CurrencyPairs for translation currency with StartCurrencyCode to currency with EndCurrencyCode
    pub exchange_id_currency_pair_settings: Vec<ExchangeIdCurrencyPairSettings>,
}

impl CurrencyPriceSourceSettings {
    pub fn new(
        start_currency_code: CurrencyCode,
        end_currency_code: CurrencyCode,
        exchange_id_currency_pair_settings: Vec<ExchangeIdCurrencyPairSettings>,
    ) -> Self {
        Self {
            start_currency_code,
            end_currency_code,
            exchange_id_currency_pair_settings,
        }
    }
}

pub struct ExchangeIdCurrencyPairSettings {
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
}
