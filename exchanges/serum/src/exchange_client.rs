use crate::serum::Serum;
use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use itertools::Itertools;
use serum_dex::matching::Side;
use serum_dex::state::MarketState;
use solana_client::client_error::reqwest::StatusCode;
use solana_program::account_info::IntoAccountInfo;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use std::collections::HashMap;
use std::mem::size_of;
use std::ops::DerefMut;

use crate::market::OpenOrderData;
use mmb_core::exchanges::common::{
    ActivePosition, CurrencyCode, CurrencyPair, ExchangeError, ExchangeErrorType, Price,
    RestRequestOutcome,
};
use mmb_core::exchanges::events::{ExchangeBalance, ExchangeBalancesAndPositions};
use mmb_core::exchanges::general::symbol::Symbol;
use mmb_core::exchanges::traits::ExchangeClient;
use mmb_core::orders::order::{OrderCancelling, OrderCreating, OrderInfo};
use mmb_core::orders::pool::OrderRef;

#[async_trait]
impl<'a> ExchangeClient for Serum {
    async fn request_all_symbols(&self) -> Result<RestRequestOutcome> {
        self.rest_client
            .get(
                self.network_type
                    .market_list_url()
                    .try_into()
                    .expect("Unable create url"),
                "",
            )
            .await
    }

    async fn create_order(&self, order: &OrderCreating) -> Result<RestRequestOutcome> {
        let mut instructions = Vec::new();
        let mut signers = Vec::new();
        let orders_keypair: Keypair;

        let market_data = self.get_market_data(&order.header.currency_pair)?;
        let accounts = self.load_orders_for_owner(&market_data.address, &market_data.program_id)?;

        let open_order_account = match accounts.first() {
            Some((acc, _)) => *acc,
            None => {
                let (orders_key, instruction) = self.create_dex_account(
                    &market_data.program_id,
                    &self.payer.pubkey(),
                    size_of::<OpenOrderData>(),
                )?;
                // life time saving
                orders_keypair = orders_key;

                signers.push(&orders_keypair);
                instructions.push(instruction);
                orders_keypair.pubkey()
            }
        };

        let place_order_ix = self.create_new_order_instruction(
            market_data.program_id,
            &market_data.metadata,
            open_order_account,
            order.header.side,
            order.price,
            order.header.amount,
            order.header.order_type,
        )?;
        instructions.push(place_order_ix);

        let settle_funds_instructions = self.create_settle_funds_instructions(
            &[open_order_account],
            &market_data.metadata,
            &market_data.address,
            &market_data.program_id,
        );
        instructions.extend(settle_funds_instructions);

        signers.push(&self.payer);

        let recent_hash = self.rpc_client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &signers,
            recent_hash,
        );

        let transaction_signature = self.send_transaction(transaction).await?;

        Ok(RestRequestOutcome::new(
            transaction_signature.to_string(),
            StatusCode::OK,
        ))
    }

    async fn request_cancel_order(&self, _order: &OrderCancelling) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair) -> Result<()> {
        todo!()
    }

    async fn get_open_orders(&self) -> Result<Vec<OrderInfo>> {
        let currency_pairs = self.markets_data.read().keys().cloned().collect_vec();

        join_all(
            currency_pairs
                .into_iter()
                .map(|currency_pair| self.get_open_orders_by_currency_pair(currency_pair)),
        )
        .await
        .into_iter()
        .flatten_ok()
        .collect()
    }

    async fn get_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<Vec<OrderInfo>> {
        let market_data = self.get_market_data(&currency_pair)?;
        let program_id = &market_data.program_id;
        let market_metadata = &market_data.metadata;
        let mut account = self
            .rpc_client
            .get_account(&market_metadata.owner_address)?;
        let account_info = (program_id, &mut account).into_account_info();

        let market_data = MarketState::load(&account_info, program_id, false)?;

        let mut asks_account = self.rpc_client.get_account(&market_metadata.asks_address)?;
        let mut bids_account = self.rpc_client.get_account(&market_metadata.bids_address)?;
        let asks_info = (&market_metadata.asks_address, &mut asks_account).into_account_info();
        let bids_info = (&market_metadata.bids_address, &mut bids_account).into_account_info();
        let mut bids = market_data.load_bids_mut(&bids_info)?;
        let mut asks = market_data.load_asks_mut(&asks_info)?;

        let bids_slab = bids.deref_mut();
        let asks_slab = asks.deref_mut();

        let mut orders =
            self.encode_orders(asks_slab, &market_metadata, Side::Ask, &currency_pair)?;
        orders.append(&mut self.encode_orders(
            bids_slab,
            &market_metadata,
            Side::Bid,
            &currency_pair,
        )?);

        Ok(orders)
    }

    async fn get_order_info(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        self.do_get_order_info(order).await.map_err(|error| {
            ExchangeError::new(ExchangeErrorType::Unknown, error.to_string(), None)
        })
    }

    async fn request_my_trades(
        &self,
        _symbol: &Symbol,
        _last_date_time: Option<mmb_utils::DateTime>,
    ) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_get_position(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_get_balance_and_position(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn get_balance(&self) -> Result<ExchangeBalancesAndPositions> {
        // price_mint_address and coin_mint_address are the same for different currency pairs and corresponding CurrencyCode
        let mint_addresses: HashMap<CurrencyCode, Pubkey> = self
            .markets_data
            .read()
            .iter()
            .flat_map(|(pair, market)| {
                let pair_codes = pair.to_codes();
                let market_metadata = market.metadata;

                [
                    (pair_codes.base, market_metadata.price_mint_address),
                    (pair_codes.quote, market_metadata.coin_mint_address),
                ]
            })
            .collect();

        let balances = join_all(mint_addresses.iter().map(|(currency_code, mint_address)| {
            self.get_exchange_balance_from_account(&currency_code, &mint_address)
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<ExchangeBalance>>>()?;

        Ok(ExchangeBalancesAndPositions {
            balances,
            positions: None,
        })
    }

    async fn request_close_position(
        &self,
        _position: &ActivePosition,
        _price: Option<Price>,
    ) -> Result<RestRequestOutcome> {
        todo!()
    }
}
