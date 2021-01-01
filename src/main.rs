use crate::core::connectivity::web_socket_client::WebSocketClient;
use crate::core::exchanges::common::{ExchangeId, ExchangeName};
use std::thread;
use std::time;
use std::sync::Arc;

pub mod core;

fn main() {
    println!("Hello, world!");

    let exchange_id: ExchangeId = "binance1".into();
    let exchange_name: ExchangeName = "binance".into();

    let client = Arc::new(WebSocketClient::new(exchange_id, exchange_name));

    let uri = "wss://stream.binance.com:9443/stream?streams=bnbbtc@depth";

    let time = time::Instant::now();

    let handle = {
        let client_clone = client.clone();
        let handle = thread::spawn(move || client_clone.open(uri));
        println!("time: {}", time.elapsed().as_secs());
        handle
    };

    thread::sleep(time::Duration::from_secs(2));
    println!("time: {}", time.elapsed().as_secs());
    client.send(r#"{
  "method": "SUBSCRIBE",
  "params": [
    "btcusdt@aggTrade"
  ],
  "id": 1
}"#.to_string());

    thread::sleep(time::Duration::from_secs(2));
    println!("time: {}", time.elapsed().as_secs());
    client.send(r#"{
  "method": "LIST_SUBSCRIPTIONS",
  "id": 2
}"#.to_string());

    thread::sleep(time::Duration::from_secs(2));
    println!("time: {}", time.elapsed().as_secs());
    client.send(r#"{
  "method": "UNSUBSCRIBE",
  "params": [
    "bnbbtc@depth"
  ],
  "id": 3
}"#.to_string());

    thread::sleep(time::Duration::from_secs(2));
    println!("time: {}", time.elapsed().as_secs());
    client.send(r#"{
  "method": "LIST_SUBSCRIPTIONS",
  "id": 4
}"#.to_string());


    thread::sleep(time::Duration::from_secs(2));
    println!("time: {}", time.elapsed().as_secs());
    client.close();

    handle.join();
}