use crate::serum::common::{get_additional_key_pair, get_key_pair};
use crate::serum::serum_builder::SerumBuilder;
use core_tests::order::OrderProxyBuilder;
use mmb_core::exchanges::common::{CurrencyPair, ExchangeAccountId};
use mmb_core::exchanges::events::{AllowedEventSourceType, ExchangeEvent};
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_core::order_book_data;
use mmb_core::orders::order::OrderSide;
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use rust_decimal_macros::dec;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fill_order_book() {
    let first_exchange_account_id = ExchangeAccountId::new("Serum_first".into(), 0);
    let first_secret_key = get_key_pair().expect("in test");
    let mut first_settings = ExchangeSettings::new_short(
        first_exchange_account_id,
        "".to_string(),
        first_secret_key,
        false,
    );
    first_settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
        base: "sol".into(),
        quote: "test".into(),
    }]);
    let first_serum_builder = SerumBuilder::try_new_with_settings(
        first_settings,
        first_exchange_account_id,
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
            OrderFeatures::default(),
            OrderTradeOption::default(),
            WebSocketOptions::default(),
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
    )
    .await
    .expect("Failed to create first SerumBuilder");
    let first_price = dec!(1);
    let first_amount = dec!(1);
    let first_order_proxy = OrderProxyBuilder::new(
        first_exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        first_price,
        first_amount,
    )
    .currency_pair(CurrencyPair::from_codes("sol".into(), "test".into()))
    .side(OrderSide::Buy)
    .build();

    let second_exchange_account_id = ExchangeAccountId::new("Serum_second".into(), 0);
    let second_secret_key = get_additional_key_pair().expect("in test");
    let mut second_settings = ExchangeSettings::new_short(
        second_exchange_account_id,
        "".to_string(),
        second_secret_key,
        false,
    );
    second_settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
        base: "sol".into(),
        quote: "test".into(),
    }]);
    let mut second_serum_builder = SerumBuilder::try_new_with_settings(
        second_settings,
        second_exchange_account_id,
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
            OrderFeatures::default(),
            OrderTradeOption::default(),
            WebSocketOptions::default(),
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
    )
    .await
    .expect("Failed to create second SerumBuilder");
    let second_price = dec!(2);
    let second_amount = dec!(10);
    let second_order_proxy = OrderProxyBuilder::new(
        second_exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        second_price,
        second_amount,
    )
    .currency_pair(CurrencyPair::from_codes("sol".into(), "test".into()))
    .side(OrderSide::Buy)
    .build();

    let first_order_ref = first_order_proxy
        .create_order(first_serum_builder.exchange.clone())
        .await
        .expect("Create first order failed with error");

    let second_order_ref = second_order_proxy
        .create_order(second_serum_builder.exchange.clone())
        .await
        .expect("Create second order failed with error");

    let event = second_serum_builder
        .rx
        .recv()
        .await
        .expect("CreateOrderSucceeded event had to be occurred");

    let order_book_event = if let ExchangeEvent::OrderBookEvent(order_book_event) = event {
        order_book_event
    } else {
        panic!("Should receive OrderBookEvent")
    };

    let expected_order_book_data = order_book_data![
        ;
        first_price => first_amount,
        second_price => second_amount,
    ];

    assert_eq!(expected_order_book_data, *order_book_event.data);

    first_order_proxy
        .cancel_order_or_fail(&first_order_ref, first_serum_builder.exchange.clone())
        .await;

    second_order_proxy
        .cancel_order_or_fail(&second_order_ref, second_serum_builder.exchange.clone())
        .await;
}
