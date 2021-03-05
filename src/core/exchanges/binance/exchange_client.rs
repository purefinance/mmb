use super::binance::Binance;
use crate::core::exchanges::common::{CurrencyPair, RestRequestOutcome};
use crate::core::exchanges::rest_client;
use crate::core::exchanges::traits::{ExchangeClient, Support};
use crate::core::orders::order::*;
use async_trait::async_trait;

#[async_trait(?Send)]
impl ExchangeClient for Binance {
    async fn create_order(&self, order: &OrderCreating) -> RestRequestOutcome {
        let specific_currency_pair = self.get_specific_currency_pair(&order.header.currency_pair);

        let mut parameters = vec![
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
            parameters.push(("timeInForce".to_owned(), "GTC".to_owned()));
            parameters.push(("price".to_owned(), order.price.to_string()));
        }

        if order.header.execution_type == OrderExecutionType::MakerOnly {
            parameters.push(("timeInForce".to_owned(), "GTX".to_owned()));
        }

        // TODO What is marging trading?
        let url_path = if self.settings.is_marging_trading {
            "/fapi/v1/order"
        } else {
            "/api/v3/order"
        };
        let full_url = format!("{}{}", self.settings.rest_host, url_path);

        self.add_authentification_headers(&mut parameters);

        rest_client::send_post_request(&full_url, &self.settings.api_key, &parameters).await
    }

    // TODO not implemented correctly
    async fn get_account_info(&self) {
        let mut parameters = rest_client::HttpParams::new();

        self.add_authentification_headers(&mut parameters);

        let path_to_get_account_data = "/api/v3/account";
        let full_url = format! {"{}{}", self.settings.rest_host, path_to_get_account_data};

        rest_client::send_get_request(&full_url, &self.settings.api_key, &parameters).await;
    }

    // TODO not implemented correctly
    async fn cancel_order(&self, order: &OrderCancelling) -> RestRequestOutcome {
        let specific_currency_pair = self.get_specific_currency_pair(&order.currency_pair);
        let mut parameters = rest_client::HttpParams::new();
        parameters.push((
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        ));
        parameters.push(("orderId".to_owned(), order.order_id.as_str().to_owned()));

        let url_path = if self.settings.is_marging_trading {
            "/fapi/v1/order"
        } else {
            "/api/v3/order"
        };
        let full_url = format!("{}{}", self.settings.rest_host, url_path);

        self.add_authentification_headers(&mut parameters);

        let outcome =
            rest_client::send_delete_request(&full_url, &self.settings.api_key, &parameters).await;

        outcome
    }

    async fn request_open_orders(&self) -> RestRequestOutcome {
        let mut parameters = rest_client::HttpParams::new();
        let url_path = if self.settings.is_marging_trading {
            "/fapi/v1/openOrders"
        } else {
            "/api/v3/openOrders"
        };
        let full_url = format!("{}{}", self.settings.rest_host, url_path);

        self.add_authentification_headers(&mut parameters);
        let orders =
            rest_client::send_get_request(&full_url, &self.settings.api_key, &parameters).await;

        orders
    }

    // TODO not implemented correctly
    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) {
        let specific_currency_pair = self.get_specific_currency_pair(&currency_pair);
        let path_to_delete = "/api/v3/openOrders";
        let mut full_url = self.settings.rest_host.clone();
        full_url.push_str(path_to_delete);

        let mut parameters = rest_client::HttpParams::new();
        parameters.push((
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        ));

        self.add_authentification_headers(&mut parameters);

        let _cancel_order_outcome =
            rest_client::send_delete_request(&full_url, &self.settings.api_key, &parameters).await;
    }
}
