use anyhow::Result;
use mmb_core::connectivity::{websocket_open, WebSocketParams, WebSocketRole};
use mmb_core::exchanges::general::features::*;
use mmb_core::{
    exchanges::common::ExchangeAccountId, exchanges::events::AllowedEventSourceType,
    exchanges::general::commission::Commission, exchanges::general::features::ExchangeFeatures,
    exchanges::general::features::OpenOrdersType,
};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::init_infrastructure;
use tokio::time::{timeout, Duration};

use crate::binance::binance_builder::BinanceBuilder;

async fn init_test_stuff() -> Result<(ExchangeAccountId, WebSocketParams, WebSocketParams)> {
    let exchange_account_id: ExchangeAccountId = "Binance_0".parse().expect("in test");
    let exchange = BinanceBuilder::try_new(
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
    .await?
    .exchange;

    let main = exchange
        .get_websocket_params(WebSocketRole::Main)
        .await
        .expect("Failed to get Main WebSocket params");

    let secondary = exchange
        .get_websocket_params(WebSocketRole::Secondary)
        .await
        .expect("Failed to get Secondary WebSocket params");

    Ok((exchange_account_id, main, secondary))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
pub async fn connect_disconnect() {
    init_infrastructure("log.txt");
    let (account, main, secondary) = match init_test_stuff().await {
        Ok((account, main, secondary)) => (account, main, secondary),
        Err(_) => {
            log::error!("failed to init test stuff, disabling test (missing API keys?)");
            return;
        }
    };

    for _ in 0..3 {
        let (sender, mut receiver) = websocket_open(account, main.clone(), Some(secondary.clone()))
            .await
            .expect("in test");

        // receive first message
        // should arrive in few milliseconds (on production)
        let data = timeout(Duration::from_secs(10), receiver.recv())
            .await
            .expect("in test");

        log::info!("RECEIVED: {}", data.expect("in test"));

        // close connection
        drop(sender);

        // drain whole channel
        let future = async move {
            while let Some(msg) = receiver.recv().await {
                log::info!("RECEIVED ON DRAIN: {}", msg);
            }
        };

        // should drain very quickly
        timeout(Duration::from_millis(10), future)
            .await
            .expect("in test");
    }
}
