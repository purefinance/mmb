use crate::core::exchanges::common::{
    Amount, CurrencyCode, ExchangeError, Price, RestRequestOutcome,
};
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::exchanges::general::exchange::RequestResult;
use crate::core::orders::fill::OrderFillType;
use crate::core::orders::order::{ExchangeOrderId, OrderRole};
use crate::core::DateTime;
use crate::core::{
    exchanges::general::{exchange::Exchange, features::RestFillsType},
    orders::pool::OrderRef,
};
use anyhow::{bail, Result};
use itertools::Itertools;
use log::info;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OrderTrade {
    pub exchange_order_id: ExchangeOrderId,
    pub trade_id: String,
    pub datetime: DateTime,
    pub price: Price,
    pub amount: Amount,
    pub order_role: OrderRole,
    pub fee_currency_code: CurrencyCode,
    pub fee_rate: Option<Price>,
    pub fee_amount: Option<Amount>,
    pub fill_type: OrderFillType,
}

impl OrderTrade {
    pub fn new(
        exchange_order_id: ExchangeOrderId,
        trade_id: String,
        datetime: DateTime,
        price: Price,
        amount: Amount,
        order_role: OrderRole,
        fee_currency_code: CurrencyCode,
        fee_rate: Option<Price>,
        fee_amount: Option<Amount>,
        fill_type: OrderFillType,
    ) -> Self {
        Self {
            exchange_order_id,
            trade_id,
            datetime,
            price,
            amount,
            order_role,
            fee_currency_code,
            fee_rate,
            fee_amount,
            fill_type,
        }
    }
}

impl Exchange {
    pub async fn get_order_trades(
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
                if let Some(exchange_order_id) = order.exchange_order_id() {
                    let data = my_trades
                        .into_iter()
                        .filter(|order_trade| order_trade.exchange_order_id == exchange_order_id)
                        .collect_vec();

                    Ok(RequestResult::Success(data))
                } else {
                    bail!("There is no exchange_order in order {:?}", order);
                }
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
        let response = self
            .exchange_client
            .request_my_trades(currency_pair_metadata, last_date_time)
            .await?;

        // FIXME is is_launched_from_tests necessary here?

        match self.get_rest_error(&response) {
            Some(error) => Ok(RequestResult::Error(error)),
            None => match self
                .exchange_client
                .parse_get_my_trades(&response, last_date_time)
            {
                Ok(data) => Ok(RequestResult::Success(data)),
                Err(error) => {
                    self.handle_parse_error(error, &response, "".into(), None)?;
                    Ok(RequestResult::Error(ExchangeError::unknown_error(
                        &response.content,
                    )))
                }
            },
        }
    }

    async fn get_order_trades_core(
        &self,
        order: &OrderRef,
    ) -> Result<RequestResult<Vec<OrderTrade>>> {
        match order.exchange_order_id() {
            Some(exchange_order_id) => {
                let response = self.request_order_trades_core(&exchange_order_id).await;

                info!(
                    "get_order_trades_core response {} {:?} on {} {:?}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                    response
                );

                match self.get_rest_error(&response) {
                    Some(error) => Ok(RequestResult::Error(error)),
                    None => match self.parse_get_order_trades_core(&response, exchange_order_id) {
                        Ok(data) => Ok(RequestResult::Success(data)),
                        Err(error) => {
                            self.handle_parse_error(error, &response, "".into(), None)?;
                            Ok(RequestResult::Error(ExchangeError::unknown_error(
                                &response.content,
                            )))
                        }
                    },
                }
            }
            None => bail!("There are no exchange_order_id in order {:?}", order),
        }
    }

    async fn request_order_trades_core(
        &self,
        _exchange_order_id: &ExchangeOrderId,
    ) -> RestRequestOutcome {
        unimplemented!()
    }

    pub(crate) fn parse_get_order_trades_core(
        &self,
        _response: &RestRequestOutcome,
        _exchange_order_id: ExchangeOrderId,
    ) -> Result<Vec<OrderTrade>> {
        unimplemented!()
    }
}
