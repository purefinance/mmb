use mmb_database::impl_event;
use mmb_domain::market::CurrencyPair;
use mmb_domain::market::ExchangeId;
use mmb_domain::order::snapshot::Price;
use mmb_utils::DateTime;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PriceSourceModel {
    pub init_time: DateTime,
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
    pub bid: Option<Price>,
    pub ask: Option<Price>,
}

impl_event!(PriceSourceModel, "price_sources");

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
