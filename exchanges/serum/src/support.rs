use crate::helpers::FromU64Array;
use crate::serum::Serum;

use anyhow::{Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use serum_dex::matching::Side;
use solana_account_decoder::UiAccount;
use url::Url;

use mmb_core::connectivity::connectivity_manager::WebSocketRole;
use mmb_core::exchanges::common::CurrencyPair;
use mmb_core::exchanges::common::{CurrencyCode, CurrencyId, SpecificCurrencyPair};
use mmb_core::exchanges::general::symbol::{Precision, Symbol};
use mmb_core::exchanges::traits::{
    HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb,
    Support,
};
use mmb_core::orders::fill::EventSourceType;
use mmb_core::orders::order::OrderStatus;
use mmb_core::settings::ExchangeSettings;

use crate::solana_client::SolanaMessage;
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use spl_token::state;

#[async_trait]
impl Support for Serum {
    fn on_websocket_message(&self, msg: &str) -> Result<()> {
        match self.rpc_client.handle_on_message(msg) {
            SolanaMessage::AccountUpdated(currency_pair, side, ui_account) => {
                self.handle_account_market_changed(currency_pair, side, ui_account)
            }
            _ => Ok(()),
        }
    }

    fn on_connecting(&self) -> Result<()> {
        // Not needed for implementation Serum
        Ok(())
    }

    fn set_send_websocket_message_callback(&self, callback: SendWebsocketMessageCb) {
        self.rpc_client
            .set_send_websocket_message_callback(callback);
    }

    fn set_order_created_callback(&self, callback: OrderCreatedCb) {
        *self.order_created_callback.lock() = callback;
    }

    fn set_order_cancelled_callback(&self, callback: OrderCancelledCb) {
        *self.order_cancelled_callback.lock() = callback;
    }

    fn set_handle_order_filled_callback(&self, callback: HandleOrderFilledCb) {
        *self.handle_order_filled_callback.lock() = callback;
    }

    fn set_handle_trade_callback(&self, callback: HandleTradeCb) {
        *self.handle_trade_callback.lock() = callback;
    }

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>) {
        *self.traded_specific_currencies.lock() = currencies;
    }

    fn is_websocket_enabled(&self, role: WebSocketRole) -> bool {
        match role {
            WebSocketRole::Main => true,
            WebSocketRole::Secondary => false,
        }
    }

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Url> {
        let url = match role {
            WebSocketRole::Main => self.network_type.ws(),
            WebSocketRole::Secondary => unimplemented!("Not needed for implementation Serum"),
        };

        Url::parse(url).with_context(|| format!("Unable parse websocket {role:?} uri from {url}"))
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

    fn get_settings(&self) -> &ExchangeSettings {
        todo!()
    }
}

impl Serum {
    fn handle_account_market_changed(
        &self,
        currency_pair: CurrencyPair,
        side: Side,
        ui_account: UiAccount,
    ) -> Result<()> {
        let market = self.get_market_data(currency_pair)?;
        let market_info = &market.metadata;
        let orders =
            self.get_orders_from_ui_account(ui_account, market_info, side, currency_pair)?;

        let mut guard_lock = self.open_orders_by_owner.write();
        guard_lock
            .iter_mut()
            .filter(|(_, order_info)| order_info.currency_pair == currency_pair)
            .for_each(|(client_order_id, order_info)| match order_info.status {
                OrderStatus::Creating => {
                    if let Some(order) = orders
                        .iter()
                        .find(|order| order.client_order_id == *client_order_id)
                    {
                        order_info.status = OrderStatus::Created;
                        order_info.exchange_order_id = order.exchange_order_id.clone();
                        (&self.order_created_callback).lock()(
                            order.client_order_id.clone(),
                            order.exchange_order_id.clone(),
                            EventSourceType::WebSocket,
                        );
                    }
                }
                OrderStatus::Canceling => {
                    if !orders
                        .iter()
                        .any(|order| order.client_order_id == *client_order_id)
                    {
                        order_info.status = OrderStatus::Canceled;
                        (&self.order_cancelled_callback).lock()(
                            client_order_id.clone(),
                            order_info.exchange_order_id.clone(),
                            EventSourceType::WebSocket,
                        );
                    }
                }
                _ => {}
            });
        guard_lock.retain(|_, order_info| !order_info.status.is_finished());

        Ok(())
    }

    pub fn get_symbol_from_market(
        &self,
        market_name: &str,
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
            market_name.rsplit_once('/').with_context(|| {
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
