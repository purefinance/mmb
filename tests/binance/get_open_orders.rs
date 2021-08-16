use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::settings::{CurrencyPairSetting, ExchangeSettings};

use crate::binance::binance_builder::BinanceBuilder;
use crate::core::order::OrderProxy;
use crate::get_binance_credentials_or_exit;

use rust_decimal_macros::dec;

#[actix_rt::test]
async fn open_orders_exists() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let binance_builder = match BinanceBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => {
            return;
        }
    };

    let first_order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromOpenOrdersExistsTest".to_owned()),
        CancellationToken::default(),
    );

    let second_order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromOpenOrdersExistsTest".to_owned()),
        CancellationToken::default(),
    );

    if let Err(error) = first_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
    {
        assert!(false, "Create order failed with error {:?}.", error)
    }

    if let Err(error) = second_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
    {
        assert!(false, "Create order failed with error {:?}.", error)
    }

    let all_orders = binance_builder
        .exchange
        .clone()
        .get_open_orders(false)
        .await
        .expect("in test");

    assert_eq!(all_orders.len(), 2);
}

#[actix_rt::test]
async fn open_orders_by_currency_pair_exist() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let (api_key, secret_key) = get_binance_credentials_or_exit!();
    let mut settings =
        ExchangeSettings::new_short(exchange_account_id.clone(), api_key, secret_key, false);

    settings.currency_pairs = Some(vec![
        CurrencyPairSetting {
            base: "phb".into(),
            quote: "btc".into(),
            currency_pair: None,
        },
        CurrencyPairSetting {
            base: "sngls".into(),
            quote: "btc".into(),
            currency_pair: None,
        },
    ]);

    let binance_builder = match BinanceBuilder::try_new_with_settings(
        settings.clone(),
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::OneCurrencyPair,
            true,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => {
            return;
        }
    };

    let first_order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromGetOpenOrdersByCurrencyPairTest".to_owned()),
        CancellationToken::default(),
    );

    first_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");

    let mut second_order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromGetOpenOrdersByCurrencyPairTest".to_owned()),
        CancellationToken::default(),
    );
    second_order_proxy.currency_pair = CurrencyPair::from_codes("sngls".into(), "btc".into());
    second_order_proxy.amount = dec!(1000);

    second_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");

    let all_orders = binance_builder
        .exchange
        .get_open_orders(true)
        .await
        .expect("in test");

    let _ = binance_builder
        .exchange
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    assert_eq!(all_orders.len(), 2);

    for order in all_orders {
        assert!(
            order.client_order_id == first_order_proxy.client_order_id
                || order.client_order_id == second_order_proxy.client_order_id
        );
    }
}

#[actix_rt::test]
async fn should_return_open_orders() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let binance_builder = match BinanceBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            true,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => {
            return;
        }
    };

    // createdOrder
    let order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromShouldReturnOpenOrdersTest".to_owned()),
        CancellationToken::default(),
    );

    order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");
    // createdOrder

    // orderForCancellation
    let order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromShouldReturnOpenOrdersTest".to_owned()),
        CancellationToken::default(),
    );

    match order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
    {
        Ok(order_ref) => {
            // If here are no error - order was cancelled successfully
            binance_builder
                .exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await
                .expect("in test");
        }

        Err(error) => {
            assert!(false, "Create order failed with error {:?}.", error)
        }
    }
    // orderForCancellation

    // failedToCreateOrder
    let mut order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromShouldReturnOpenOrdersTest".to_owned()),
        CancellationToken::default(),
    );
    order_proxy.amount = dec!(0);

    if let Ok(order_ref) = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
    {
        assert!(
            false,
            "Order {:?} has been created although we expected an error.",
            order_ref
        )
    }
    // failedToCreateOrder

    // TODO: orderForCompletion

    let all_orders = binance_builder
        .exchange
        .get_open_orders(true)
        .await
        .expect("in test");

    let _ = binance_builder
        .exchange
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    assert_eq!(all_orders.len(), 1);
}
