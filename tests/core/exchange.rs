use std::sync::Arc;

use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::ExchangeEvent;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;
use mmb_lib::core::exchanges::{binance::binance::*, general::commission::Commission};
use mmb_lib::core::lifecycle::application_manager::ApplicationManager;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::settings::ExchangeSettings;

use anyhow::Result;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};

use crate::binance::common::*;

pub struct ExchangeBuilder {
    pub exchange: Arc<Exchange>,
    pub tx: Sender<ExchangeEvent>,
    pub rx: Receiver<ExchangeEvent>,
}

impl ExchangeBuilder {
    pub async fn try_new(
        exchange_account_id: ExchangeAccountId,
        cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
    ) -> Result<ExchangeBuilder> {
        let binance_keys = get_binance_credentials();
        if let Err(error) = binance_keys {
            return Err(error);
        }

        let (api_key, secret_key) = binance_keys.unwrap();

        let mut settings =
            ExchangeSettings::new_short(exchange_account_id.clone(), api_key, secret_key, false);

        let application_manager = ApplicationManager::new(cancellation_token);
        let (tx, rx) = broadcast::channel(10);

        BinanceBuilder.extend_settings(&mut settings);
        settings.websocket_channels = vec!["depth".into(), "trade".into()];

        let binance = Box::new(Binance::new(
            exchange_account_id.clone(),
            settings,
            tx.clone(),
            application_manager.clone(),
        ));

        let timeout_manager = get_timeout_manager(&exchange_account_id);
        let exchange = Exchange::new(
            exchange_account_id.clone(),
            binance,
            features,
            tx.clone(),
            application_manager,
            timeout_manager,
            commission,
        );
        exchange.clone().connect().await;
        exchange.build_metadata().await;

        Ok(ExchangeBuilder {
            exchange: exchange,
            tx: tx,
            rx: rx,
        })
    }
}
