use anyhow::{Context, Result};
use rust_decimal_macros::dec;
use serum::serum::Serum;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::serum::common::{get_key_pair, get_network_type, get_timeout_manager};
use mmb_core::exchanges::common::{Amount, ExchangeAccountId, Price};
use mmb_core::exchanges::events::ExchangeEvent;
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::general::features::ExchangeFeatures;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::infrastructure::init_lifetime_manager;
use mmb_core::orders::pool::OrdersPool;
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_utils::cancellation_token::CancellationToken;

pub struct SerumBuilder {
    pub exchange: Arc<Exchange>,
    pub default_price: Price,
    pub default_amount: Amount,
    pub rx: broadcast::Receiver<ExchangeEvent>,
}

impl SerumBuilder {
    pub async fn try_new(
        exchange_account_id: ExchangeAccountId,
        _cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
    ) -> Result<Self> {
        let secret_key = get_key_pair()?;
        let mut settings =
            ExchangeSettings::new_short(exchange_account_id, "".to_string(), secret_key, false);

        settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
            base: "sol".into(),
            quote: "test".into(),
        }]);

        Self::try_new_with_settings(settings, exchange_account_id, features, commission).await
    }

    pub async fn try_new_with_settings(
        settings: ExchangeSettings,
        exchange_account_id: ExchangeAccountId,
        features: ExchangeFeatures,
        commission: Commission,
    ) -> Result<Self> {
        let lifetime_manager = init_lifetime_manager();
        let (tx, rx) = broadcast::channel(10);
        let timeout_manager = get_timeout_manager(exchange_account_id);
        let network_type = get_network_type().context("Get network type")?;
        let orders_pool = OrdersPool::new();

        let serum = Box::new(Serum::new(
            exchange_account_id,
            settings.clone(),
            tx.clone(),
            lifetime_manager.clone(),
            orders_pool.clone(),
            network_type,
            false,
        ));

        let exchange = Exchange::new(
            exchange_account_id,
            serum,
            orders_pool,
            features,
            RequestTimeoutArguments::from_requests_per_minute(240),
            tx.clone(),
            lifetime_manager,
            timeout_manager,
            commission,
        );
        exchange.clone().connect().await;
        exchange.build_symbols(&settings.currency_pairs).await;

        Ok(Self {
            exchange,
            default_price: dec!(0.01),
            default_amount: dec!(0.01),
            rx,
        })
    }
}
