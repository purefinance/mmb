use anyhow::Result;
use futures::Future;
use log::info;
use mmb_lib::core::{
    connectivity::connectivity_manager::ConnectivityManager,
    connectivity::websocket_actor::WebSocketParams,
    exchanges::events::AllowedEventSourceType,
    exchanges::general::commission::Commission,
    exchanges::general::exchange::Exchange,
    exchanges::general::features::ExchangeFeatures,
    exchanges::general::features::OpenOrdersType,
    exchanges::timeouts::timeout_manager::TimeoutManager,
    exchanges::{binance::binance::Binance, common::ExchangeAccountId},
    settings::ExchangeSettings,
};
use parking_lot::Mutex;
use std::{collections::HashMap, time::Duration};
use std::{
    pin::Pin,
    sync::{mpsc::channel, Arc},
};
use tokio::{sync::oneshot, time::sleep};

// TODO Not a unit test. Should be moved to integration tests
#[actix_rt::test]
pub async fn should_connect_and_reconnect_normally() {
    const EXPECTED_CONNECTED_COUNT: u32 = 3;

    let (finish_sender, finish_receiver) = oneshot::channel::<()>();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let websocket_host = "wss://stream.binance.com:9443".into();
    let currency_pairs = vec!["phbbtc".into(), "btcusdt".into()];
    let channels = vec!["depth".into(), "aggTrade".into()];
    let exchange_client = Box::new(Binance::new(
        ExchangeSettings::default(),
        exchange_account_id.clone(),
    ));

    let (tx, _rx) = channel();
    let exchange = Exchange::new(
        exchange_account_id.clone(),
        websocket_host,
        currency_pairs,
        channels,
        exchange_client,
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        TimeoutManager::new(HashMap::new()),
        Commission::default(),
    );

    let exchange_weak = Arc::downgrade(&exchange);
    let connectivity_manager = ConnectivityManager::new(exchange_account_id.clone());

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
