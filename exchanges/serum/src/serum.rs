use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use solana_client::rpc_client::RpcClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

use mmb_core::core::exchanges::common::{
    Amount, CurrencyCode, CurrencyId, CurrencyPair, ExchangeAccountId, Price, SpecificCurrencyPair,
};
use mmb_core::core::exchanges::events::{AllowedEventSourceType, ExchangeEvent, TradeId};
use mmb_core::core::exchanges::general::exchange::BoxExchangeClient;
use mmb_core::core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    RestFillsType, WebSocketOptions,
};
use mmb_core::core::exchanges::general::handlers::handle_order_filled::FillEventData;
use mmb_core::core::exchanges::rest_client::RestClient;
use mmb_core::core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::core::exchanges::traits::{ExchangeClientBuilder, ExchangeClientBuilderResult};
use mmb_core::core::lifecycle::application_manager::ApplicationManager;
use mmb_core::core::orders::fill::EventSourceType;
use mmb_core::core::orders::order::{ClientOrderId, ExchangeOrderId, OrderSide};
use mmb_core::core::settings::ExchangeSettings;
use mmb_utils::DateTime;

const MAINNET_SOLANA_URL_PATH: &str = "https://api.mainnet-beta.solana.com";

pub struct Serum {
    pub id: ExchangeAccountId,
    pub settings: ExchangeSettings,
    pub order_created_callback:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>>,
    pub order_cancelled_callback:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>>,
    pub handle_order_filled_callback: Mutex<Box<dyn FnMut(FillEventData) + Send + Sync>>,
    pub handle_trade_callback: Mutex<
        Box<dyn FnMut(CurrencyPair, TradeId, Price, Amount, OrderSide, DateTime) + Send + Sync>,
    >,

    pub unified_to_specific: RwLock<HashMap<CurrencyPair, SpecificCurrencyPair>>,
    pub supported_currencies: DashMap<CurrencyId, CurrencyCode>,
    pub traded_specific_currencies: Mutex<Vec<SpecificCurrencyPair>>,
    pub(super) _application_manager: Arc<ApplicationManager>,
    pub(super) _events_channel: broadcast::Sender<ExchangeEvent>,
    pub(super) rest_client: RestClient,
    pub(super) rpc_client: RpcClient,
}

impl Serum {
    pub fn new(
        id: ExchangeAccountId,
        settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        application_manager: Arc<ApplicationManager>,
    ) -> Self {
        Self {
            id,
            settings,
            order_created_callback: Mutex::new(Box::new(|_, _, _| {})),
            order_cancelled_callback: Mutex::new(Box::new(|_, _, _| {})),
            handle_order_filled_callback: Mutex::new(Box::new(|_| {})),
            handle_trade_callback: Mutex::new(Box::new(|_, _, _, _, _, _| {})),
            unified_to_specific: Default::default(),
            supported_currencies: Default::default(),
            traded_specific_currencies: Default::default(),
            _application_manager: application_manager,
            _events_channel: events_channel,
            rest_client: RestClient::new(),
            rpc_client: RpcClient::new(MAINNET_SOLANA_URL_PATH.to_string()),
        }
    }
}

pub struct SerumBuilder;

impl ExchangeClientBuilder for SerumBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: tokio::sync::broadcast::Sender<ExchangeEvent>,
        application_manager: Arc<ApplicationManager>,
    ) -> ExchangeClientBuilderResult {
        let exchange_account_id = exchange_settings.exchange_account_id;

        ExchangeClientBuilderResult {
            client: Box::new(Serum::new(
                exchange_account_id,
                exchange_settings,
                events_channel,
                application_manager,
            )) as BoxExchangeClient,
            features: ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::new(RestFillsType::None),
                OrderFeatures::default(),
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                false,
                false,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
            ),
        }
    }

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        RequestTimeoutArguments::from_requests_per_minute(1200)
    }
}
