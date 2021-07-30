#![cfg(test)]
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

use crate::get_binance_credentials_or_exit;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

impl BaseStrategySettings for TestStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance0".parse().expect("for testing")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("phb".into(), "btc".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[actix_rt::test]
async fn orders_cancelled() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

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
        include_str!("control_panel.toml"),
        include_str!("control_panel.cred.toml"),
    )
    .expect("in test");
    let mut exchange_settings = &mut settings.core.exchanges[0];
    exchange_settings.api_key = api_key.clone();
    exchange_settings.secret_key = secret_key;
    let exchange_account_id = exchange_settings.exchange_account_id.clone();

    let init_settings = InitSettings::Directly(settings.clone());
    let engine = launch_trading_engine(&config, init_settings, |_| Box::new(TestStrategy))
        .await
        .expect("in test");

    let context = engine.context().clone();
    let exchange = context
        .exchanges
        .get(&exchange_account_id)
        .expect("in test");

    let currency_pairs = settings
        .core
        .exchanges
        .first()
        .and_then(|exchange_settings| exchange_settings.currency_pairs.clone())
        .expect("in test");
    let currency_pair_setting = currency_pairs.first().expect("in test");
    let test_currency_pair = CurrencyPair::from_codes(
        currency_pair_setting.base.clone(),
        currency_pair_setting.quote.clone(),
    );
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
            .await
            .expect("in test")
            .content,
    )
    .expect("in test");

    let exchange_statistics = &statistics["trade_place_stats"]["Binance0|phb/btc"];
    let opened_orders_count = exchange_statistics["opened_orders_count"]
        .as_u64()
        .expect("in test");
    let canceled_orders_count = exchange_statistics["canceled_orders_count"]
        .as_u64()
        .expect("in test");

    // Only one order was created and cancelled
    assert_eq!(opened_orders_count, 1);
    assert_eq!(canceled_orders_count, 1);

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
}
