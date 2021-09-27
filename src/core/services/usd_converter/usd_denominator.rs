use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use itertools::Itertools;
use tokio::sync::Mutex;

use crate::{
    core::{
        exchanges::common::{Amount, CurrencyCode, Price},
        lifecycle::application_manager::ApplicationManager,
        misc::{
            safe_timer::{SafeTimer, TimerAction},
            traits::market_service::{CreateMarketService, GetMarketCurrencyCodePrice},
        },
        services::market_prices::market_currency_code_price::MarketCurrencyCodePrice,
    },
    hashmap,
};

pub struct UsdDenominator {
    market_service: Arc<dyn GetMarketCurrencyCodePrice + Send + Sync>,
    application_manager: Arc<ApplicationManager>,
    market_prices_by_symbol: HashMap<CurrencyCode, MarketCurrencyCodePrice>,
    /// must be panic safety(look at comment for TimerAction)
    pub price_update_callback: fn() -> Result<()>,
    pub refresh_timer: Option<Arc<Mutex<SafeTimer>>>,
}

#[async_trait]
impl TimerAction for UsdDenominator {
    async fn timer_action(&mut self) -> Result<()> {
        let market_prices = self.market_service.get_market_currency_code_price().await;
        self.market_prices_by_symbol = UsdDenominator::to_prices_dictionary(market_prices);
        (self.price_update_callback)()
    }
}

impl UsdDenominator {
    fn to_prices_dictionary(
        tickers: Vec<MarketCurrencyCodePrice>,
    ) -> HashMap<CurrencyCode, MarketCurrencyCodePrice> {
        tickers
            .iter()
            .map(|x| (x.symbol.clone(), x.clone()))
            .collect()
    }

    fn new(
        market_service: Arc<dyn GetMarketCurrencyCodePrice + Send + Sync>,
        market_prices: Vec<MarketCurrencyCodePrice>,
        auto_refresh_data: bool,
        application_manager: Arc<ApplicationManager>,
    ) -> Arc<Mutex<Self>> {
        let this = Arc::new(Mutex::new(Self {
            market_service,
            application_manager: application_manager.clone(),
            market_prices_by_symbol: UsdDenominator::to_prices_dictionary(market_prices),
            price_update_callback: || Ok(()),
            refresh_timer: None,
        }));

        if auto_refresh_data {
            let cloned_this = this.clone();
            this.try_lock().expect("msg").refresh_timer = Some(SafeTimer::new(
                cloned_this as Arc<Mutex<dyn TimerAction + Send>>,
                "UsdDenominator::refresh_data()".into(),
                Duration::from_secs(7200), // 2 hours
                application_manager,
                true,
            ));
        }

        this
    }

    pub async fn create_async<T>(
        auto_refresh_data: bool,
        application_manager: Arc<ApplicationManager>,
    ) -> Arc<Mutex<Self>>
    where
        T: GetMarketCurrencyCodePrice + CreateMarketService,
    {
        let service = T::new();
        let market_prices = service.get_market_currency_code_price().await;
        UsdDenominator::new(
            service,
            market_prices,
            auto_refresh_data,
            application_manager,
        )
    }

    pub fn get_non_refreshing_usd_denominator(&self) -> Arc<Mutex<Self>> {
        UsdDenominator::new(
            self.market_service.clone(),
            self.market_prices_by_symbol.values().cloned().collect_vec(),
            false,
            self.application_manager.clone(),
        )
    }

    fn currency_code_exceptions() -> HashMap<CurrencyCode, CurrencyCode> {
        hashmap![CurrencyCode::from( "IOTA") => CurrencyCode::from("MIOTA")]
    }

    pub fn get_all_prices_in_usd(&self) -> HashMap<CurrencyCode, Price> {
        let mut result: HashMap<_, _> = self
            .market_prices_by_symbol
            .iter()
            .filter(|(_, v)| v.price_usd.is_some())
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.price_usd.expect("cannot be None: filter is broken"),
                )
            })
            .collect();

        for (source_code, mapped_code) in UsdDenominator::currency_code_exceptions() {
            if let Some(price_in_usd) = result.get(&mapped_code).cloned() {
                *result
                    .get_mut(&source_code)
                    .expect("failed to get value from dictionary by source_code") = price_in_usd;
                let _ = result.remove(&mapped_code);
            }
        }

        result
    }

    pub fn get_price_in_usd(&self, currency_code: &CurrencyCode) -> Option<Price> {
        let currency_code = UsdDenominator::currency_code_exceptions()
            .get(currency_code)
            .cloned()
            .unwrap_or(currency_code.clone());
        self.market_prices_by_symbol.get(&currency_code)?.price_usd
    }

    pub fn usd_to_currency(
        &self,
        currency_code: &CurrencyCode,
        amount_in_usd: Amount,
    ) -> Option<Amount> {
        Some(amount_in_usd / self.get_price_in_usd(currency_code)?)
    }

    pub fn currency_to_usd(
        &self,
        currency_code: &CurrencyCode,
        amount_in_base: Amount,
    ) -> Option<Amount> {
        Some(amount_in_base * self.get_price_in_usd(currency_code)?)
    }

    pub fn to_usd_string(amount_in_usd: Amount) -> String {
        amount_in_usd.to_string() + " USD"
    }
}
