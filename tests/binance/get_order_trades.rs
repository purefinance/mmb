use mmb::exchanges::{events::AllowedEventSourceType, general::commission::Commission};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::currency_pair_metadata::{CurrencyPairMetadata, Precision};
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::event::OrderEventType;
use rust_decimal_macros::*;

use mmb_lib::core::exchanges::events::ExchangeEvent;

use crate::binance::binance_builder::BinanceBuilder;
use crate::core::order::OrderProxy;

#[actix_rt::test]
async fn empty_open_trades_returned_successfully() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let mut binance_builder = match BinanceBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::new(RestFillsType::MyTrades),
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
        Err(_) => return,
    };

    let order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromCreateSuccessfullyTest".to_owned()),
        CancellationToken::default(),
    );

    match order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
    {
        Ok(order_ref) => {
            let currency_pair_metadata = CurrencyPairMetadata::new(
                true,
                true,
                "PHB".into(),
                "phb".into(),
                "BTC".into(),
                "btc".into(),
                None,
                None,
                None,
                None,
                None,
                "phb".into(),
                None,
                Precision::ByTick { tick: dec!(0) },
                Precision::ByTick { tick: dec!(0) },
            );
            let test = binance_builder
                .exchange
                .get_order_trades(&currency_pair_metadata, &order_ref)
                .await
                .expect("in test");
            dbg!(&test);

            order_proxy
                .cancel_order_or_fail(&order_ref, binance_builder.exchange.clone())
                .await;
        }

        Err(error) => {
            assert!(false, "Create order failed with error {:?}.", error)
        }
    }
}
