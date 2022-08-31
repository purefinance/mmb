use domain::market::ExchangeId;
use domain::order::snapshot::Price;
use mmb_utils::DateTime;

use domain::market::CurrencyPair;

pub(crate) struct PriceSourceModel {
    pub _init_time: DateTime,
    pub _exchange_id: ExchangeId,
    pub _currency_pair: CurrencyPair,
    pub _bid: Option<Price>,
    pub _ask: Option<Price>,
}

impl PriceSourceModel {
    pub fn new(
        _init_time: DateTime,
        _exchange_id: ExchangeId,
        _currency_pair: CurrencyPair,
        _bid: Option<Price>,
        _ask: Option<Price>,
    ) -> Self {
        Self {
            _init_time,
            _exchange_id,
            _currency_pair,
            _bid,
            _ask,
        }
    }
}
