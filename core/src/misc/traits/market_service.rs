use std::sync::Arc;

use async_trait::async_trait;

use crate::services::market_prices::market_currency_code_price::MarketCurrencyCodePrice;

#[async_trait]
pub trait GetMarketCurrencyCodePrice: Send + Sync {
    async fn get_market_currency_code_price(&self) -> Vec<MarketCurrencyCodePrice>;
}

pub trait CreateMarketService {
    fn new() -> Arc<Self>;
}
