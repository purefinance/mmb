use serde::Deserialize;
use serum_dex::state::MarketState;
use solana_program::pubkey::Pubkey;

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
