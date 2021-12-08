use crate::core::{
    exchanges::common::{CurrencyPair, ExchangeId, Price},
    DateTime,
};

pub(crate) struct PriceSourceModel {
    pub init_time: DateTime,
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
    pub bid: Option<Price>,
    pub ask: Option<Price>,
}

impl PriceSourceModel {
    pub fn new(
        init_time: DateTime,
        exchange_id: ExchangeId,
        currency_pair: CurrencyPair,
        bid: Option<Price>,
        ask: Option<Price>,
    ) -> Self {
        Self {
            init_time,
            exchange_id,
            currency_pair,
            bid,
            ask,
        }
    }
}
