use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::lifecycle::app_lifetime_manager::AppLifetimeManager;
use itertools::Itertools;
use mmb_domain::market::CurrencyCode;
use mmb_domain::market::CurrencyId;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_utils::{hashmap, infrastructure::SpawnFutureFlags};
use parking_lot::Mutex;

use crate::{
    infrastructure::spawn_by_timer,
    misc::traits::market_service::{CreateMarketService, GetMarketCurrencyCodePrice},
    services::market_prices::market_currency_code_price::MarketCurrencyCodePrice,
};

pub struct UsdDenominator {
    market_service: Arc<dyn GetMarketCurrencyCodePrice>,
    lifetime_manager: Arc<AppLifetimeManager>,
    market_prices_by_currency_code: Mutex<HashMap<CurrencyCode, MarketCurrencyCodePrice>>,
    pub price_update_callback: Box<dyn Fn() + Sync + Send>,
}

impl UsdDenominator {
    fn create_prices_dictionary(
        tickers: Vec<MarketCurrencyCodePrice>,
    ) -> HashMap<CurrencyCode, MarketCurrencyCodePrice> {
        let exceptions: HashMap<_, _> = UsdDenominator::currency_code_exceptions()
            .into_iter()
            .map(|(k, v)| (v, k))
            .collect();

        tickers
            .into_iter()
            .map(|x| {
                let currency_code = exceptions
                    .get(&x.currency_code.as_str().into())
                    .cloned()
                    .unwrap_or(x.currency_code);
                (currency_code, x)
            })
            .collect()
    }

    fn new(
        market_service: Arc<dyn GetMarketCurrencyCodePrice>,
        market_prices: Vec<MarketCurrencyCodePrice>,
        auto_refresh_data: bool,
        lifetime_manager: Arc<AppLifetimeManager>,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            market_service,
            lifetime_manager,
            market_prices_by_currency_code: Mutex::new(UsdDenominator::create_prices_dictionary(
                market_prices,
            )),
            price_update_callback: Box::new(|| ()),
        });

        if auto_refresh_data {
            let this = this.clone();
            let _ = spawn_by_timer(
                "UsdDenominator::refresh_data()",
                Duration::ZERO,
                Duration::from_secs(7200), // 2 hours
                SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                move || Self::refresh_data(this.clone()),
            );
        }

        this
    }

    pub async fn refresh_data(this: Arc<Self>) {
        let market_prices = this.market_service.get_market_currency_code_price().await;
        *this.market_prices_by_currency_code.lock() =
            UsdDenominator::create_prices_dictionary(market_prices);
        (this.price_update_callback)()
    }

    pub async fn create_async<T: 'static>(
        auto_refresh_data: bool,
        lifetime_manager: Arc<AppLifetimeManager>,
    ) -> Arc<Self>
    where
        T: GetMarketCurrencyCodePrice + CreateMarketService,
    {
        let service = T::new();
        let market_prices = service.get_market_currency_code_price().await;
        UsdDenominator::new(
            service as Arc<dyn GetMarketCurrencyCodePrice>,
            market_prices,
            auto_refresh_data,
            lifetime_manager,
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
            self.lifetime_manager.clone(),
        )
    }

    fn currency_code_exceptions() -> HashMap<CurrencyCode, CurrencyId> {
        hashmap!["IOTA".into() => "MIOTA".into()]
    }

    pub fn get_all_prices_in_usd(&self) -> HashMap<CurrencyCode, Price> {
        self.market_prices_by_currency_code
            .lock()
            .iter()
            .filter_map(|(currency_code, market_currency_code_price)| {
                market_currency_code_price
                    .price_usd
                    .map(|price| (*currency_code, price))
            })
            .collect()
    }

    pub fn get_price_in_usd(&self, currency_code: CurrencyCode) -> Option<Price> {
        self.market_prices_by_currency_code
            .lock()
            .get(&currency_code)?
            .price_usd
    }

    pub fn usd_to_currency(
        &self,
        currency_code: CurrencyCode,
        amount_in_usd: Amount,
    ) -> Option<Amount> {
        Some(amount_in_usd / self.get_price_in_usd(currency_code)?)
    }

    pub fn currency_to_usd(
        &self,
        currency_code: CurrencyCode,
        amount_in_base: Amount,
    ) -> Option<Amount> {
        Some(amount_in_base * self.get_price_in_usd(currency_code)?)
    }
}
