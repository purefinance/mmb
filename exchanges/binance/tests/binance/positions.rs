#[cfg(test)]
mod futures {
    use crate::binance::binance_builder::{default_exchange_account_id, BinanceBuilder};
    use crate::binance::common::{get_binance_credentials, get_position_value_by_side};
    use core_tests::order::OrderProxy;
    use mmb_core::exchanges::general::features::{
        ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
        RestFillsType, WebSocketOptions,
    };
    use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
    use mmb_domain::events::AllowedEventSourceType;
    use mmb_domain::exchanges::commission::Commission;
    use mmb_domain::position::ActivePosition;
    use mmb_utils::cancellation_token::CancellationToken;
    use mmb_utils::logger::init_logger;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_positions() {
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

        let features = ExchangeFeatures::new(
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

        let binance_builder = BinanceBuilder::try_new_with_settings(
            settings,
            exchange_account_id,
            CancellationToken::default(),
            features,
            Commission::default(),
            true,
        )
        .await;

        let amount = binance_builder.min_amount;
        let currency_pair = binance_builder.default_currency_pair;
        let mut order_proxy = OrderProxy::new(
            binance_builder.exchange.exchange_account_id,
            Some("FromCancelledSuccessfullyTest".to_owned()),
            CancellationToken::default(),
            binance_builder.execution_price,
            amount,
            currency_pair,
        );
        order_proxy.timeout = Duration::from_secs(15);

        order_proxy
            .create_order(binance_builder.exchange.clone())
            .await
            .expect("Create order failed with error:");

        // Need wait some time until order will be filled
        let _ = sleep(Duration::from_secs(5));

        let active_positions = binance_builder
            .exchange
            .get_active_positions(order_proxy.cancellation_token.clone())
            .await;

        let position_info = active_positions.first().expect("Have no active positions");
        assert_eq!(
            (
                get_position_value_by_side(order_proxy.side, position_info.derivative.position),
                position_info.derivative.currency_pair,
                position_info.derivative.get_side()
            ),
            (amount, currency_pair, order_proxy.side)
        );

        let closed_position = binance_builder
            .exchange
            .close_position(position_info, None, order_proxy.cancellation_token.clone())
            .await
            .expect("Failed to get closed position");

        assert_eq!(closed_position.amount, amount);

        binance_builder
            .exchange
            .cancel_all_orders(order_proxy.currency_pair)
            .await
            .expect("Failed to cancel all orders");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_balance_and_positions() {
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

        let features = ExchangeFeatures::new(
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

        let binance_builder = BinanceBuilder::try_new_with_settings(
            settings,
            exchange_account_id,
            CancellationToken::default(),
            features,
            Commission::default(),
            true,
        )
        .await;

        let amount = binance_builder.min_amount;
        let currency_pair = binance_builder.default_currency_pair;
        let mut order_proxy = OrderProxy::new(
            binance_builder.exchange.exchange_account_id,
            Some("FromCancelledSuccessfullyTest".to_owned()),
            CancellationToken::default(),
            binance_builder.execution_price,
            amount,
            currency_pair,
        );
        order_proxy.timeout = Duration::from_secs(15);

        order_proxy
            .create_order(binance_builder.exchange.clone())
            .await
            .expect("Create order failed with error:");

        // Need wait some time until order will be filled
        let _ = sleep(Duration::from_secs(5));

        let balance_and_positions = binance_builder
            .exchange
            .get_balance(order_proxy.cancellation_token.clone())
            .await
            .expect("Failed to get balance and positions");

        log::info!("Balance and positions: {balance_and_positions:?}");

        let positions = balance_and_positions.positions.expect("Missing positions");
        let active_position =
            ActivePosition::new(positions.first().expect("Have no active positions").clone());
        assert_eq!(
            (
                get_position_value_by_side(order_proxy.side, active_position.derivative.position),
                active_position.derivative.currency_pair,
                active_position.derivative.get_side()
            ),
            (amount, currency_pair, order_proxy.side)
        );

        let closed_position = binance_builder
            .exchange
            .close_position(
                &active_position,
                None,
                order_proxy.cancellation_token.clone(),
            )
            .await
            .expect("Failed to get closed position");

        assert_eq!(closed_position.amount, amount);

        binance_builder
            .exchange
            .cancel_all_orders(order_proxy.currency_pair)
            .await
            .expect("Failed to cancel all orders");
    }
}
