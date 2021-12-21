use crate::serum::Serum;
use anyhow::Result;
use async_trait::async_trait;
use awc::http::Uri;
use dashmap::DashMap;
use mmb_core::core::connectivity::connectivity_manager::WebSocketRole;
use mmb_core::core::exchanges::common::CurrencyPair;
use mmb_core::core::exchanges::common::{
    ActivePosition, Amount, ClosedPosition, CurrencyCode, CurrencyId, ExchangeError, Price,
    RestRequestOutcome, SpecificCurrencyPair,
};
use mmb_core::core::exchanges::events::{ExchangeBalancesAndPositions, TradeId};
use mmb_core::core::exchanges::general::handlers::handle_order_filled::FillEventData;
use mmb_core::core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::core::exchanges::general::symbol::Symbol;
use mmb_core::core::exchanges::traits::Support;
use mmb_core::core::orders::fill::EventSourceType;
use mmb_core::core::orders::order::{ClientOrderId, ExchangeOrderId, OrderInfo, OrderSide};
use mmb_core::core::settings::ExchangeSettings;
use mmb_utils::DateTime;
use std::sync::Arc;

#[async_trait]
impl Support for Serum {
    fn is_rest_error_code(&self, response: &RestRequestOutcome) -> Result<(), ExchangeError> {
        todo!()
    }

    fn get_order_id(&self, response: &RestRequestOutcome) -> Result<ExchangeOrderId> {
        todo!()
    }

    fn clarify_error_type(&self, error: &mut ExchangeError) {
        todo!()
    }

    fn on_websocket_message(&self, msg: &str) -> Result<()> {
        todo!()
    }

    fn on_connecting(&self) -> Result<()> {
        todo!()
    }

    fn set_order_created_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    ) {
        todo!()
    }

    fn set_order_cancelled_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    ) {
        todo!()
    }

    fn set_handle_order_filled_callback(
        &self,
        callback: Box<dyn FnMut(FillEventData) + Send + Sync>,
    ) {
        todo!()
    }

    fn set_handle_trade_callback(
        &self,
        callback: Box<
            dyn FnMut(CurrencyPair, TradeId, Price, Amount, OrderSide, DateTime) + Send + Sync,
        >,
    ) {
        todo!()
    }

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>) {
        todo!()
    }

    fn is_websocket_enabled(&self, role: WebSocketRole) -> bool {
        todo!()
    }

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Uri> {
        todo!()
    }

    fn get_specific_currency_pair(&self, currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        todo!()
    }

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode> {
        todo!()
    }

    fn should_log_message(&self, message: &str) -> bool {
        todo!()
    }

    fn parse_open_orders(&self, response: &RestRequestOutcome) -> Result<Vec<OrderInfo>> {
        todo!()
    }

    fn parse_order_info(&self, response: &RestRequestOutcome) -> Result<OrderInfo> {
        todo!()
    }

    fn parse_all_symbols(&self, response: &RestRequestOutcome) -> Result<Vec<Arc<Symbol>>> {
        todo!()
    }

    fn parse_get_my_trades(
        &self,
        response: &RestRequestOutcome,
        last_date_time: Option<DateTime>,
    ) -> Result<Vec<OrderTrade>> {
        todo!()
    }

    fn get_settings(&self) -> &ExchangeSettings {
        todo!()
    }

    fn parse_get_position(&self, response: &RestRequestOutcome) -> Vec<ActivePosition> {
        todo!()
    }

    fn parse_close_position(&self, response: &RestRequestOutcome) -> Result<ClosedPosition> {
        todo!()
    }

    fn parse_get_balance(&self, response: &RestRequestOutcome) -> ExchangeBalancesAndPositions {
        todo!()
    }
}
