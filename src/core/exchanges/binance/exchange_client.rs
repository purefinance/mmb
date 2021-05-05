use super::binance::Binance;
use crate::core::exchanges::common::{CurrencyPair, RestRequestOutcome};
use crate::core::exchanges::rest_client;
use crate::core::exchanges::traits::{ExchangeClient, Support};
use crate::core::orders::order::*;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
impl ExchangeClient for Binance {
    async fn request_metadata(&self) -> Result<RestRequestOutcome> {
        // TODO implement request metadata
        Ok(RestRequestOutcome::new(
            "".into(),
            awc::http::StatusCode::OK,
        ))
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
        }

        if order.header.execution_type == OrderExecutionType::MakerOnly {
            http_params.push(("timeInForce".to_owned(), "GTX".to_owned()));
        }
        self.add_authentification_headers(&mut http_params)?;

        // TODO What is marging trading?
        let url_path = match self.settings.is_marging_trading {
            true => "/fapi/v1/order",
            false => "/api/v3/order",
        };

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &vec![])?;

        rest_client::send_post_request(full_url, &self.settings.api_key, &http_params).await
    }

    async fn request_cancel_order(&self, order: &OrderCancelling) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(&order.header.currency_pair);

        let url_path = match self.settings.is_marging_trading {
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

        let outcome = rest_client::send_delete_request(full_url, &self.settings.api_key).await?;

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

        let _cancel_order_outcome =
            rest_client::send_delete_request(full_url, &self.settings.api_key).await;

        Ok(())
    }

    async fn request_open_orders(&self) -> Result<RestRequestOutcome> {
        let url_path = match self.settings.is_marging_trading {
            true => "/fapi/v1/openOrders",
            false => "/api/v3/openOrders",
        };

        let mut http_params = rest_client::HttpParams::new();
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &http_params)?;

        let orders = rest_client::send_get_request(full_url, &self.settings.api_key).await;

        orders
    }

    async fn request_order_info(&self, order: &OrderSnapshot) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(&order.header.currency_pair);

        let url_path = match self.settings.is_marging_trading {
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
                order.header.client_order_id.as_str().to_owned(),
            ),
        ];
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &http_params)?;

        rest_client::send_get_request(full_url, &self.settings.api_key).await
    }
}
