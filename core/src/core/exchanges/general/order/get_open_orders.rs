use crate::core::exchanges::common::{CurrencyPair, RestRequestOutcome};
use crate::core::exchanges::general::request_type::RequestType;
use crate::core::orders::order::{
    ClientOrderId, OrderExecutionType, OrderHeader, OrderInfo, OrderSimpleProps, OrderSnapshot,
    OrderType,
};
use mmb_utils::cancellation_token::CancellationToken;

use crate::core::{
    exchanges::general::exchange::Exchange, exchanges::general::features::OpenOrdersType,
};
use anyhow::Error;
use anyhow::{bail, Context};
use parking_lot::RwLock;

use std::sync::Arc;
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

    async fn request_when_available_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<RestRequestOutcome, Error> {
        self.timeout_manager
            .reserve_when_available(
                self.exchange_account_id,
                RequestType::GetOpenOrders,
                None,
                CancellationToken::default(),
            )?
            .await
            .into_result()?;
        self.exchange_client
            .request_open_orders_by_currency_pair(currency_pair)
            .await
    }

    // Bugs on exchange server can lead to Err even if order was opened
    async fn get_open_orders_core(
        &self,
        check_missing_orders: bool,
    ) -> anyhow::Result<Vec<OrderInfo>> {
        let mut open_orders = Vec::new();
        match self.features.open_orders_type {
            OpenOrdersType::AllCurrencyPair => {
                self.timeout_manager
                    .reserve_when_available(
                        self.exchange_account_id,
                        RequestType::GetOpenOrders,
                        None,
                        CancellationToken::default(),
                    )?
                    .await
                    .into_result()?;
                let response = self.exchange_client.request_open_orders().await?;

                log::info!(
                    "get_open_orders() response on {}: {:?}",
                    self.exchange_account_id,
                    response
                );

                if let Some(error) = self.get_rest_error(&response) {
                    Err(error).context("From request get_open_orders by all currency pair")?;
                }

                match self.exchange_client.parse_open_orders(&response) {
                    Ok(ref mut open_orders_tmp) => open_orders.append(open_orders_tmp),
                    Err(error) => {
                        self.handle_parse_error(error, &response, "".into(), None)?;
                        return Ok(Vec::new());
                    }
                }
            }

            OpenOrdersType::OneCurrencyPair => {
                // TODO other actions here have to be written after build_symbol() implementation
                let responses = futures::future::join_all(
                    self.symbols
                        .iter()
                        .map(|x| self.request_when_available_by_currency_pair(x.currency_pair())),
                )
                .await;
                for response in responses {
                    let response = &response.with_context(|| {
                        format!("From get_open_orders() on {}", self.exchange_account_id)
                    })?;

                    if let Some(error) = self.get_rest_error(response) {
                        Err(error).context("From request get_open_orders by currency pair")?;
                    }
                    match self.exchange_client.parse_open_orders(response) {
                        Ok(ref mut orders) => open_orders.append(orders),
                        Err(error) => {
                            self.handle_parse_error(error, response, "".into(), None)?;
                            return Ok(Vec::new());
                        }
                    }
                }
            }
            OpenOrdersType::None => bail!(
                "Unsupported open_orders_type: {:?}",
                self.features.open_orders_type
            ),
        }

        if check_missing_orders {
            self.add_missing_open_orders(&open_orders);
        }

        Ok(open_orders)
    }

    fn add_missing_open_orders(&self, open_orders: &Vec<OrderInfo>) {
        for order in open_orders {
            if order.client_order_id.as_str().is_empty()
                && self
                    .orders
                    .cache_by_client_id
                    .contains_key(&order.client_order_id)
                || self
                    .orders
                    .cache_by_exchange_id
                    .contains_key(&order.exchange_order_id)
            {
                log::trace!(
                    "Open order was already added {} {} {}",
                    order.client_order_id,
                    order.exchange_order_id,
                    self.exchange_account_id,
                );
                continue;
            }

            let id_for_new_header: ClientOrderId;
            if order.client_order_id.as_str().is_empty() {
                id_for_new_header = ClientOrderId::unique_id();
            } else {
                id_for_new_header = order.client_order_id.clone();
            }
            let new_header = OrderHeader::new(
                id_for_new_header,
                chrono::Utc::now(),
                self.exchange_account_id,
                order.currency_pair,
                OrderType::Unknown,
                order.order_side,
                order.amount,
                OrderExecutionType::None,
                None,
                None,
                "MissedOpenOrder".to_string(),
            );

            let props = OrderSimpleProps::new(
                Some(order.price),
                None,
                Some(order.exchange_order_id.clone()),
                Default::default(),
                Default::default(),
                order.order_status,
                None,
            );
            let new_snapshot = Arc::new(RwLock::new(OrderSnapshot {
                props,
                header: new_header,
                // to fill this property we need to send several requests to the exchange,
                // as so as this one not required for our current tasks , we decide to refuse
                // it for better performance and reliability of graceful shutdown
                fills: Default::default(),
                status_history: Default::default(),
                internal_props: Default::default(),
            }));

            let new_order = self.orders.add_snapshot_initial(new_snapshot);

            self.orders
                .cache_by_exchange_id
                .insert(order.exchange_order_id.clone(), new_order);

            log::trace!(
                "Added open order {} {} on {}",
                order.client_order_id,
                order.exchange_order_id,
                self.exchange_account_id,
            );
        }
    }
}
