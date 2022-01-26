use anyhow::Result;
use rust_decimal_macros::dec;
use serum::serum::{NetworkType, Serum};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::serum::common::{get_key_pair, get_timeout_manager};
use mmb_core::exchanges::common::{Amount, ExchangeAccountId, Price};
use mmb_core::exchanges::events::ExchangeEvent;
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::general::features::ExchangeFeatures;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::lifecycle::application_manager::ApplicationManager;
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
        cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
    ) -> Result<Self> {
        let secret_key = get_key_pair()?;
        let mut settings =
            ExchangeSettings::new_short(exchange_account_id, "".to_string(), secret_key, false);

        settings.currency_pairs = Some(vec![CurrencyPairSetting {
            base: "btc".into(),
            quote: "usdc".into(),
            currency_pair: Some("btc/usdc".to_string()),
        }]);

        Self::try_new_with_settings(
            settings,
            exchange_account_id,
            cancellation_token,
            features,
            commission,
        )
        .await
    }

    pub async fn try_new_with_settings(
        settings: ExchangeSettings,
        exchange_account_id: ExchangeAccountId,
        cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
    ) -> Result<Self> {
        let application_manager = ApplicationManager::new(cancellation_token.clone());
        let (tx, rx) = broadcast::channel(10);
        let timeout_manager = get_timeout_manager(exchange_account_id);

        let serum = Box::new(Serum::new(
            exchange_account_id,
            settings.clone(),
            tx.clone(),
            application_manager.clone(),
            NetworkType::Devnet,
        ));

        let exchange = Exchange::new(
            exchange_account_id,
            serum,
            features,
            RequestTimeoutArguments::from_requests_per_minute(240),
            tx.clone(),
            application_manager,
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
