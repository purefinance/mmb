use anyhow::{Context, Result};
use rust_decimal_macros::dec;
use serum::serum::Serum;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::serum::common::{
    get_additional_key_pair, get_key_pair, get_network_type, get_timeout_manager,
};
use mmb_core::exchanges::common::{Amount, ExchangeAccountId, ExchangeId, Price};
use mmb_core::exchanges::events::{AllowedEventSourceType, ExchangeEvent};
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::exchange::{BoxExchangeClient, Exchange};
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    RestFillsType, WebSocketOptions,
};
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::exchanges::traits::{ExchangeClientBuilder, ExchangeClientBuilderResult};
use mmb_core::infrastructure::init_lifetime_manager;
use mmb_core::lifecycle::app_lifetime_manager::AppLifetimeManager;
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
    pub async fn build_account_0() -> Self {
        let exchange_account_id = ExchangeAccountId::new("Serum", 0); // Serum_0
        let secret_key = get_key_pair().expect("Can't get key/pair for account `Serum_0`");
        SerumBuilder::from_inner(exchange_account_id, secret_key)
            .await
            .expect("Failed to create SerumBuilder for account `Serum_0`")
    }

    pub async fn build_account_1() -> Self {
        let exchange_account_id = ExchangeAccountId::new("Serum", 1); // Serum_1
        let secret_key =
            get_additional_key_pair().expect("Can't get key/pair for account `Serum_1`");
        SerumBuilder::from_inner(exchange_account_id, secret_key)
            .await
            .expect("Failed to create SerumBuilder for account `Serum_1`")
    }

    async fn from_inner(
        exchange_account_id: ExchangeAccountId,
        secret_key: String,
    ) -> Result<SerumBuilder> {
        SerumBuilder::try_new(
            exchange_account_id,
            CancellationToken::default(),
            ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::default(),
                OrderFeatures::default(),
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                false,
                true,
                AllowedEventSourceType::default(),
                AllowedEventSourceType::default(),
            ),
            Commission::default(),
            secret_key,
        )
        .await
    }

    async fn try_new(
        exchange_account_id: ExchangeAccountId,
        _cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
        secret_key: String,
    ) -> Result<Self> {
        let mut settings =
            ExchangeSettings::new_short(exchange_account_id, "".to_string(), secret_key, false);

        settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
            base: "sol".into(),
            quote: "test".into(),
        }]);

        Self::try_new_with_settings(settings, exchange_account_id, features, commission).await
    }

    async fn try_new_with_settings(
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
        exchange.connect().await?;
        exchange.build_symbols(&settings.currency_pairs).await;

        Ok(Self {
            exchange,
            default_price: dec!(0.01),
            default_amount: dec!(0.01),
            rx,
        })
    }
}

pub struct ExchangeSerumBuilder;

impl ExchangeClientBuilder for ExchangeSerumBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
        orders: Arc<OrdersPool>,
    ) -> ExchangeClientBuilderResult {
        let exchange_account_id = exchange_settings.exchange_account_id;
        let empty_response_is_ok = false;

        let network_type = get_network_type().expect("Get network type");
        ExchangeClientBuilderResult {
            client: Box::new(Serum::new(
                exchange_account_id,
                exchange_settings,
                events_channel,
                lifetime_manager,
                orders,
                network_type,
                empty_response_is_ok,
            )) as BoxExchangeClient,
            features: ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::new(RestFillsType::None),
                OrderFeatures::default(),
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                empty_response_is_ok,
                false,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
            ),
        }
    }

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        RequestTimeoutArguments::from_requests_per_minute(240)
    }

    fn get_exchange_id(&self) -> ExchangeId {
        "Serum".into()
    }
}
