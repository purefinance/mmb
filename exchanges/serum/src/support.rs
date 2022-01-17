use crate::helpers::{convert64_to_pubkey, decimal_from_u64, split_once};
use crate::serum::Serum;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;

use mmb_core::connectivity::connectivity_manager::WebSocketRole;
use mmb_core::exchanges::common::CurrencyPair;
use mmb_core::exchanges::common::{
    ActivePosition, Amount, ClosedPosition, CurrencyCode, CurrencyId, ExchangeError, Price,
    RestRequestOutcome, SpecificCurrencyPair,
};
use mmb_core::exchanges::events::{ExchangeBalancesAndPositions, TradeId};
use mmb_core::exchanges::general::handlers::handle_order_filled::FillEventData;
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::general::symbol::{Precision, Symbol};
use mmb_core::exchanges::traits::Support;
use mmb_core::orders::fill::EventSourceType;
use mmb_core::orders::order::{ClientOrderId, ExchangeOrderId, OrderInfo, OrderSide};
use mmb_core::settings::ExchangeSettings;
use mmb_utils::DateTime;

use serum_dex::state::Market;
use solana_program::account_info::IntoAccountInfo;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use spl_token::state;

#[async_trait]
impl Support for Serum {
    fn is_rest_error_code(&self, _response: &RestRequestOutcome) -> Result<(), ExchangeError> {
        // TODO not implemented
        Ok(())
    }

    fn get_order_id(&self, _response: &RestRequestOutcome) -> Result<ExchangeOrderId> {
        todo!()
    }

    fn clarify_error_type(&self, _error: &mut ExchangeError) {
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

    fn parse_open_orders(&self, _response: &RestRequestOutcome) -> Result<Vec<OrderInfo>> {
        todo!()
    }

    fn parse_order_info(&self, _response: &RestRequestOutcome) -> Result<OrderInfo> {
        todo!()
    }

    fn parse_all_symbols(&self, response: &RestRequestOutcome) -> Result<Vec<Arc<Symbol>>> {
        let deserialized: Value = serde_json::from_str(&response.content)
            .context("Unable to deserialize response from Serum markets list")?;
        let markets = deserialized
            .as_array()
            .ok_or(anyhow!("Unable to get markets array from Serum"))?;

        let mut result = Vec::new();
        for market in markets {
            let is_deprecated = market["deprecated"]
                .as_bool()
                .context("Unable to get deprecated state market from Serum")?;
            if is_deprecated {
                continue;
            }

            let market_name = &market
                .get_as_str("name")
                .context("Unable to get name market from Serum")?;

            let market_pub_key_raw = &market
                .get_as_str("address")
                .context("Unable to get market address")?;
            let market_pub_key = Pubkey::from_str(market_pub_key_raw)
                .context("Invalid pubkey constant string specified")?;

            let symbol = self.get_symbol_from_market(market_name, market_pub_key)?;

            let specific_currency_pair = market_name.as_str().into();
            let unified_currency_pair =
                CurrencyPair::from_codes(symbol.base_currency_code, symbol.quote_currency_code);
            self.unified_to_specific
                .write()
                .insert(unified_currency_pair, specific_currency_pair);

            result.push(Arc::new(symbol));
        }

        Ok(result)
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
        let mut account = self.rpc_client.get_account(&market_pub_key)?;
        let program_id = account.owner.clone();
        let account_info = (&market_pub_key, &mut account).into_account_info();
        let market = Market::load(&account_info, &program_id)?;

        let coin_mint_adr = convert64_to_pubkey(market.coin_mint);
        let pc_mint_adr = convert64_to_pubkey(market.pc_mint);

        let coin_data = self.rpc_client.get_account_data(&coin_mint_adr)?;
        let pc_data = self.rpc_client.get_account_data(&pc_mint_adr)?;

        let coin_mint_data = state::Mint::unpack_from_slice(&coin_data)?;
        let pc_mint_data = state::Mint::unpack_from_slice(&pc_data)?;

        let (base_currency_id, quote_currency_id) = split_once(&market_name, "/");
        let base_currency_code = base_currency_id.into();
        let quote_currency_code = quote_currency_id.into();

        let is_active = true;
        let is_derivative = false;

        let min_price = (decimal_from_u64(10u64.pow(coin_mint_data.decimals as u32))?
            * decimal_from_u64(market.pc_lot_size)?)
            / (decimal_from_u64(10u64.pow(pc_mint_data.decimals as u32))?
                * decimal_from_u64(market.coin_lot_size)?);
        let max_price = Decimal::from_u64(u64::MAX);

        let min_amount = decimal_from_u64(market.coin_lot_size)?
            / decimal_from_u64(10u64.pow(coin_mint_data.decimals as u32))?;
        let max_amount = Decimal::from_u64(u64::MAX);

        let min_cost = min_price * min_amount;

        let amount_currency_code = base_currency_code;
        let balance_currency_code = base_currency_code;

        let price_precision = Precision::ByTick {
            tick: convert_decimals_to_tick(pc_mint_data.decimals).with_context(|| {
                format!(
                    "Unable to convert price precision from decimals = {}",
                    pc_mint_data.decimals
                )
            })?,
        };
        let amount_precision = Precision::ByTick {
            tick: convert_decimals_to_tick(coin_mint_data.decimals).with_context(|| {
                format!(
                    "Unable to convert amount precision from decimals = {}",
                    coin_mint_data.decimals
                )
            })?,
        };

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

fn convert_decimals_to_tick(decimals: u8) -> Result<Decimal> {
    let tick = 1.0f64 / 10f64.powf(decimals as f64);
    Decimal::from_f64_retain(tick)
        .with_context(|| format!("Error parsing decimal from {}f64", tick))
}

// TODO: Duplicate code. Take out to a separate place (q.v. Binance crate)
trait GetOrErr {
    fn get_as_str(&self, key: &str) -> Result<String>;
}

// TODO: Duplicate code. Take out to a separate place (q.v. Binance crate)
impl GetOrErr for Value {
    fn get_as_str(&self, key: &str) -> Result<String> {
        Ok(self
            .get(key)
            .with_context(|| format!("Unable to get {} from JSON value", key))?
            .as_str()
            .with_context(|| format!("Unable to get {} as string from JSON value", key))?
            .to_string())
    }
}
