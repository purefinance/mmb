use crate::core::{
    exchanges::general::exchange::Exchange, exchanges::general::features::OpenOrdersType,
    orders::order::OrderInfo,
};
use anyhow::{anyhow, bail};
use log::{info, warn};
use tokio::time::Duration;

impl Exchange {
    pub async fn get_open_orders(&self) -> anyhow::Result<Vec<OrderInfo>> {
        // Bugs on exchange server can lead to Err even if order was opened
        const TIMEOUT: Duration = Duration::from_secs(1);
        const MAX_COUNT: i32 = 5;
        let mut count = 0;
        loop {
            match self.get_open_orders_impl().await {
                Ok(gotten_orders) => return Ok(gotten_orders),
                Err(error) => {
                    count += 1;
                    if count < MAX_COUNT {
                        warn!("{}", error);
                    } else {
                        return Err(error);
                    }
                }
            }
            let _ = tokio::time::sleep(TIMEOUT).await;
        }
    }

    // Bugs on exchange server can lead to Err even if order was opened
    async fn get_open_orders_impl(&self) -> anyhow::Result<Vec<OrderInfo>> {
        match self.features.open_orders_type {
            OpenOrdersType::AllCurrencyPair => {
                // TODO implement in the future
                //reserve_when_acailable().await
                let response = self.exchange_client.request_open_orders().await?;

                info!(
                    "get_open_orders() response on {}: {:?}",
                    self.exchange_account_id, response
                );

                if let Some(error) = self.get_rest_error(&response) {
                    bail!(
                        "Rest error appeared during request get_open_orders: {}",
                        error.message
                    )
                }

                match self.exchange_client.parse_open_orders(&response) {
                    open_orders @ Ok(_) => {
                        return open_orders;
                    }
                    Err(error) => {
                        self.handle_parse_error(error, response, "".into(), None)?;
                        return Ok(Vec::new());
                    }
                }
            }
            OpenOrdersType::OneCurrencyPair => {
                // TODO implement in the future
                //reserve_when_acailable().await
                // TODO other actions here have to be written after build_metadata() implementation

                return Err(anyhow!(""));
            }
            _ => bail!(
                "Unsupported open_orders_type: {:?}",
                self.features.open_orders_type
            ),
        }

        // TODO Prolly should to be moved in first and second branches in match above
        //if (add_missing_open_orders) {
        //    add_missing_open_orders(openOrders);
        //}
    }
}
