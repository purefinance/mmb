use crate::core::exchanges::common::{ExchangeError, RestRequestOutcome};
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::exchanges::general::exchange::RequestResult;
use crate::core::orders::order::ExchangeOrderId;
use crate::core::DateTime;
use crate::core::{
    exchanges::general::{exchange::Exchange, features::RestFillsType},
    orders::pool::OrderRef,
};
use anyhow::{bail, Result};
use itertools::Itertools;
use log::info;

pub(crate) struct OrderTrade {
    pub exchange_order_id: Option<ExchangeOrderId>,
}

impl Exchange {
    pub(crate) async fn get_order_trades(
        &self,
        currency_pair_metadata: &CurrencyPairMetadata,
        order: &OrderRef,
    ) -> Result<RequestResult<Vec<OrderTrade>>> {
        let fills_type = &self.features.rest_fills_features.fills_type;
        match fills_type {
            RestFillsType::OrderTrades => self.get_order_trades_core(order).await,
            RestFillsType::MyTrades => {
                self.get_my_trades_with_filter(currency_pair_metadata, order)
                    .await
            }
            _ => bail!("Fills type {:?} is not supported", fills_type),
        }
    }

    async fn get_my_trades_with_filter(
        &self,
        currency_pair_metadata: &CurrencyPairMetadata,
        order: &OrderRef,
    ) -> Result<RequestResult<Vec<OrderTrade>>> {
        let my_trades = self.get_my_trades(currency_pair_metadata, None).await?;
        match my_trades {
            RequestResult::Error(_) => Ok(my_trades),
            RequestResult::Success(my_trades) => {
                let data = my_trades
                    .into_iter()
                    .filter(|order_trade| {
                        order_trade.exchange_order_id == order.exchange_order_id()
                    })
                    .collect_vec();

                Ok(RequestResult::Success(data))
            }
        }
    }

    pub(crate) async fn get_my_trades(
        &self,
        currency_pair_metadata: &CurrencyPairMetadata,
        last_date_time: Option<DateTime>,
    ) -> Result<RequestResult<Vec<OrderTrade>>> {
        // FIXME What does this comment mean? Should we keep it in rust?
        // using var timer = UseTimeMetric(ExchangeRequestType.GetMyTrades);
        let response = self.request_my_trades(currency_pair_metadata, last_date_time);

        // FIXME is is_launched_from_tests necessary here?

        match self.get_rest_error(&response) {
            Some(error) => {
                // FIXME remove return;
                return Ok(RequestResult::Error(error));
            }

            None => match self.parse_get_my_trades(&response, last_date_time) {
                Ok(data) => return Ok(RequestResult::Success(data)),
                Err(error) => {
                    self.handle_parse_error(error, &response, "".into(), None)?;
                    return Ok(RequestResult::Error(ExchangeError::unknown_error(
                        &response.content,
                    )));
                }
            },
        }
    }

    // TODO implement
    pub(crate) fn request_my_trades(
        &self,
        _currency_pair_metadata: &CurrencyPairMetadata,
        _last_date_time: Option<chrono::DateTime<chrono::Utc>>,
    ) -> RestRequestOutcome {
        unimplemented!()
    }

    pub(crate) fn parse_get_my_trades(
        &self,
        _response: &RestRequestOutcome,
        _last_date_time: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<OrderTrade>> {
        unimplemented!()
    }

    async fn get_order_trades_core(
        &self,
        order: &OrderRef,
    ) -> Result<RequestResult<Vec<OrderTrade>>> {
        match order.exchange_order_id() {
            Some(exchange_order_id) => {
                let response = self.request_order_trades_core(&exchange_order_id).await;

                // TODO complete when request_order_trades_core will be implemented
                info!(
                    "get_order_trades_core response {} {:?} on {} {:?}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                    response
                );

                if let Some(error) = self.get_rest_error(&response) {
                    bail!(
                        "Rest error appeared during request get_open_orders: {}",
                        error.message
                    )
                }
            }
            None => bail!("There are no exchange_order_id in order {:?}", order),
        }
        todo!()
    }

    async fn request_order_trades_core(
        &self,
        _exchange_order_id: &ExchangeOrderId,
    ) -> RestRequestOutcome {
        unimplemented!()
    }
}
