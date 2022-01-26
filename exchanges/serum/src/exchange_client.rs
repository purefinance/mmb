use crate::serum::Serum;
use anyhow::Result;
use async_trait::async_trait;
use solana_client::client_error::reqwest::StatusCode;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use std::mem::size_of;

use crate::market::OpenOrderData;
use mmb_core::exchanges::common::{ActivePosition, CurrencyPair, Price, RestRequestOutcome};
use mmb_core::exchanges::general::symbol::Symbol;
use mmb_core::exchanges::traits::ExchangeClient;
use mmb_core::orders::order::{OrderCancelling, OrderCreating};
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

    async fn request_open_orders(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_open_orders_by_currency_pair(
        &self,
        _currency_pair: CurrencyPair,
    ) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_order_info(&self, _order: &OrderRef) -> Result<RestRequestOutcome> {
        todo!()
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

    async fn request_get_balance(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_close_position(
        &self,
        _position: &ActivePosition,
        _price: Option<Price>,
    ) -> Result<RestRequestOutcome> {
        todo!()
    }
}
