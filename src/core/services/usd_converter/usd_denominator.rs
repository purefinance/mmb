use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::FutureExt;
use itertools::Itertools;
use parking_lot::Mutex;

use crate::{
    core::{
        exchanges::common::{Amount, CurrencyCode, Price},
        infrastructure::spawn_repeatable,
        lifecycle::application_manager::ApplicationManager,
        misc::traits::market_service::{CreateMarketService, GetMarketCurrencyCodePrice},
        services::market_prices::market_currency_code_price::MarketCurrencyCodePrice,
    },
    hashmap,
};

pub struct UsdDenominator {
    market_service: Arc<dyn GetMarketCurrencyCodePrice + Send + Sync>,
    application_manager: Arc<ApplicationManager>,
    market_prices_by_currency_code: Mutex<HashMap<CurrencyCode, MarketCurrencyCodePrice>>,
    pub price_update_callback: fn(),
}

impl UsdDenominator {
    fn to_prices_dictionary(
        tickers: Vec<MarketCurrencyCodePrice>,
    ) -> HashMap<CurrencyCode, MarketCurrencyCodePrice> {
        tickers
            .iter()
            .map(|x| (x.currency_code.clone(), x.clone()))
            .collect()
    }

    fn new(
        market_service: Arc<dyn GetMarketCurrencyCodePrice + Send + Sync>,
        market_prices: Vec<MarketCurrencyCodePrice>,
        auto_refresh_data: bool,
        application_manager: Arc<ApplicationManager>,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            market_service,
            application_manager: application_manager.clone(),
            market_prices_by_currency_code: Mutex::new(UsdDenominator::to_prices_dictionary(
                market_prices,
            )),
            price_update_callback: || (),
        });

        if auto_refresh_data {
            let cloned_this = this.clone();
            let _ = spawn_repeatable(
                move || Self::refresh_data(cloned_this.clone()).boxed(),
                "UsdDenominator::refresh_data()",
                Duration::from_secs(7200), // 2 hours
                true,
            );
        }

        this
    }

    pub async fn refresh_data(this: Arc<Self>) {
        let market_prices = this.market_service.get_market_currency_code_price().await;
        *this.market_prices_by_currency_code.lock() =
            UsdDenominator::to_prices_dictionary(market_prices);
        (this.price_update_callback)()
    }

    pub async fn create_async<T>(
        auto_refresh_data: bool,
        application_manager: Arc<ApplicationManager>,
    ) -> Arc<Self>
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

    pub fn get_non_refreshing_usd_denominator(&self) -> Arc<Self> {
        UsdDenominator::new(
            self.market_service.clone(),
            self.market_prices_by_currency_code
                .lock()
                .values()
                .cloned()
                .collect_vec(),
            false,
            self.application_manager.clone(),
        )
    }

    fn currency_code_exceptions() -> HashMap<CurrencyCode, CurrencyCode> {
        hashmap![CurrencyCode::from( "IOTA") => CurrencyCode::from("MIOTA")]
    }

    pub fn get_all_prices_in_usd(&self) -> HashMap<CurrencyCode, Price> {
        let exceptions: HashMap<CurrencyCode, CurrencyCode> =
            UsdDenominator::currency_code_exceptions()
                .iter()
                .map(|(k, v)| (v.clone(), k.clone()))
                .collect();

        self.market_prices_by_currency_code
            .lock()
            .iter()
            .filter_map(|(k, v)| {
                v.price_usd.map(|price| {
                    let k = exceptions.get(k).unwrap_or(k);
                    (k.clone(), price)
                })
            })
            .collect()
    }

    pub fn get_price_in_usd(&self, currency_code: &CurrencyCode) -> Option<Price> {
        let currency_code = UsdDenominator::currency_code_exceptions()
            .get(currency_code)
            .cloned()
            .unwrap_or(currency_code.clone());
        self.market_prices_by_currency_code
            .lock()
            .get(&currency_code)?
            .price_usd
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
}
