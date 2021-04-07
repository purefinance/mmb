use crate::core::{
    exchanges::common::Amount,
    exchanges::common::CurrencyCode,
    exchanges::common::CurrencyId,
    exchanges::common::SpecificCurrencyPair,
    exchanges::common::{CurrencyPair, ExchangeAccountId, Price},
    orders::order::OrderSide,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PrecisionType {
    ByFraction,
    ByMantissa,
}

// FIXME Strange name, need to fix
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BeforeAfter {
    Before,
    After,
}

pub const SYMBOL_DEFAULT_PRECISION: i8 = i8::MAX;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub is_active: bool,
    pub is_derivative: bool,
    pub base_currency_id: CurrencyId,
    pub base_currency_code: CurrencyCode,
    pub quote_currency_id: CurrencyId,
    pub quote_currency_code: CurrencyCode,
    // Currency pair in specific for exchange (which related to symbol)
    pub specific_currency_pair: SpecificCurrencyPair,
    pub min_price: Option<Price>,
    pub max_price: Option<Price>,
    pub price_precision: i8,
    pub price_precision_type: PrecisionType,
    pub price_tick: Option<Price>,
    pub amount_currency_code: CurrencyCode,
    pub min_amount: Option<Amount>,
    pub max_amount: Option<Amount>,
    pub amount_precision: i8,
    pub amount_precision_type: PrecisionType,
    pub amount_tick: Option<Amount>,
    pub min_cost: Option<Price>,
    pub balance_currency_code: Option<CurrencyCode>,
}

impl Symbol {
    // Currency pair in unified for crate format
    pub fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_currency_codes(
            self.base_currency_code.clone(),
            self.quote_currency_code.clone(),
        )
    }

    pub fn get_trade_code(&self, side: OrderSide, before_after: BeforeAfter) -> CurrencyCode {
        use BeforeAfter::*;
        use OrderSide::*;

        match (before_after, side) {
            (Before, Buy) => self.quote_currency_code.clone(),
            (Before, Sell) => self.base_currency_code.clone(),
            (After, Buy) => self.base_currency_code.clone(),
            (After, Sell) => self.quote_currency_code.clone(),
        }
    }
}

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
        false
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
