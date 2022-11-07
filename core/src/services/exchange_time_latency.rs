use crate::exchanges::general::exchange::Exchange;
use crate::lifecycle::trading_engine::Service;
use anyhow::Result;
use dashmap::DashMap;
use mmb_domain::market::ExchangeAccountId;
use std::sync::Arc;
use tokio::sync::oneshot::Receiver;

#[allow(dead_code)]
pub struct ExchangeTimeLatencyService {
    exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>,
}

impl Service for ExchangeTimeLatencyService {
    fn name(&self) -> &str {
        "ExchangeTimeLatencyService"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<Receiver<Result<()>>> {
        None
    }
}

impl ExchangeTimeLatencyService {
    pub fn new(exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>) -> Self {
        Self { exchanges }
    }

    // TODO Uncomment when issue #810 will be solved
    pub async fn update_server_time_latency(self: Arc<Self>) {
        // for exchange in &self.exchanges {
        //     let requests = [
        //         self.get_local_time_offset(exchange.clone()),
        //         self.get_local_time_offset(exchange.clone()),
        //         self.get_local_time_offset(exchange.clone()),
        //         self.get_local_time_offset(exchange.clone()),
        //         self.get_local_time_offset(exchange.clone()),
        //     ];
        //     let offsets = future::join_all(requests).await;
        //
        //     let (mut sum, mut len) = (0, 0);
        //     for result in offsets {
        //         match result {
        //             Ok(value) => {
        //                 sum += value;
        //                 len += 1;
        //             }
        //             Err(error) => log::error!("{error:?}"),
        //         }
        //     }
        //
        //     if 0 < len {
        //         let average_latency = sum / len;
        //         exchange.update_server_time_latency(average_latency)
        //     } else {
        //         log::error!("Has no value to calc server time latency");
        //     }
        // }
    }

    // async fn get_local_time_offset(&self, exchange: Arc<Exchange>) -> Result<i64> {
    //     let local_send_time = get_current_milliseconds();
    //     let server_time = exchange
    //         .exchange_client
    //         .get_server_time()
    //         .await
    //         .ok_or_else(|| anyhow!("Exchange doesn't support getting time"))??;
    //     let local_receive_time = get_current_milliseconds();
    //
    //     let min = local_send_time - server_time;
    //     let max = local_receive_time - server_time;
    //
    //     Ok((max + min) / 2)
    // }
}
