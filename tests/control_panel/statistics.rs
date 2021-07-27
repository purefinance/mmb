#![cfg(test)]
use anyhow::Result;
use chrono::Utc;
use futures::FutureExt;
use hyper::Uri;
use mmb_lib::core::settings::BaseStrategySettings;
use mmb_lib::core::{config::parse_settings, orders::order::OrderCreating};
use mmb_lib::core::{
    disposition_execution::{PriceSlot, TradingContext},
    orders::order::OrderHeader,
};
use mmb_lib::core::{
    exchanges::common::Amount,
    lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings},
    DateTime,
};
use mmb_lib::core::{
    exchanges::common::{CurrencyPair, ExchangeAccountId},
    infrastructure::spawn_future,
};
use mmb_lib::core::{exchanges::rest_client::RestClient, orders::order::OrderSnapshot};
use mmb_lib::core::{explanation::Explanation, orders::order::ClientOrderId};
use mmb_lib::core::{
    lifecycle::cancellation_token::CancellationToken, orders::order::OrderSide,
    orders::order::OrderType,
};
use mmb_lib::core::{
    order_book::local_snapshot_service::LocalSnapshotsService, orders::order::OrderExecutionType,
};
use mmb_lib::strategies::disposition_strategy::DispositionStrategy;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

impl BaseStrategySettings for TestStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance0".parse().expect("for testing")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("eth".into(), "btc".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

// Get data to access binance account
#[macro_export]
macro_rules! get_binance_credentials_or_error {
    () => {{
        let api_key = std::env::var("BINANCE_API_KEY");
        let api_key = match api_key {
            Ok(v) => v,
            Err(_) => {
                dbg!("Environment variable BINANCE_API_KEY are not set. Unable to continue test");
                return Ok(());
            }
        };

        let secret_key = std::env::var("BINANCE_SECRET_KEY");
        let secret_key = match secret_key {
            Ok(v) => v,
            Err(_) => {
                dbg!("Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test");
                return Ok(());
            }
        };

        (api_key, secret_key)
    }};
}

#[actix_rt::test]
#[ignore]
async fn orders_cancelled() -> Result<()> {
    let (api_key, secret_key) = get_binance_credentials_or_error!();

    struct TestStrategy;

    impl DispositionStrategy for TestStrategy {
        fn calculate_trading_context(
            &mut self,
            _max_amount: Decimal,
            _now: DateTime,
            _local_snapshots_service: &LocalSnapshotsService,
            _explanation: &mut Explanation,
        ) -> Option<TradingContext> {
            None
        }

        fn handle_order_fill(
            &self,
            _cloned_order: &Arc<OrderSnapshot>,
            _price_slot: &PriceSlot,
            _target_eai: &ExchangeAccountId,
            _cancellation_token: CancellationToken,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let config = EngineBuildConfig::standard();

    let mut settings = parse_settings::<TestStrategySettings>(
        include_str!("../lifecycle.toml"),
        include_str!("../lifecycle.cred.toml"),
    )?;
    let mut exchange_settings = &mut settings.core.exchanges[0];
    exchange_settings.api_key = api_key.clone();
    exchange_settings.secret_key = secret_key;
    let exchange_account_id = exchange_settings.exchange_account_id.clone();

    let init_settings = InitSettings::Directly(settings);
    let engine = launch_trading_engine(&config, init_settings, |_| Box::new(TestStrategy)).await?;

    let context = engine.context().clone();
    let exchange = context
        .exchanges
        .get(&exchange_account_id)
        .expect("in test");

    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");

    let test_order_client_id = ClientOrderId::unique_id();
    let order_header = OrderHeader::new(
        test_order_client_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(10000),
        OrderExecutionType::None,
        None,
        None,
        "FromCreateOrderTest".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header.clone(),
        price: dec!(0.0000001),
    };
    let _ = exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await;

    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");

    let rest_client = RestClient::new();
    let statistics: Value = serde_json::from_str(
        &rest_client
            .get(
                "http://127.0.0.1:8080/stats"
                    .parse::<Uri>()
                    .expect("in test"),
                &api_key,
            )
            .await?
            .content,
    )?;

    let exchange_statistics = &statistics["trade_place_data"]["Binance0|phb/btc"];
    let opened_orders_amount = exchange_statistics["opened_orders_amount"]
        .as_u64()
        .expect("in test");
    let canceled_orders_amount = exchange_statistics["canceled_orders_amount"]
        .as_u64()
        .expect("in test");

    // Only one order was created and cancelled
    assert_eq!(opened_orders_amount, 1);
    assert_eq!(canceled_orders_amount, 1);

    let context = context.clone();
    let action = async move {
        sleep(Duration::from_millis(200)).await;
        context
            .clone()
            .application_manager
            .run_graceful_shutdown("test")
            .await;

        Ok(())
    };
    spawn_future(
        "run graceful_shutdown in launch_engine test",
        true,
        action.boxed(),
    );

    engine.run().await;

    Ok(())
}
