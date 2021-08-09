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

use crate::binance::common::{get_binance_credentials, get_timeout_manager};

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
        need_to_clean_up: bool,
    ) -> Result<ExchangeBuilder> {
        let (api_key, secret_key) = match get_binance_credentials() {
            Ok((api_key, secret_key)) => (api_key, secret_key),
            Err(_) => (String::from(""), String::from("")),
        };
        if api_key == "" || secret_key == "" {
            return Err(anyhow::Error::msg(
                "Environment variable BINANCE_SECRET_KEY or BINANCE_API_KEY are not set. Unable to continue test",
            ));
        }

        let settings =
            ExchangeSettings::new_short(exchange_account_id.clone(), api_key, secret_key, false);
        ExchangeBuilder::try_new_with_custom_settings(
            settings,
            exchange_account_id,
            cancellation_token,
            features,
            commission,
            need_to_clean_up,
        )
        .await
    }

    pub async fn try_new_with_custom_settings(
        mut settings: ExchangeSettings,
        exchange_account_id: ExchangeAccountId,
        cancellation_token: CancellationToken,
        features: ExchangeFeatures,
        commission: Commission,
        need_to_clean_up: bool,
    ) -> Result<ExchangeBuilder> {
        let application_manager = ApplicationManager::new(cancellation_token.clone());
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
        ); // TODO: change to mmb_lib::core::exchanges::general::exchange_creation::create_exchange::create_exchange() when it will be ready
        exchange.clone().connect().await;
        exchange.build_metadata().await;

        if need_to_clean_up {
            exchange
                .clone()
                .cancel_opened_orders(cancellation_token.clone())
                .await;
        }

        Ok(ExchangeBuilder {
            exchange: exchange,
            tx: tx,
            rx: rx,
        })
    }
}
