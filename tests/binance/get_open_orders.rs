use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::exchange_creation::get_symbols;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::settings::{CurrencyPairSetting, ExchangeSettings};

use crate::core::exchange::ExchangeBuilder;
use crate::core::order::Order;
use crate::get_binance_credentials_or_exit;

use rust_decimal_macros::dec;

#[actix_rt::test]
async fn open_orders_exists() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let exchange_builder = ExchangeBuilder::try_new(
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
    .await;

    if let Err(_) = exchange_builder {
        return;
    }
    let exchange_builder = exchange_builder.unwrap();

    let first_order = Order::new(
        exchange_account_id.clone(),
        Some("FromOpenOrdersExistsTest".to_owned()),
        CancellationToken::default(),
    );

    let second_order = Order::new(
        exchange_account_id.clone(),
        Some("FromOpenOrdersExistsTest".to_owned()),
        CancellationToken::default(),
    );

    if let Err(error) = first_order.create(exchange_builder.exchange.clone()).await {
        log::error!("{:?}", error);
        return;
    }

    if let Err(error) = second_order.create(exchange_builder.exchange.clone()).await {
        log::error!("{:?}", error);
        return;
    }

    let all_orders = exchange_builder
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

    let exchange_builder = ExchangeBuilder::try_new(
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
    .await;

    if let Err(_) = exchange_builder {
        return;
    }
    let exchange_builder = exchange_builder.unwrap();

    let first_order = Order::new(
        exchange_account_id.clone(),
        Some("FromGetOpenOrdersByCurrencyPairTest".to_owned()),
        CancellationToken::default(),
    );

    if let Some(currency_pairs) = &settings.currency_pairs {
        exchange_builder
            .exchange
            .set_symbols(get_symbols(&exchange_builder.exchange, &currency_pairs[..]))
    }

    first_order
        .create(exchange_builder.exchange.clone())
        .await
        .expect("in test");

    let mut second_order = Order::new(
        exchange_account_id.clone(),
        Some("FromGetOpenOrdersByCurrencyPairTest".to_owned()),
        CancellationToken::default(),
    );
    second_order.currency_pair = CurrencyPair::from_codes("sngls".into(), "btc".into());
    second_order.amount = dec!(1000);

    second_order
        .create(exchange_builder.exchange.clone())
        .await
        .expect("in test");

    let all_orders = exchange_builder
        .exchange
        .get_open_orders(true)
        .await
        .expect("in test");

    let _ = exchange_builder
        .exchange
        .cancel_opened_orders(CancellationToken::default())
        .await;

    assert_eq!(all_orders.len(), 2);

    for order in all_orders {
        assert!(
            order.client_order_id == first_order.client_order_id
                || order.client_order_id == second_order.client_order_id
        );
    }
}

#[actix_rt::test]
async fn should_return_open_orders() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let exchange_builder = ExchangeBuilder::try_new(
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
    .await;

    if let Err(_) = exchange_builder {
        return;
    }
    let exchange_builder = exchange_builder.unwrap();

    // createdOrder
    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromShouldReturnOpenOrdersTest".to_owned()),
        CancellationToken::default(),
    );

    order
        .create(exchange_builder.exchange.clone())
        .await
        .expect("in test");
    // createdOrder

    // orderForCancellation
    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromShouldReturnOpenOrdersTest".to_owned()),
        CancellationToken::default(),
    );

    match order.create(exchange_builder.exchange.clone()).await {
        Ok(order_ref) => {
            // If here are no error - order was cancelled successfully
            exchange_builder
                .exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await
                .expect("in test");
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
    // orderForCancellation

    // failedToCreateOrder
    let mut order = Order::new(
        exchange_account_id.clone(),
        Some("FromShouldReturnOpenOrdersTest".to_owned()),
        CancellationToken::default(),
    );
    order.amount = dec!(0);

    if let Ok(order_ref) = order.create(exchange_builder.exchange.clone()).await {
        dbg!(&order_ref);
        assert!(false)
    }
    // failedToCreateOrder

    // TODO: orderForCompletion

    let all_orders = exchange_builder
        .exchange
        .get_open_orders(true)
        .await
        .expect("in test");

    let _ = exchange_builder
        .exchange
        .cancel_opened_orders(CancellationToken::default())
        .await;

    assert_eq!(all_orders.len(), 1);
}
