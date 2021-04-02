use crate::core::{
    exchanges::common::Amount,
    exchanges::common::CurrencyCode,
    exchanges::common::{CurrencyPair, ExchangeAccountId, Price},
    orders::order::OrderSide,
};

pub struct CurrencyPairMetadata {
    pub base_currency_code: CurrencyCode,
    pub quote_currency_code: CurrencyCode,
}

impl CurrencyPairMetadata {
    pub fn new(exchange_order_id: ExchangeAccountId, currency_pair: CurrencyPair) -> Self {
        Self {
            base_currency_code: CurrencyCode::new("base".into()),
            quote_currency_code: CurrencyCode::new("quot".into()),
        }
    }

    pub fn is_derivative(&self) -> bool {
        true
    }

    // TODO second params is round
    pub fn price_round(&self, price: Price) -> Price {
        price
    }

    // TODO is that appropriate return type?
    pub fn get_commision_currency_code(&self, side: OrderSide) -> CurrencyCode {
        CurrencyCode::new("test".into())
    }

    pub fn convert_amount_from_amount_currency_code(
        &self,
        to_currency_code: CurrencyCode,
        amount_in_amount_currency_code: Amount,
        currency_pair_price: Price,
    ) -> Amount {
        amount_in_amount_currency_code
    }
}
