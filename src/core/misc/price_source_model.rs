use crate::core::{
    exchanges::common::{CurrencyPair, ExchangeId, Price},
    DateTime,
};

pub(crate) struct PriceSourceModel {
    /// this is the date of the last save in the DB
    pub save_date: DateTime,
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
    pub bid: Option<Price>,
    pub ask: Option<Price>,
}

impl PriceSourceModel {
    pub fn new(
        save_date: DateTime,
        exchange_id: ExchangeId,
        currency_pair: CurrencyPair,
        bid: Option<Price>,
        ask: Option<Price>,
    ) -> Self {
        Self {
            save_date,
            exchange_id,
            currency_pair,
            bid,
            ask,
        }
    }
}
