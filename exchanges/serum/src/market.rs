use anyhow::Result;
use mmb_core::exchanges::common::CurrencyPair;
use mmb_core::orders::order::{ExchangeOrderId, OrderStatus};
use mmb_utils::infrastructure::WithExpect;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, MathematicalOps};
use rust_decimal_macros::dec;
use serde::Deserialize;
use serum_dex::matching::Side;
use serum_dex::state::MarketState;
use solana_program::pubkey::Pubkey;

pub struct OrderSerumInfo {
    pub currency_pair: CurrencyPair,
    pub owner: Pubkey,
    pub status: OrderStatus,
    pub exchange_order_id: ExchangeOrderId,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct OpenOrderData {
    pub begin_alignment: [u8; 5],
    pub account_flags: u64,
    pub market: [u64; 4],
    pub owner: [u64; 4],
    pub base_token_free: u64,
    pub base_token_total: u64,
    pub quote_token_rfee: u64,
    pub quote_token_total: u64,
    pub free_slot_bits: u128,
    pub is_bid_bits: u128,
    pub orders: [u128; 128],
    pub client_ids: [u64; 128],
    pub referrer_rebates_accured: u64,
    pub end_alignment: [u8; 7],
}

#[derive(Debug, Copy, Clone)]
pub struct MarketMetaData {
    pub state: MarketState,
    pub price_decimal: u8,
    pub coin_decimal: u8,
    pub owner_address: Pubkey,
    pub coin_mint_address: Pubkey,
    pub price_mint_address: Pubkey,
    pub coin_vault_address: Pubkey,
    pub price_vault_address: Pubkey,
    pub req_queue_address: Pubkey,
    pub event_queue_address: Pubkey,
    pub bids_address: Pubkey,
    pub asks_address: Pubkey,
    pub vault_signer_nonce: Pubkey,
    pub coin_lot: u64,
    pub price_lot: u64,
}

impl MarketMetaData {
    pub(super) fn encode_price(&self, raw_price: u64) -> Result<Decimal> {
        Ok(Decimal::from(raw_price)
            * Decimal::from(self.state.pc_lot_size)
            * dec!(10).powi(self.coin_decimal as i64 - self.price_decimal as i64)
            / Decimal::from(self.coin_lot))
    }

    pub(super) fn encode_size(self, raw_size: u64) -> Result<Decimal> {
        Ok(Decimal::from(raw_size) * Decimal::from(self.coin_lot)
            / dec!(10).powi(self.coin_decimal as i64))
    }

    pub(super) fn make_max_native(&self, price: u64, size: u64) -> u64 {
        self.state.pc_lot_size * size * price
    }

    pub(super) fn make_price(&self, raw_price: Decimal) -> u64 {
        let price = raw_price
            * Decimal::from(self.coin_lot)
            * dec!(10).powi(self.price_decimal as i64 - self.coin_decimal as i64)
            / Decimal::from(self.state.pc_lot_size);

        price
            .to_u64()
            .with_expect(|| format!("Unable to convert make_size as decimal to u64 = {price}"))
    }

    pub(super) fn make_size(&self, raw_size: Decimal) -> u64 {
        let size =
            raw_size * dec!(10).powi(self.coin_decimal as i64) / Decimal::from(self.coin_lot);

        size.to_u64()
            .with_expect(|| format!("Unable to convert make_size as decimal to u64 = {size}"))
    }
}

#[derive(Deserialize, Debug)]
pub struct DeserMarketData {
    pub address: String,
    pub name: String,
    pub deprecated: bool,
    #[serde(rename = "programId")]
    pub program_id: String,
}

#[derive(Debug, Clone, Copy)]
pub struct MarketData {
    pub address: Pubkey,
    pub program_id: Pubkey,
    pub metadata: MarketMetaData,
}

impl MarketData {
    pub fn new(address: Pubkey, program_id: Pubkey, metadata: MarketMetaData) -> Self {
        Self {
            address,
            program_id,
            metadata,
        }
    }
}

#[derive(Debug)]
pub struct Order {
    pub order_id: u128,
    pub price: Decimal,
    pub quantity: Decimal,
    pub slot: u8,
    pub client_order_id: u64,
    pub owner: Pubkey,
    pub side: Side,
}
