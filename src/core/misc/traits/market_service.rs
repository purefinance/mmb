use std::sync::Arc;

use async_trait::async_trait;

use crate::core::services::market_prices::market_symbol_price::MarketSymbolPrice;

#[async_trait]
pub trait MarketService {
    async fn get_market_symbol_price(&self) -> Vec<MarketSymbolPrice>;
}

pub trait NewMarketService {
    fn new() -> Arc<dyn MarketService + Send + Sync>;
}
