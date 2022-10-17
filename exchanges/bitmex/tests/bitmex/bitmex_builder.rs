use crate::bitmex::common::{
    default_currency_pair, get_bitmex_credentials, get_prices, get_timeout_manager,
};
use anyhow::{bail, Result};
use bitmex::bitmex::Bitmex;
use mmb_core::balance::manager::balance_manager::BalanceManager;
use mmb_core::database::events::recorder::EventRecorder;
use mmb_core::exchanges::exchange_blocker::ExchangeBlocker;
use mmb_core::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
use mmb_core::exchanges::general::exchange::{get_specific_currency_pair_for_tests, Exchange};
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_core::exchanges::hosts::Hosts;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::infrastructure::init_lifetime_manager;
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::{AllowedEventSourceType, ExchangeEvent};
use mmb_domain::exchanges::commission::Commission;
use mmb_domain::market::{CurrencyPair, ExchangeAccountId};
use mmb_domain::order::pool::OrdersPool;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_utils::hashmap;
use mmb_utils::infrastructure::WithExpect;
use std::sync::Arc;
use tokio::sync::broadcast;

pub(crate) fn default_exchange_account_id() -> ExchangeAccountId {
    const EXCHANGE_ACCOUNT_ID: &str = "Bitmex_0";

    EXCHANGE_ACCOUNT_ID.parse().expect("in test")
}

#[allow(dead_code)]
pub(crate) struct BitmexBuilder {
    pub(crate) exchange: Arc<Exchange>,
    hosts: Hosts,
    exchange_settings: ExchangeSettings,
    pub(crate) execution_price: Price,
    pub(crate) min_price: Price,
    pub(crate) min_amount: Amount,
    pub(crate) default_currency_pair: CurrencyPair,
    tx: broadcast::Sender<ExchangeEvent>,
    pub(crate) rx: broadcast::Receiver<ExchangeEvent>,
}

impl BitmexBuilder {
    pub(crate) async fn build_account_with_setting(
        settings: ExchangeSettings,
        features: ExchangeFeatures,
    ) -> Self {
        BitmexBuilder::try_new_with_settings(settings, features, Commission::default()).await
    }

    pub(crate) async fn build_account(is_margin_trading: bool) -> Result<Self> {
        BitmexBuilder::try_new(
            default_exchange_account_id(),
            ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::default(),
                OrderFeatures {
                    supports_get_order_info_by_client_order_id: true,
                    ..OrderFeatures::default()
                },
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                true,
                AllowedEventSourceType::default(),
                AllowedEventSourceType::default(),
                AllowedEventSourceType::default(),
            ),
            Commission::default(),
            is_margin_trading,
        )
        .await
    }

    pub(crate) async fn build_account_with_source_types(
        allowed_create_event_source_type: AllowedEventSourceType,
        allowed_cancel_event_source_type: AllowedEventSourceType,
        is_margin_trading: bool,
    ) -> Result<Self> {
        let exchange_account_id: ExchangeAccountId = "Bitmex_0".parse().expect("in test");
        BitmexBuilder::try_new(
            exchange_account_id,
            ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::default(),
                OrderFeatures {
                    supports_get_order_info_by_client_order_id: true,
                    ..OrderFeatures::default()
                },
                OrderTradeOption::default(),
                WebSocketOptions {
                    cancellation_notification: true,
                    ..WebSocketOptions::default()
                },
                true,
                allowed_create_event_source_type,
                AllowedEventSourceType::default(),
                allowed_cancel_event_source_type,
            ),
            Commission::default(),
            is_margin_trading,
        )
        .await
    }

    async fn try_new(
        exchange_account_id: ExchangeAccountId,
        features: ExchangeFeatures,
        commission: Commission,
        is_margin_trading: bool,
    ) -> Result<Self> {
        let (api_key, secret_key) = match get_bitmex_credentials() {
            Ok((api_key, secret_key)) => (api_key, secret_key),
            Err(_) => ("".to_string(), "".to_string()),
        };
        if api_key.is_empty() || secret_key.is_empty() {
            bail!(
                "Environment variable BITMEX_SECRET_KEY or BITMEX_API_KEY are not set. Unable to continue test",
            )
        }

        let mut settings = ExchangeSettings::new_short(
            exchange_account_id,
            api_key,
            secret_key,
            is_margin_trading,
        );

        // Default currency pair for tests
        match is_margin_trading {
            true => {
                settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
                    base: "XBT".into(),
                    quote: "USD".into(),
                }]);
            }
            false => {
                settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
                    base: "XBT".into(),
                    quote: "USDT".into(),
                }]);
            }
        }

        Ok(Self::try_new_with_settings(settings, features, commission).await)
    }

    async fn try_new_with_settings(
        settings: ExchangeSettings,
        features: ExchangeFeatures,
        commission: Commission,
    ) -> Self {
        let lifetime_manager = init_lifetime_manager();
        let (tx, rx) = broadcast::channel(10);

        let bitmex = Box::new(Bitmex::new(
            settings.clone(),
            tx.clone(),
            lifetime_manager.clone(),
        ));

        let hosts = bitmex.hosts.clone();

        let exchange_blocker = ExchangeBlocker::new(vec![settings.exchange_account_id]);
        let event_recorder = EventRecorder::start(None, None)
            .await
            .expect("Failure start EventRecorder");

        let timeout_manager = get_timeout_manager(settings.exchange_account_id);
        let exchange = Exchange::new(
            settings.exchange_account_id,
            bitmex,
            OrdersPool::new(),
            features,
            RequestTimeoutArguments::from_requests_per_minute(1200),
            tx.clone(),
            lifetime_manager,
            timeout_manager,
            Arc::downgrade(&exchange_blocker),
            commission,
            event_recorder,
        );
        exchange.build_symbols(&settings.currency_pairs).await;
        exchange.connect_ws().await.with_expect(move || {
            format!(
                "Failed to connect to websockets on exchange {}",
                settings.exchange_account_id
            )
        });

        let currency_pair_to_symbol_converter = CurrencyPairToSymbolConverter::new(
            hashmap![ settings.exchange_account_id => exchange.clone() ],
        );

        let balance_manager = BalanceManager::new(currency_pair_to_symbol_converter, None);

        exchange.setup_balance_manager(balance_manager);

        let currency_pair = default_currency_pair();
        let symbol = exchange
            .symbols
            .get(&currency_pair)
            .with_expect(|| format!("Can't find symbol {currency_pair})"))
            .value()
            .clone();
        let specific_currency_pair = get_specific_currency_pair_for_tests(&exchange, currency_pair);

        let (execution_price, min_price) = get_prices(
            specific_currency_pair,
            &hosts,
            &settings,
            &symbol.price_precision,
        )
        .await;
        let min_amount = symbol
            .get_min_amount(execution_price)
            .expect("Failed to calc min amount");

        Self {
            exchange,
            hosts,
            exchange_settings: settings,
            execution_price,
            min_price,
            min_amount,
            default_currency_pair: currency_pair,
            tx,
            rx,
        }
    }
}
