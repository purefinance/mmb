use super::binance::Binance;
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::exchanges::rest_client;
use crate::core::exchanges::traits::{ExchangeClient, Support};
use crate::core::orders::order::*;
use crate::core::DateTime;
use crate::core::{
    exchanges::common::{CurrencyPair, RestRequestOutcome},
    orders::pool::OrderRef,
};
use anyhow::{bail, Result};
use async_trait::async_trait;
use chrono::Utc;

#[async_trait]
impl ExchangeClient for Binance {
    async fn request_metadata(&self) -> Result<RestRequestOutcome> {
        // In currenct versions works only with Spot market
        let url_path = "/api/v3/exchangeInfo";
        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &vec![])?;

        self.rest_client.get(full_url, &self.settings.api_key).await
    }

    async fn create_order(&self, order: &OrderCreating) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(&order.header.currency_pair);

        let mut http_params = vec![
            (
                "symbol".to_owned(),
                specific_currency_pair.as_str().to_owned(),
            ),
            (
                "side".to_owned(),
                Self::to_server_order_side(order.header.side),
            ),
            (
                "type".to_owned(),
                Self::to_server_order_type(order.header.order_type),
            ),
            ("quantity".to_owned(), order.header.amount.to_string()),
            (
                "newClientOrderId".to_owned(),
                order.header.client_order_id.as_str().to_owned(),
            ),
        ];

        if order.header.order_type != OrderType::Market {
            http_params.push(("timeInForce".to_owned(), "GTC".to_owned()));
            http_params.push(("price".to_owned(), order.price.to_string()));
        } else if order.header.execution_type == OrderExecutionType::MakerOnly {
            http_params.push(("timeInForce".to_owned(), "GTX".to_owned()));
        }
        self.add_authentification_headers(&mut http_params)?;

        let url_path = match self.settings.is_margin_trading {
            true => "/fapi/v1/order",
            false => "/api/v3/order",
        };

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &vec![])?;

        self.rest_client
            .post(full_url, &self.settings.api_key, &http_params)
            .await
    }

    async fn request_cancel_order(&self, order: &OrderCancelling) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(&order.header.currency_pair);

        let url_path = match self.settings.is_margin_trading {
            true => "/fapi/v1/order",
            false => "/api/v3/order",
        };

        let mut http_params = vec![
            (
                "symbol".to_owned(),
                specific_currency_pair.as_str().to_owned(),
            ),
            (
                "orderId".to_owned(),
                order.exchange_order_id.as_str().to_owned(),
            ),
        ];
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &http_params)?;

        let outcome = self
            .rest_client
            .delete(full_url, &self.settings.api_key)
            .await?;

        Ok(outcome)
    }

    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> Result<()> {
        let specific_currency_pair = self.get_specific_currency_pair(&currency_pair);

        let host = &self.settings.rest_host;
        let path_to_delete = "/api/v3/openOrders";

        let mut http_params = vec![(
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        )];
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(host, path_to_delete, &http_params)?;

        let _cancel_order_outcome = self
            .rest_client
            .delete(full_url, &self.settings.api_key)
            .await;

        Ok(())
    }

    async fn request_open_orders(&self) -> Result<RestRequestOutcome> {
        let mut http_params = rest_client::HttpParams::new();
        self.add_authentification_headers(&mut http_params)?;

        self.request_open_orders_by_http_header(http_params).await
    }

    async fn request_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(&currency_pair);
        let mut http_params = vec![(
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        )];
        self.add_authentification_headers(&mut http_params)?;

        self.request_open_orders_by_http_header(http_params).await
    }

    async fn request_order_info(&self, order: &OrderRef) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(&order.currency_pair());

        let url_path = match self.settings.is_margin_trading {
            true => "/fapi/v1/order",
            false => "/api/v3/order",
        };

        let mut http_params = vec![
            (
                "symbol".to_owned(),
                specific_currency_pair.as_str().to_owned(),
            ),
            (
                "origClientOrderId".to_owned(),
                order.client_order_id().as_str().to_owned(),
            ),
        ];
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &http_params)?;

        self.rest_client.get(full_url, &self.settings.api_key).await
    }

    async fn request_my_trades(
        &self,
        currency_pair_metadata: &CurrencyPairMetadata,
        last_date_time: Option<DateTime>,
    ) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(&CurrencyPair::from_codes(
            &currency_pair_metadata.base_currency_code,
            &currency_pair_metadata.quote_currency_code,
        ));
        let mut http_params = vec![(
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        )];

        self.add_authentification_headers(&mut http_params)?;
        dbg!(&http_params);

        let url_path = match self.settings.is_margin_trading {
            true => "/fapi/v1/userTrades",
            false => "/api/v3/myTrades",
        };

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &http_params)?;
        dbg!(&full_url);
        self.rest_client.get(full_url, &self.settings.api_key).await
    }
}
