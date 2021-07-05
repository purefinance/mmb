use anyhow::{anyhow, Context, Result};
use core::fmt::Debug;
use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
};
use std::{io::Write, sync::Arc};
use toml::toml;

use dashmap::DashMap;
use futures::{future::join_all, FutureExt};
use log::{error, info};
use tokio::sync::{broadcast, oneshot};

use crate::core::exchanges::common::{Amount, CurrencyPair, ExchangeAccountId, ExchangeId};
use crate::core::exchanges::events::{ExchangeEvent, ExchangeEvents, CHANNEL_MAX_EVENTS_COUNT};
use crate::core::exchanges::general::exchange_creation::create_exchange;
use crate::core::exchanges::general::{
    exchange::Exchange, exchange_creation::create_timeout_manager,
};
use crate::core::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::core::exchanges::traits::ExchangeClientBuilder;
use crate::core::internal_events_loop::InternalEventsLoop;
use crate::core::lifecycle::application_manager::ApplicationManager;
use crate::core::lifecycle::cancellation_token::CancellationToken;
use crate::core::lifecycle::trading_engine::{EngineContext, TradingEngine};
use crate::core::logger::init_logger;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::settings::{AppSettings, BaseStrategySettings, CoreSettings};
use crate::core::{
    disposition_execution::executor::DispositionExecutorService,
    infrastructure::{keep_application_manager, spawn_future},
};
use crate::hashmap;
use crate::strategies::disposition_strategy::DispositionStrategy;
use crate::{
    core::exchanges::binance::binance::BinanceBuilder, rest_api::control_panel::ControlPanel,
};
use std::convert::identity;

pub struct EngineBuildConfig {
    pub supported_exchange_clients: HashMap<ExchangeId, Box<dyn ExchangeClientBuilder + 'static>>,
}

impl EngineBuildConfig {
    pub fn standard() -> Self {
        let exchange_name = "Binance".into();
        let supported_exchange_clients =
            hashmap![exchange_name => Box::new(BinanceBuilder) as Box<dyn ExchangeClientBuilder>];

        EngineBuildConfig {
            supported_exchange_clients,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum InitSettings<TStrategySettings>
where
    TStrategySettings: BaseStrategySettings + Clone,
{
    Directly(AppSettings<TStrategySettings>),
    Load(String, String),
}

pub async fn launch_trading_engine<'a, TStrategySettings>(
    build_settings: &EngineBuildConfig,
    init_user_settings: InitSettings<TStrategySettings>,
) -> Result<TradingEngine>
where
    TStrategySettings: BaseStrategySettings + Clone + Debug + Deserialize<'a>,
{
    init_logger();

    info!("*****************************");
    info!("TradingEngine starting");

    let settings = match init_user_settings {
        InitSettings::Directly(v) => v,
        InitSettings::Load(config_path, credentials_path) => {
            load_settings::<TStrategySettings>(&config_path, &credentials_path)?
        }
    };

    let application_manager = ApplicationManager::new(CancellationToken::new());
    keep_application_manager(application_manager.clone());
    let (events_sender, events_receiver) = broadcast::channel(CHANNEL_MAX_EVENTS_COUNT);

    let timeout_manager = create_timeout_manager(&settings.core, &build_settings);
    let exchanges = create_exchanges(
        &settings.core,
        build_settings,
        events_sender.clone(),
        application_manager.clone(),
        &timeout_manager,
    )
    .await;

    let exchanges_map: DashMap<_, _> = exchanges
        .into_iter()
        .map(|exchange| (exchange.exchange_account_id.clone(), exchange))
        .collect();

    let exchange_events = ExchangeEvents::new(events_sender);

    let (finish_graceful_shutdown_tx, finish_graceful_shutdown_rx) = oneshot::channel();
    let engine_context = EngineContext::new(
        settings.core.clone(),
        exchanges_map.clone(),
        exchange_events,
        finish_graceful_shutdown_tx,
        timeout_manager,
        application_manager,
    );

    let internal_events_loop = InternalEventsLoop::new();
    let control_panel = ControlPanel::new("127.0.0.1:8080");

    {
        let local_exchanges_map = exchanges_map.into_iter().map(identity).collect();
        let action = internal_events_loop.clone().start(
            events_receiver,
            local_exchanges_map,
            engine_context.application_manager.stop_token(),
        );
        let _ = spawn_future("internal_events_loop start", true, action.boxed());
    }

    if let Err(error) = control_panel.clone().start() {
        error!("Unable to start rest api: {}", error);
    }

    let strategy_settings = &settings.strategy as &dyn BaseStrategySettings;
    let disposition_executor_service = create_disposition_executor_service(
        strategy_settings.exchange_account_id(),
        strategy_settings.currency_pair(),
        strategy_settings.max_amount(),
        &engine_context,
    );

    engine_context.shutdown_service.register_services(&[
        control_panel,
        internal_events_loop,
        disposition_executor_service,
    ]);

    info!("TradingEngine started");
    Ok(TradingEngine::new(
        engine_context,
        finish_graceful_shutdown_rx,
    ))
}

fn create_disposition_executor_service(
    exchange_account_id: ExchangeAccountId,
    currency_pair: CurrencyPair,
    max_amount: Amount,
    engine_context: &Arc<EngineContext>,
) -> Arc<DispositionExecutorService> {
    DispositionExecutorService::new(
        engine_context.clone(),
        engine_context.get_events_channel(),
        LocalSnapshotsService::default(),
        exchange_account_id.clone(),
        currency_pair.clone(),
        max_amount,
        Box::new(DispositionStrategy::new(exchange_account_id, currency_pair)),
        engine_context.application_manager.stop_token(),
    )
}

pub fn load_settings<'a, TSettings>(
    config_path: &str,
    credentials_path: &str,
) -> Result<AppSettings<TSettings>>
where
    TSettings: BaseStrategySettings + Clone + Debug + Deserialize<'a>,
{
    let mut settings = config::Config::default();
    settings.merge(config::File::with_name(&config_path))?;
    let exchanges = settings.get_array("core.exchanges")?;

    let mut credentials = config::Config::default();
    credentials.merge(config::File::with_name(credentials_path))?;

    // Extract creds accoring to exchange_account_id and add it to every ExchengeSettings
    let mut exchanges_with_creds = Vec::new();
    for exchange in exchanges {
        let mut exchange = exchange.into_table()?;

        let exchange_account_id = exchange.get("exchange_account_id").ok_or(anyhow!(
            "Config file has no exchange account id for Exchange"
        ))?;
        let api_key = &credentials.get_str(&format!("{}.api_key", exchange_account_id))?;
        let secret_key = &credentials.get_str(&format!("{}.secret_key", exchange_account_id))?;

        exchange.insert("api_key".to_owned(), api_key.as_str().into());
        exchange.insert("secret_key".to_owned(), secret_key.as_str().into());

        exchanges_with_creds.push(exchange);
    }

    let mut config_with_creds = config::Config::new();
    config_with_creds.set("core.exchanges", exchanges_with_creds)?;

    settings.merge(config_with_creds)?;

    let decoded = settings.try_into()?;

    Ok(decoded)
}

pub fn save_settings<'a, TSettings>(
    settings: AppSettings<TSettings>,
    config_path: &str,
    credentials_path: &str,
) -> Result<()>
where
    TSettings: BaseStrategySettings + Clone + Debug + Deserialize<'a> + Serialize,
{
    // Write credentials into it's config
    #[derive(Debug)]
    struct Credentials {
        exchange_account_id: String,
        api_key: String,
        secret_key: String,
    }

    let credentials_per_exchange = settings
        .core
        .exchanges
        .iter()
        .map(|exchange_settings| Credentials {
            exchange_account_id: exchange_settings.exchange_account_id.to_string(),
            api_key: exchange_settings.api_key.clone(),
            secret_key: exchange_settings.secret_key.clone(),
        })
        .collect_vec();

    let mut credentials_config = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(credentials_path)?;
    for creds in credentials_per_exchange {
        credentials_config.write_all(format!("[{}]\n", creds.exchange_account_id).as_bytes())?;
        credentials_config.write_all(format!("api_key = \"{}\"\n", creds.api_key).as_bytes())?;
        credentials_config
            .write_all(format!("secret_key = \"{}\"\n\n", creds.secret_key).as_bytes())?;
    }

    // Remove credentials from main config
    // FIXME Евгений, можно ли это как-то спрямить, чтобы не было кучи одинаковы ok_or?
    let mut serialized = toml::value::Value::try_from(settings)?;
    let exchanges = serialized
        .as_table_mut()
        .ok_or(anyhow!("Unable to get a toml table from settings"))?
        .get_mut("core")
        .ok_or(anyhow!("Unable to get core settings"))?
        .as_table_mut()
        .ok_or(anyhow!("Unable to get toml table from core"))?
        .get_mut("exchanges")
        .ok_or(anyhow!("Unable to get exchange from core table"))?
        .as_array_mut()
        .ok_or(anyhow!("Unable to get exchanges as a toml array"))?;
    for exchange in exchanges {
        let exchange = exchange
            .as_table_mut()
            .ok_or(anyhow!("Unable to get mutable exchange table"))?;
        dbg!(&exchange);

        let _ = exchange.remove("api_key");
        let _ = exchange.remove("secret_key");
    }

    let mut main_config = File::create(config_path)?;
    main_config.write_all(&serialized.to_string().as_bytes())?;

    Ok(())
}

pub async fn create_exchanges(
    core_settings: &CoreSettings,
    build_settings: &EngineBuildConfig,
    events_channel: broadcast::Sender<ExchangeEvent>,
    application_manager: Arc<ApplicationManager>,
    timeout_manager: &Arc<TimeoutManager>,
) -> Vec<Arc<Exchange>> {
    join_all(core_settings.exchanges.iter().map(|x| {
        create_exchange(
            x,
            build_settings,
            events_channel.clone(),
            application_manager.clone(),
            timeout_manager.clone(),
        )
    }))
    .await
}
