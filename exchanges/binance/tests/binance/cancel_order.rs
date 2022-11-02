use mmb_domain::order::snapshot::*;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger;
use std::time::Duration;

use crate::binance::binance_builder::{default_exchange_account_id, BinanceBuilder};
use crate::binance::common::get_binance_credentials;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_core::exchanges::general::features::{
    BalancePositionOption, ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption,
    RestFillsFeatures, RestFillsType, WebSocketOptions,
};
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::AllowedEventSourceType;
use mmb_domain::exchanges::commission::Commission;
use mmb_domain::market::ExchangeErrorType;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_successfully() {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order_proxy = OrderProxy::new(
        binance_builder.exchange.exchange_account_id,
        Some("FromCancelledSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.min_price,
        binance_builder.min_amount,
        binance_builder.default_currency_pair,
    );

    let order_ref = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("Create order failed with error:");

    order_proxy
        .cancel_order_or_fail(&order_ref, binance_builder.exchange.clone())
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_opened_orders_successfully() {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };
    let exchange_account_id = binance_builder.exchange.exchange_account_id;

    let first_order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCancelOpenedOrdersSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.min_price,
        binance_builder.min_amount,
        binance_builder.default_currency_pair,
    );
    first_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");

    let second_order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCancelOpenedOrdersSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.min_price,
        binance_builder.min_amount,
        binance_builder.default_currency_pair,
    );
    second_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");

    let orders = &binance_builder
        .exchange
        .get_open_orders(false)
        .await
        .expect("Opened orders not found for exchange account id:");

    assert_eq!(orders.len(), 2);
    binance_builder
        .exchange
        .clone()
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    let orders = &binance_builder
        .exchange
        .get_open_orders(false)
        .await
        .expect("Opened orders not found for exchange account id");

    assert_eq!(orders.len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_to_cancel() {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order = OrderProxy::new(
        binance_builder.exchange.exchange_account_id,
        Some("FromNothingToCancelTest".to_owned()),
        CancellationToken::default(),
        binance_builder.min_price,
        binance_builder.min_amount,
        binance_builder.default_currency_pair,
    );
    let order_to_cancel = OrderCancelling {
        header: order.make_header(),
        exchange_order_id: "1234567890".into(),
        extension_data: None,
    };

    // Cancel last order
    let cancel_outcome = binance_builder
        .exchange
        .cancel_order(order_to_cancel, CancellationToken::default())
        .await
        .expect("in test");
    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert!(error.message.contains("Unknown order sent"));
    }
}

#[cfg(test)]
mod futures {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_after_fill() {
        init_logger();

        let (api_key, secret_key) = match get_binance_credentials() {
            Ok((api_key, secret_key)) => (api_key, secret_key),
            Err(_) => return,
        };
        let exchange_account_id = default_exchange_account_id();
        let mut settings =
            ExchangeSettings::new_short(exchange_account_id, api_key, secret_key, true);
        settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
            base: "BTC".into(),
            quote: "USDT".into(),
        }]);

        let mut features = ExchangeFeatures::new(
            OpenOrdersType::OneCurrencyPair,
            RestFillsFeatures::new(RestFillsType::MyTrades),
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
        );
        features.balance_position_option = BalancePositionOption::IndividualRequests;

        let binance_builder = BinanceBuilder::try_new_with_settings(
            settings,
            exchange_account_id,
            CancellationToken::default(),
            features,
            Commission::default(),
            true,
        )
        .await;

        let mut order_proxy = OrderProxy::new(
            binance_builder.exchange.exchange_account_id,
            Some("FromCancelledSuccessfullyTest".to_owned()),
            CancellationToken::default(),
            binance_builder.execution_price,
            binance_builder.min_amount,
            binance_builder.default_currency_pair,
        );
        order_proxy.timeout = Duration::from_secs(15);

        let order_ref = order_proxy
            .create_order(binance_builder.exchange.clone())
            .await
            .expect("Create order failed with error:");

        let _ = sleep(Duration::from_secs(5));

        let order_to_cancel = OrderCancelling {
            header: order_proxy.make_header(),
            exchange_order_id: order_ref
                .exchange_order_id()
                .expect("Failed to get exchange order id of created order"),
            extension_data: None,
        };

        let cancel_outcome = binance_builder
            .exchange
            .cancel_order(order_to_cancel, CancellationToken::default())
            .await
            .expect("in test");

        if let RequestResult::Error(error) = cancel_outcome.outcome {
            assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
        }

        let active_positions = binance_builder
            .exchange
            .get_active_positions(order_proxy.cancellation_token.clone())
            .await;
        let position_info = active_positions.first().expect("Have no active positions");

        let _ = binance_builder
            .exchange
            .close_position(position_info, None, order_proxy.cancellation_token.clone())
            .await
            .expect("Failed to get closed position");
    }
}
