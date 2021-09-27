use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{bail, Result};
use futures::{lock::Mutex, FutureExt};
use itertools::Itertools;

use crate::{
    core::{
        exchanges::common::{Amount, CurrencyCode, Price},
        infrastructure::spawn_future,
        lifecycle::cancellation_token::CancellationToken,
        misc::traits::market_service::{CreateMarketService, GetMarketCurrencyCodePrice},
        services::market_prices::market_currency_code_price::MarketCurrencyCodePrice,
    },
    hashmap,
};

pub struct UsdDenominator {
    market_service: Arc<dyn GetMarketCurrencyCodePrice + Send + Sync>,
    // вернуть applicationManager
    cancellation_token: CancellationToken, // review: нужен ли тут ApplicationManager или достаточно токена?
    market_prices_by_symbol: HashMap<CurrencyCode, MarketCurrencyCodePrice>,
    pub event_handler: Option<fn()>,
}

impl UsdDenominator {
    fn to_prices_dictionary(
        mut tickers: Vec<MarketCurrencyCodePrice>,
    ) -> HashMap<CurrencyCode, MarketCurrencyCodePrice> {
        tickers.dedup(); // review: не уверен, что здесь все правильно сделал
        tickers
            .iter()
            .map(|x| (x.symbol.clone(), x.clone()))
            .collect()
    }

    fn new(
        market_service: Arc<dyn GetMarketCurrencyCodePrice + Send + Sync>,
        market_prices: Vec<MarketCurrencyCodePrice>,
        auto_refresh_data: bool,
        cancellation_token: CancellationToken,
    ) -> Arc<Mutex<Self>> {
        let this = Arc::new(Mutex::new(Self {
            market_service,
            cancellation_token,
            market_prices_by_symbol: UsdDenominator::to_prices_dictionary(market_prices),
            event_handler: None,
        }));

        if auto_refresh_data {
            let this_for_timer = this.clone();
            let action = async move {
                let two_hours = 7200;
                let mut period = Duration::from_secs(two_hours);
                loop {
                    tokio::time::sleep(period).await;
                    if this_for_timer
                        .lock()
                        .await
                        .cancellation_token
                        .is_cancellation_requested()
                    {
                        break; // остановить здесь apllication manager
                    } // выделить SafeTimer ловить внутри панику catch_unwind
                    this_for_timer.lock().await.refresh_data().await?;
                    period = Duration::from_secs(two_hours);
                }
                Ok(())
            };
            spawn_future("Start UsdDenominator refresh timer", true, action.boxed());
        }

        this
    }

    async fn refresh_data(&mut self) -> Result<()> {
        let market_prices = self.market_service.get_market_currency_code_price().await;
        self.market_prices_by_symbol = UsdDenominator::to_prices_dictionary(market_prices);
        // review: нужен ли тут amotic exchange?
        // Interlocked.Exchange(ref _marketPricesBySymbol, newMarketPricesDict);
        if let Some(event_handler) = self.event_handler {
            event_handler();
            return Ok(());
        }
        bail!("UsdDenominator::refresh_data event_handler is not set")
    }

    pub async fn crate_async<T>(
        auto_refresh_data: bool,
        cancellation_token: CancellationToken,
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
            cancellation_token,
        )
    }

    pub fn get_non_refreshing_usd_denominator(&self) -> Arc<Mutex<UsdDenominator>> {
        UsdDenominator::new(
            self.market_service.clone(),
            self.market_prices_by_symbol.values().cloned().collect_vec(),
            false,
            self.cancellation_token.clone(),
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
