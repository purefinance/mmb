use mmb_core::core::connectivity::connectivity_manager::{
    ConnectivityManager, ConnectivityManagerNotifier, WebSocketRole,
};
use mmb_core::core::connectivity::websocket_connection::{WebSocketConnection, WebSocketParams};
use mmb_core::core::exchanges::common::ExchangeAccountId;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub async fn connect_and_send_msg() {
    let url: Url = "wss://stream.binance.com:9443/stream?streams=bnbbtc@depth"
        .parse()
        .expect("in test");
    let role = WebSocketRole::Main;
    let exchange_account_id = ExchangeAccountId::from_str("Binance_0").expect("in test");
    let params = WebSocketParams::new(url);
    let connectivity_manager = ConnectivityManager::new(exchange_account_id);
    let connectivity_manager_notifier =
        ConnectivityManagerNotifier::new(role, Arc::downgrade(&connectivity_manager));

    let websocket_connection = WebSocketConnection::open_connection(
        exchange_account_id,
        role,
        params,
        connectivity_manager_notifier,
    )
    .await
    .expect("Failed to connect websocket");

    assert_eq!(
        websocket_connection.is_connected(),
        true,
        "Websocket should be connected"
    );

    websocket_connection
        .send_string(
            r#"{
       "method": "SUBSCRIBE",
       "params": [
         "btcusdt@aggTrade"
       ],
       "id": 1
    }"#
            .to_string(),
        )
        .await
        .expect("Failed to send message");

    websocket_connection
        .send_force_close()
        .await
        .expect("Failed to disconnect websocket");

    assert_eq!(
        websocket_connection.is_connected(),
        false,
        "Websocket should be closed to this moment"
    );
}
