use anyhow::Result;
use binance::binance::Binance;
use mmb_core::balance::manager::balance_manager::BalanceManager;
use mmb_core::database::events::recorder::EventRecorder;
use mmb_core::exchanges::exchange_blocker::ExchangeBlocker;
use mmb_core::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
use mmb_core::exchanges::general::exchange::*;
use mmb_core::exchanges::general::features::*;
use mmb_core::exchanges::hosts::Hosts;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::infrastructure::init_lifetime_manager;
use mmb_core::settings::CurrencyPairSetting;
use mmb_core::settings::ExchangeSettings;
use mmb_domain::events::{AllowedEventSourceType, ExchangeEvent};
use mmb_domain::exchanges::commission::Commission;
use mmb_domain::market::*;
use mmb_domain::order::pool::OrdersPool;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::hashmap;
use mmb_utils::infrastructure::WithExpect;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::binance::common::get_min_amount;
use crate::binance::common::{default_currency_pair, get_prices};
use crate::binance::common::{get_binance_credentials, get_timeout_manager};

pub(crate) fn default_exchange_account_id() -> ExchangeAccountId {
    const EXCHANGE_ACCOUNT_ID: &str = "Binance_0";

    EXCHANGE_ACCOUNT_ID.parse().expect("in test")
}

#[allow(dead_code)]
pub struct BinanceBuilder {
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

impl BinanceBuilder {
    pub async fn build_account_0() -> Result<Self> {
        BinanceBuilder::try_new(
            default_exchange_account_id(),
            CancellationToken::default(),
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
            true,
            false,
        )
        .await
    }

    pub async fn build_account_0_with_source_types(
        allowed_create_event_source_type: AllowedEventSourceType,
        allowed_cancel_event_source_type: AllowedEventSourceType,
    ) -> Result<Self> {
        BinanceBuilder::try_new(
            default_exchange_account_id(),
            CancellationToken::default(),
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
            true,
            false,
        )
        .await
    }

    pub async fn build_account_0_futures() -> Result<Self> {
        BinanceBuilder::try_new(
            default_exchange_account_id(),
            CancellationToken::default(),
            ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::default(),
                OrderFeatures::default(),
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                true,
                AllowedEventSourceType::default(),
                AllowedEventSourceType::default(),
                AllowedEventSourceType::default(),
            ),
            Commission::default(),
            true,
            true,
        )
        .await
    }

    pub async fn try_new(
        exchange_account_id: ExchangeAccountId,
        cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
        need_to_clean_up: bool,
        is_margin_trading: bool,
    ) -> Result<Self> {
        let (api_key, secret_key) = match get_binance_credentials() {
            Ok((api_key, secret_key)) => (api_key, secret_key),
            Err(_) => ("".to_string(), "".to_string()),
        };
        if api_key.is_empty() || secret_key.is_empty() {
            return Err(anyhow::Error::msg(
                "Environment variable BINANCE_SECRET_KEY or BINANCE_API_KEY are not set. Unable to continue test",
            ));
        }

        let mut settings = ExchangeSettings::new_short(
            exchange_account_id,
            api_key,
            secret_key,
            is_margin_trading,
        );

        // default currency pair for tests
        settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
            base: "btc".into(),
            quote: "usdt".into(),
        }]);

        Ok(Self::try_new_with_settings(
            settings,
            exchange_account_id,
            cancellation_token,
            features,
            commission,
            need_to_clean_up,
        )
        .await)
    }

    pub async fn try_new_with_settings(
        mut settings: ExchangeSettings,
        exchange_account_id: ExchangeAccountId,
        cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
        need_to_clean_up: bool,
    ) -> Self {
        let lifetime_manager = init_lifetime_manager();
        let (tx, rx) = broadcast::channel(10);

        settings.websocket_channels = vec!["depth".into(), "trade".into()];

        let binance = Box::new(Binance::new(
            exchange_account_id,
            settings.clone(),
            tx.clone(),
            lifetime_manager.clone(),
            get_timeout_manager(exchange_account_id),
            false,
        ));

        let hosts = binance.hosts.clone();

        let exchange_blocker = ExchangeBlocker::new(vec![exchange_account_id]);

        let event_recorder = EventRecorder::start(None, None)
            .await
            .expect("Failure start EventRecorder");

        let timeout_manager = get_timeout_manager(exchange_account_id);
        let exchange = Exchange::new(
            exchange_account_id,
            binance,
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
        exchange.connect_ws().await.with_expect(move || {
            format!("Failed to connect to websockets on exchange {exchange_account_id}")
        });
        exchange.build_symbols(&settings.currency_pairs).await;

        let currency_pair_to_symbol_converter =
            CurrencyPairToSymbolConverter::new(hashmap![ exchange_account_id => exchange.clone() ]);

        let balance_manager = BalanceManager::new(currency_pair_to_symbol_converter, None);

        exchange.setup_balance_manager(balance_manager);

        // TODO Remove that workaround when RAII order clearing will be implemented
        if need_to_clean_up {
            exchange
                .clone()
                .cancel_opened_orders(cancellation_token.clone(), true)
                .await;
        }

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

        let min_amount = get_min_amount(
            specific_currency_pair,
            &hosts,
            &settings,
            execution_price,
            &symbol,
        )
        .await;

        Self {
            exchange,
            hosts,
            exchange_settings: settings,
            execution_price,
            min_price,
            min_amount,
            default_currency_pair: default_currency_pair(),
            tx,
            rx,
        }
    }
}
