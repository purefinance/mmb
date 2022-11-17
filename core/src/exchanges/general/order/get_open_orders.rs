use crate::exchanges::general::request_type::RequestType;
use crate::misc::time::time_manager;
use crate::{exchanges::general::exchange::Exchange, exchanges::general::features::OpenOrdersType};
use anyhow::bail;
use itertools::Itertools;
use mmb_domain::order::snapshot::{
    ClientOrderId, OrderHeader, OrderInfo, OrderOptions, OrderSimpleProps, OrderSnapshot,
};
use mmb_utils::cancellation_token::CancellationToken;
use tokio::time::Duration;

impl Exchange {
    pub async fn get_open_orders(
        &self,
        add_missing_open_orders: bool,
    ) -> anyhow::Result<Vec<OrderInfo>> {
        // Bugs on exchange server can lead to Err even if order was opened
        const MAX_COUNT: i32 = 5;
        let mut count = 0;
        loop {
            match self.get_open_orders_core(add_missing_open_orders).await {
                Ok(gotten_orders) => return Ok(gotten_orders),
                Err(error) => {
                    count += 1;
                    if count < MAX_COUNT {
                        log::warn!("{}", error);
                    } else {
                        return Err(error);
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    // Bugs on exchange server can lead to Err even if order was opened
    async fn get_open_orders_core(
        &self,
        check_missing_orders: bool,
    ) -> anyhow::Result<Vec<OrderInfo>> {
        let open_orders = match self.features.open_orders_type {
            OpenOrdersType::AllCurrencyPair => {
                self.timeout_manager
                    .reserve_when_available(
                        self.exchange_account_id,
                        RequestType::GetOpenOrders,
                        None,
                        CancellationToken::default(),
                    )
                    .await
                    .into_result()?;

                self.exchange_client.get_open_orders().await?
            }
            OpenOrdersType::OneCurrencyPair => {
                let currency_pair_orders =
                    futures::future::join_all(self.symbols.iter().map(|x| async move {
                        self.timeout_manager
                            .reserve_when_available(
                                self.exchange_account_id,
                                RequestType::GetOpenOrders,
                                None,
                                CancellationToken::default(),
                            )
                            .await
                            .into_result()?;
                        self.exchange_client
                            .get_open_orders_by_currency_pair(x.currency_pair())
                            .await
                    }))
                    .await;

                currency_pair_orders
                    .into_iter()
                    .flatten_ok()
                    .try_collect()?
            }
            OpenOrdersType::None => bail!(
                "Unsupported open_orders_type: {:?}",
                self.features.open_orders_type
            ),
        };

        if check_missing_orders {
            self.add_missing_open_orders(&open_orders);
        }

        Ok(open_orders)
    }

    fn add_missing_open_orders(&self, open_orders: &[OrderInfo]) {
        for order_info in open_orders {
            if order_info.client_order_id.as_str().is_empty()
                && self
                    .orders
                    .cache_by_client_id
                    .contains_key(&order_info.client_order_id)
                || self
                    .orders
                    .cache_by_exchange_id
                    .contains_key(&order_info.exchange_order_id)
            {
                log::trace!(
                    "Open order was already added {} {} {}",
                    order_info.client_order_id,
                    order_info.exchange_order_id,
                    self.exchange_account_id,
                );
                continue;
            }

            let id_for_new_header = if order_info.client_order_id.as_str().is_empty() {
                ClientOrderId::unique_id()
            } else {
                order_info.client_order_id.clone()
            };

            let new_header = OrderHeader::with_options(
                id_for_new_header,
                self.exchange_account_id,
                order_info.currency_pair,
                order_info.order_side,
                order_info.amount,
                OrderOptions::unknown(Some(order_info.price)),
                None,
                None,
                "MissedOpenOrder".to_string(),
            );

            let props = OrderSimpleProps::new(
                time_manager::now(),
                None,
                Some(order_info.exchange_order_id.clone()),
                order_info.order_status,
                None,
            );
            let new_snapshot = OrderSnapshot {
                props,
                header: new_header,
                // to fill this property we need to send several requests to the exchange,
                // as so as this one not required for our current tasks , we decide to refuse
                // it for better performance and reliability of graceful shutdown
                fills: Default::default(),
                status_history: Default::default(),
                internal_props: Default::default(),
                extension_data: order_info.extension_data.clone(),
            };

            let new_order = self.orders.add_snapshot_initial(&new_snapshot);

            self.orders
                .cache_by_exchange_id
                .insert(order_info.exchange_order_id.clone(), new_order);

            log::trace!(
                "Added open order {} {} on {}",
                order_info.client_order_id,
                order_info.exchange_order_id,
                self.exchange_account_id,
            );
        }
    }
}
