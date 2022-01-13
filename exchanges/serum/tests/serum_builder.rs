use anyhow::Result;
use serum::serum::Serum;
use std::sync::Arc;
use tokio::sync::broadcast;

use mmb_core::exchanges::common::{ExchangeAccountId, ExchangeId};
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::general::features::ExchangeFeatures;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::{
    RequestTimeoutArguments, RequestsTimeoutManagerFactory,
};
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::exchanges::traits::ExchangeClientBuilder;
use mmb_core::lifecycle::application_manager::ApplicationManager;
use mmb_core::lifecycle::launcher::EngineBuildConfig;
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::hashmap;

pub struct SerumBuilder {}

impl SerumBuilder {
    pub async fn try_new(
        exchange_account_id: ExchangeAccountId,
        cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
    ) -> Result<Self> {
        let mut settings =
            ExchangeSettings::new_short(exchange_account_id, "".to_string(), "".to_string(), false);

        settings.currency_pairs = Some(vec![CurrencyPairSetting {
            base: "btc".into(),
            quote: "usdc".into(),
            currency_pair: None,
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
        let (tx, _) = broadcast::channel(10);
        let timeout_manager = Self::get_timeout_manager(exchange_account_id);

        let serum = Box::new(Serum::new(
            exchange_account_id,
            settings.clone(),
            tx.clone(),
            application_manager.clone(),
        ));

        let exchange = Exchange::new(
            exchange_account_id,
            serum,
            features,
            RequestTimeoutArguments::from_requests_per_minute(1200),
            tx.clone(),
            application_manager,
            timeout_manager,
            commission,
        );
        exchange.clone().connect().await;
        exchange.build_symbols(&settings.currency_pairs).await;

        Ok(Self {})
    }

    fn get_timeout_manager(exchange_account_id: ExchangeAccountId) -> Arc<TimeoutManager> {
        let engine_build_config = EngineBuildConfig::standard(
            Box::new(serum::serum::SerumBuilder) as Box<dyn ExchangeClientBuilder>
        );
        let timeout_arguments = engine_build_config.supported_exchange_clients
            [&ExchangeId::new("Binance".into())]
            .get_timeout_arguments();
        let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
            timeout_arguments,
            exchange_account_id,
        );

        TimeoutManager::new(hashmap![exchange_account_id => request_timeout_manager])
    }
}
