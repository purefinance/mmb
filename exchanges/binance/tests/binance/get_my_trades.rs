#[cfg(test)]
mod futures {
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
    use mmb_utils::cancellation_token::CancellationToken;
    use mmb_utils::infrastructure::WithExpect;
    use mmb_utils::logger::init_logger;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_my_trades() {
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
            Some("FromGetOrderInfoTest".to_owned()),
            CancellationToken::default(),
            binance_builder.execution_price,
            binance_builder.min_amount,
            binance_builder.default_currency_pair,
        );
        order_proxy.timeout = Duration::from_secs(15);

        let order_ref = order_proxy
            .create_order(binance_builder.exchange.clone())
            .await
            .expect("in test");

        // Need wait some time until order will be filled
        let _ = sleep(Duration::from_secs(5));

        let currency_pair = order_proxy.currency_pair;
        let symbol = binance_builder
            .exchange
            .symbols
            .get(&currency_pair)
            .with_expect(|| format!("Can't find symbol {currency_pair})"))
            .value()
            .clone();
        let trades = binance_builder
            .exchange
            .get_order_trades(&symbol, &order_ref)
            .await
            .expect("in test");

        match trades {
            RequestResult::Success(data) => {
                let trade = data.first().expect("No one trade received");
                assert_eq!(
                    trade.exchange_order_id.clone(),
                    order_ref
                        .exchange_order_id()
                        .expect("Failed to get order's exchange id"),
                )
            }
            RequestResult::Error(err) => panic!("Failed to get trades: {err:?}"),
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
