use anyhow::Result;
use futures::Future;
use log::info;
use mmb_core::core::exchanges::general::features::*;
use mmb_core::core::lifecycle::cancellation_token::CancellationToken;
use mmb_core::core::{
    connectivity::connectivity_manager::ConnectivityManager,
    connectivity::websocket_connection::WebSocketParams, exchanges::common::ExchangeAccountId,
    exchanges::events::AllowedEventSourceType, exchanges::general::commission::Commission,
    exchanges::general::features::ExchangeFeatures, exchanges::general::features::OpenOrdersType,
};
use parking_lot::Mutex;
use std::time::Duration;
use std::{pin::Pin, sync::Arc};
use tokio::{sync::oneshot, time::sleep};

use crate::binance::binance_builder::BinanceBuilder;

#[actix_rt::test]
pub async fn should_connect_and_reconnect_normally() {
    const EXPECTED_CONNECTED_COUNT: u32 = 3;

    let (finish_sender, finish_receiver) = oneshot::channel::<()>();

    let exchange_account_id: ExchangeAccountId = "Binance_0".parse().expect("in test");
    let binance_builder = match BinanceBuilder::try_new(
        exchange_account_id,
        CancellationToken::default(),
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
        true,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let exchange_weak = Arc::downgrade(&binance_builder.exchange);
    let connectivity_manager = ConnectivityManager::new(exchange_account_id);

    let connected_count = Arc::new(Mutex::new(0));
    {
        let connected_count = connected_count.clone();
        connectivity_manager
            .clone()
            .set_callback_connected(Box::new(move || *connected_count.lock() += 1));
    }

    let get_websocket_params = Box::new(move |websocket_role| {
        let exchange = exchange_weak.upgrade().expect("in test");
        let params = exchange.get_websocket_params(websocket_role);
        Box::pin(params) as Pin<Box<dyn Future<Output = Result<WebSocketParams>>>>
    });

    for _ in 0..EXPECTED_CONNECTED_COUNT {
        let connect_result = connectivity_manager
            .clone()
            .connect(false, get_websocket_params.clone())
            .await;
        assert_eq!(
            connect_result, true,
            "websocket should connect successfully"
        );

        connectivity_manager.clone().disconnect().await;
    }

    assert_eq!(
        *connected_count.lock(),
        EXPECTED_CONNECTED_COUNT,
        "we should reconnect expected count times"
    );

    let _ = finish_sender.send(()).expect("in test");

    tokio::select! {
        _ = finish_receiver => info!("Test finished successfully"),
        _ = sleep(Duration::from_secs(10)) => panic!("Test time is gone!")
    }
}
