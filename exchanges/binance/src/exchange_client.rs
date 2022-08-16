use super::binance::Binance;
use crate::support::{BinanceOrderInfo, BinancePosition};
use anyhow::{Context, Result};
use async_trait::async_trait;
use function_name::named;
use itertools::Itertools;
use mmb_core::exchanges::common::{
    ActivePosition, ClosedPosition, CurrencyPair, ExchangeError, Price,
};
use mmb_core::exchanges::events::ExchangeBalancesAndPositions;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_core::exchanges::general::order::cancel::CancelOrderResult;
use mmb_core::exchanges::general::order::create::CreateOrderResult;
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::general::request_type::RequestType;
use mmb_core::exchanges::general::symbol::Symbol;
use mmb_core::exchanges::rest_client::UriBuilder;
use mmb_core::exchanges::traits::{ExchangeClient, Support};
use mmb_core::orders::fill::EventSourceType;
use mmb_core::orders::order::*;
use mmb_core::orders::pool::OrderRef;
use mmb_utils::DateTime;
use std::sync::Arc;

#[async_trait]
impl ExchangeClient for Binance {
    async fn create_order(&self, order: &OrderRef) -> CreateOrderResult {
        match self.request_create_order(order).await {
            Ok(request_outcome) => match self.get_order_id(&request_outcome) {
                Ok(order_id) => CreateOrderResult::succeed(&order_id, EventSourceType::Rest),
                Err(error) => CreateOrderResult::failed(error, EventSourceType::Rest),
            },
            Err(err) => CreateOrderResult::failed(err, EventSourceType::Rest),
        }
    }

    async fn cancel_order(&self, order: OrderCancelling) -> CancelOrderResult {
        let order_header = order.header.clone();

        match self.request_cancel_order(order).await {
            Ok(_) => CancelOrderResult::succeed(
                order_header.client_order_id.clone(),
                EventSourceType::Rest,
                None,
            ),
            Err(err) => CancelOrderResult::failed(err, EventSourceType::Rest),
        }
    }

    #[named]
    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> Result<()> {
        let specific_currency_pair = self.get_specific_currency_pair(currency_pair);

        let mut builder = UriBuilder::from_path("/api/v3/openOrders");
        builder.add_kv("symbol", &specific_currency_pair);
        self.add_authentification(&mut builder);

        let uri = builder.build_uri(&self.hosts.rest_uri_host(), true);

        let api_key = &self.settings.api_key;
        self.rest_client
            .delete(uri, api_key, function_name!(), String::new())
            .await?;

        Ok(())
    }

    async fn get_open_orders(&self) -> Result<Vec<OrderInfo>> {
        let response = self.request_open_orders().await?;

        Ok(self.parse_open_orders(&response))
    }

    async fn get_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<Vec<OrderInfo>> {
        let response = self
            .request_open_orders_by_currency_pair(currency_pair)
            .await?;

        Ok(self.parse_open_orders(&response))
    }

    async fn get_order_info(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        match self.request_order_info(order).await {
            Ok(request_outcome) => Ok(self.parse_order_info(&request_outcome)),
            Err(error) => Err(ExchangeError::parsing(error.to_string())),
        }
    }

    async fn close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> Result<ClosedPosition> {
        let response = self.request_close_position(position, price).await?;
        let binance_order: BinanceOrderInfo = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_open_orders request");

        Ok(ClosedPosition::new(
            ExchangeOrderId::from(binance_order.exchange_order_id.to_string().as_ref()),
            binance_order.orig_quantity,
        ))
    }

    async fn get_active_positions(&self) -> Result<Vec<ActivePosition>> {
        let response = self.request_get_position().await?;
        let binance_positions: Vec<BinancePosition> = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_active_positions_core request");

        Ok(binance_positions
            .into_iter()
            .map(|x| self.binance_position_to_active_position(x))
            .collect_vec())
    }

    async fn get_balance(&self, is_spot: bool) -> Result<ExchangeBalancesAndPositions> {
        let response = match is_spot {
            true => self.request_get_balance_spot().await?,
            false => self.request_get_balance().await?,
        };

        Ok(self.parse_get_balance(&response))
    }

    async fn get_my_trades(
        &self,
        symbol: &Symbol,
        last_date_time: Option<DateTime>,
    ) -> RequestResult<Vec<OrderTrade>> {
        // TODO Add metric UseTimeMetric(RequestType::GetMyTrades)
        match self.request_my_trades(symbol, last_date_time).await {
            Ok(response) => match self.parse_get_my_trades(&response, last_date_time) {
                Ok(data) => RequestResult::Success(data),
                Err(_) => RequestResult::Error(ExchangeError::unknown(&response.content)),
            },
            Err(err) => RequestResult::Error(ExchangeError::parsing(err.to_string())),
        }
    }

    async fn build_all_symbols(&self) -> Result<Vec<Arc<Symbol>>> {
        let response = &self.request_all_symbols().await?;
        self.parse_all_symbols(response)
    }
}

impl Binance {
    #[named]
    async fn get_listen_key(&self) -> Result<String> {
        let request_outcome = self
            .request_listen_key()
            .await
            .context(concat!("request in ", function_name!()))?;

        Self::parse_listen_key(&request_outcome).context(concat!("parse in ", function_name!()))
    }

    pub(super) async fn receive_listen_key(&self) -> String {
        const MAX_ATTEMPTS_COUNT: u8 = 10;
        for attempt in 0..MAX_ATTEMPTS_COUNT {
            self.timeout_manager
                .reserve_when_available(
                    self.settings.exchange_account_id.clone(),
                    RequestType::GetListenKey,
                    None,
                    self.lifetime_manager.stop_token(),
                )
                .await;

            match self.get_listen_key().await {
                Ok(listen_key) => return listen_key,
                Err(err) if attempt < MAX_ATTEMPTS_COUNT => {
                    log::warn!("Failed get_listen_key attempt {attempt}: {err:?}")
                }
                Err(err) => panic!("Failed get_listen_key attempt {attempt}: {err:?}"),
            }
        }

        unreachable!()
    }

    pub(crate) async fn ping_listen_key(&self) {
        // TODO check is_trading

        let exchange_account_id = self.settings.exchange_account_id;
        log::trace!("Updating listenKey {exchange_account_id}");
        if self.listen_key.read().is_none() {
            log::warn!("Skipping listenKey update when websocket is not connected on {exchange_account_id}");
            return;
        }

        self.timeout_manager
            .reserve_when_available(
                exchange_account_id,
                RequestType::UpdateListenKey,
                None,
                self.lifetime_manager.stop_token(),
            )
            .await;

        let listen_key = match self.listen_key.read().clone() {
            None => {
                log::warn!("Skipping listenKey update when websocket is not connected on {exchange_account_id}");
                return;
            }
            Some(v) => v,
        };

        match self.request_update_listen_key(&listen_key).await {
            Ok(_) => log::trace!("Updated listenKey"),
            Err(err) => log::warn!("Failed to update listenKey {err}"),
        }
    }
}
