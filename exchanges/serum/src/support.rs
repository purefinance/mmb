use crate::helpers::FromU64Array;
use crate::serum::Serum;

use anyhow::{Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;

use mmb_core::connectivity::connectivity_manager::WebSocketRole;
use mmb_core::exchanges::common::CurrencyPair;
use mmb_core::exchanges::common::{
    ActivePosition, Amount, ClosedPosition, CurrencyCode, CurrencyId, Price, RestRequestOutcome,
    SpecificCurrencyPair,
};
use mmb_core::exchanges::events::{ExchangeBalancesAndPositions, TradeId};
use mmb_core::exchanges::general::handlers::handle_order_filled::FillEventData;
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::general::symbol::{Precision, Symbol};
use mmb_core::exchanges::traits::Support;
use mmb_core::orders::fill::EventSourceType;
use mmb_core::orders::order::{ClientOrderId, ExchangeOrderId, OrderSide};
use mmb_core::settings::ExchangeSettings;
use mmb_utils::DateTime;

use crate::market::{DeserMarketData, MarketData};
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use spl_token::state;

#[async_trait]
impl Support for Serum {
    fn get_order_id(&self, _response: &RestRequestOutcome) -> Result<ExchangeOrderId> {
        todo!()
    }

    fn on_websocket_message(&self, _msg: &str) -> Result<()> {
        unimplemented!("Not needed for implementation Serum")
    }

    fn on_connecting(&self) -> Result<()> {
        // TODO not implemented
        Ok(())
    }

    fn set_order_created_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    ) {
        *self.order_created_callback.lock() = callback;
    }

    fn set_order_cancelled_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    ) {
        *self.order_cancelled_callback.lock() = callback;
    }

    fn set_handle_order_filled_callback(
        &self,
        callback: Box<dyn FnMut(FillEventData) + Send + Sync>,
    ) {
        *self.handle_order_filled_callback.lock() = callback;
    }

    fn set_handle_trade_callback(
        &self,
        callback: Box<
            dyn FnMut(CurrencyPair, TradeId, Price, Amount, OrderSide, DateTime) + Send + Sync,
        >,
    ) {
        *self.handle_trade_callback.lock() = callback;
    }

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>) {
        *self.traded_specific_currencies.lock() = currencies;
    }

    fn is_websocket_enabled(&self, _role: WebSocketRole) -> bool {
        false
    }

    async fn create_ws_url(&self, _role: WebSocketRole) -> Result<Url> {
        unimplemented!("Not needed for implementation Serum")
    }

    fn get_specific_currency_pair(&self, currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        self.unified_to_specific.read()[&currency_pair]
    }

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode> {
        &self.supported_currencies
    }

    fn should_log_message(&self, message: &str) -> bool {
        message.contains("executionReport")
    }

    fn parse_all_symbols(&self, response: &RestRequestOutcome) -> Result<Vec<Arc<Symbol>>> {
        let markets: Vec<DeserMarketData> = serde_json::from_str(&response.content)
            .context("Unable to deserialize response from Serum markets list")?;

        markets
            .into_iter()
            .filter(|market| !market.deprecated)
            .map(|market| {
                let market_address = Pubkey::from_str(&market.address)
                    .context("Invalid address constant string specified")?;
                let market_program_id = Pubkey::from_str(&market.program_id)
                    .context("Invalid program_id constant string specified")?;

                let symbol = self.get_symbol_from_market(&market.name, market_address)?;

                let specific_currency_pair = market.name.as_str().into();
                let unified_currency_pair =
                    CurrencyPair::from_codes(symbol.base_currency_code, symbol.quote_currency_code);
                self.unified_to_specific
                    .write()
                    .insert(unified_currency_pair, specific_currency_pair);

                // market initiation
                let market_metadata =
                    self.load_market_meta_data(&market_address, &market_program_id)?;
                let market_data =
                    MarketData::new(market_address, market_program_id, market_metadata);
                self.markets_data
                    .write()
                    .insert(symbol.currency_pair(), market_data);

                Ok(Arc::new(symbol))
            })
            .collect()
    }

    fn parse_get_my_trades(
        &self,
        _response: &RestRequestOutcome,
        _last_date_time: Option<DateTime>,
    ) -> Result<Vec<OrderTrade>> {
        todo!()
    }

    fn get_settings(&self) -> &ExchangeSettings {
        todo!()
    }

    fn parse_get_position(&self, _response: &RestRequestOutcome) -> Vec<ActivePosition> {
        todo!()
    }

    fn parse_close_position(&self, _response: &RestRequestOutcome) -> Result<ClosedPosition> {
        todo!()
    }

    fn parse_get_balance(&self, _response: &RestRequestOutcome) -> ExchangeBalancesAndPositions {
        todo!()
    }
}

impl Serum {
    pub fn get_symbol_from_market(
        &self,
        market_name: &String,
        market_pub_key: Pubkey,
    ) -> Result<Symbol> {
        let market = self.get_market(&market_pub_key)?;

        let coin_mint_adr = Pubkey::from_u64_array(market.coin_mint);
        let pc_mint_adr = Pubkey::from_u64_array(market.pc_mint);

        let coin_data = self.rpc_client.get_account_data(&coin_mint_adr)?;
        let pc_data = self.rpc_client.get_account_data(&pc_mint_adr)?;

        let coin_mint_data = state::Mint::unpack_from_slice(&coin_data)?;
        let pc_mint_data = state::Mint::unpack_from_slice(&pc_data)?;

        let (base_currency_id, quote_currency_id) =
            market_name.rsplit_once("/").with_context(|| {
                format!("Unable to get currency pair from market name {market_name}")
            })?;
        let base_currency_code = base_currency_id.into();
        let quote_currency_code = quote_currency_id.into();

        let is_active = true;
        let is_derivative = false;

        let pc_lot_size = Decimal::from(market.pc_lot_size);
        let coin_lot_size = Decimal::from(market.coin_lot_size);
        let factor_pc_decimals = dec!(10).powi(pc_mint_data.decimals as i64);
        let factor_coin_decimals = dec!(10).powi(coin_mint_data.decimals as i64);
        let min_price = (factor_coin_decimals * pc_lot_size) / (factor_pc_decimals * coin_lot_size);
        let min_amount = coin_lot_size / factor_coin_decimals;
        let min_cost = min_price * min_amount;

        let max_price = Decimal::from_u64(u64::MAX);
        let max_amount = Decimal::from_u64(u64::MAX);

        let amount_currency_code = base_currency_code;
        let balance_currency_code = base_currency_code;

        let price_precision = Precision::tick_from_precision(pc_mint_data.decimals as i8);
        let amount_precision = Precision::tick_from_precision(coin_mint_data.decimals as i8);

        Ok(Symbol::new(
            is_active,
            is_derivative,
            base_currency_id.into(),
            base_currency_code,
            quote_currency_id.into(),
            quote_currency_code,
            Some(min_price),
            max_price,
            Some(min_amount),
            max_amount,
            Some(min_cost),
            amount_currency_code,
            Some(balance_currency_code),
            price_precision,
            amount_precision,
        ))
    }
}
