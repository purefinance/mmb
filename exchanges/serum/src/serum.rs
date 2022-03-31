use anyhow::{anyhow, bail, Context, Result};
use dashmap::DashMap;
use futures::future::join_all;
use itertools::Itertools;
use memoffset::offset_of;
use parking_lot::{Mutex, RwLock};
use rand::rngs::OsRng;
use rust_decimal_macros::dec;
use serum_dex::critbit::{Slab, SlabView};
use serum_dex::instruction::{cancel_order, MarketInstruction};
use serum_dex::matching::Side;
use serum_dex::state::{gen_vault_signer_key, Market, MarketState};
use solana_account_decoder::UiAccount;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType};
use solana_client_helpers::spl_associated_token_account::get_associated_token_address;
use solana_program::account_info::IntoAccountInfo;
use solana_program::instruction::{AccountMeta, Instruction};
use solana_program::program_pack::Pack;
use solana_program::pubkey::Pubkey;
use solana_sdk::account::Account;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use spl_token::state;
use std::collections::HashMap;
use std::mem::size_of;
use std::num::NonZeroU64;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;

use crate::helpers::{FromU64Array, ToOrderSide, ToSerumSide, ToU128};
use crate::market::{DeserMarketData, MarketData, MarketMetaData, OpenOrderData, OrderSerumInfo};
use crate::solana_client::{NetworkType, SolanaClient};
use mmb_core::exchanges::common::{
    Amount, CurrencyCode, CurrencyId, CurrencyPair, ExchangeAccountId, Price, RestRequestOutcome,
    SpecificCurrencyPair,
};
use mmb_core::exchanges::events::{AllowedEventSourceType, ExchangeBalance, ExchangeEvent};
use mmb_core::exchanges::general::exchange::BoxExchangeClient;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    RestFillsType, WebSocketOptions,
};
use mmb_core::exchanges::general::symbol::Symbol;
use mmb_core::exchanges::rest_client::{ErrorHandlerData, ErrorHandlerEmpty, RestClient};
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::exchanges::traits::{
    ExchangeClient, ExchangeClientBuilder, ExchangeClientBuilderResult, HandleOrderFilledCb,
    HandleTradeCb, OrderCancelledCb, OrderCreatedCb,
};
use mmb_core::lifecycle::app_lifetime_manager::AppLifetimeManager;
use mmb_core::orders::order::{
    ClientOrderId, ExchangeOrderId, OrderCancelling, OrderCreating, OrderInfo, OrderSide,
    OrderStatus, OrderType,
};
use mmb_core::orders::pool::OrderRef;
use mmb_core::settings::ExchangeSettings;

pub struct Serum {
    pub id: ExchangeAccountId,
    pub settings: ExchangeSettings,
    pub payer: Keypair,
    pub order_created_callback: Mutex<OrderCreatedCb>,
    pub order_cancelled_callback: Mutex<OrderCancelledCb>,
    pub handle_order_filled_callback: Mutex<HandleOrderFilledCb>,
    pub handle_trade_callback: Mutex<HandleTradeCb>,

    pub unified_to_specific: RwLock<HashMap<CurrencyPair, SpecificCurrencyPair>>,
    pub supported_currencies: DashMap<CurrencyId, CurrencyCode>,
    pub traded_specific_currencies: Mutex<Vec<SpecificCurrencyPair>>,
    pub(super) rest_client: RestClient,
    pub(super) rpc_client: Arc<SolanaClient>,
    pub(super) markets_data: RwLock<HashMap<CurrencyPair, MarketData>>,
    pub(super) open_orders_by_owner: RwLock<HashMap<ClientOrderId, OrderSerumInfo>>,
    pub network_type: NetworkType,
}

impl Serum {
    pub fn new(
        id: ExchangeAccountId,
        settings: ExchangeSettings,
        _events_channel: broadcast::Sender<ExchangeEvent>,
        _lifetime_manager: Arc<AppLifetimeManager>,
        network_type: NetworkType,
        empty_response_is_ok: bool,
    ) -> Self {
        let payer = Keypair::from_base58_string(&settings.secret_key);
        let exchange_account_id = settings.exchange_account_id;

        Self {
            id,
            settings,
            payer,
            order_created_callback: Mutex::new(Box::new(|_, _, _| {})),
            order_cancelled_callback: Mutex::new(Box::new(|_, _, _| {})),
            handle_order_filled_callback: Mutex::new(Box::new(|_| {})),
            handle_trade_callback: Mutex::new(Box::new(|_, _, _, _, _, _| {})),
            unified_to_specific: Default::default(),
            supported_currencies: Default::default(),
            traded_specific_currencies: Default::default(),
            rest_client: RestClient::new(ErrorHandlerData::new(
                empty_response_is_ok,
                exchange_account_id,
                ErrorHandlerEmpty::new(),
            )),
            rpc_client: Arc::new(SolanaClient::new(&network_type)),
            markets_data: Default::default(),
            open_orders_by_owner: Default::default(),
            network_type,
        }
    }

    pub fn get_market(&self, address: &Pubkey) -> Result<MarketState> {
        let mut account = self.rpc_client.get_account(address)?;
        let program_id = account.owner;
        let account_info = (address, &mut account).into_account_info();
        let market = Market::load(&account_info, &program_id, false)?;
        Ok(*market.deref())
    }

    pub fn get_market_data(&self, currency_pair: CurrencyPair) -> Result<MarketData> {
        let lock = self.markets_data.read();
        lock.get(&currency_pair)
            .cloned()
            .ok_or(anyhow!("Unable to get market data"))
    }

    pub fn load_market_meta_data(
        &self,
        address: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<MarketMetaData> {
        let market = self.get_market(address)?;
        let vault_signer_nonce =
            gen_vault_signer_key(market.vault_signer_nonce, address, program_id)?;

        let coin_mint_address = Pubkey::from_u64_array(market.coin_mint);
        let price_mint_address = Pubkey::from_u64_array(market.pc_mint);

        let coin_data = self.rpc_client.get_account_data(&coin_mint_address)?;
        let pc_data = self.rpc_client.get_account_data(&price_mint_address)?;

        let coin_min_data = state::Mint::unpack_from_slice(&coin_data)?;
        let price_mint_data = state::Mint::unpack_from_slice(&pc_data)?;

        Ok(MarketMetaData {
            state: market,
            price_decimal: price_mint_data.decimals,
            coin_decimal: coin_min_data.decimals,
            owner_address: Pubkey::from_u64_array(market.own_address),
            coin_mint_address,
            price_mint_address,
            coin_vault_address: Pubkey::from_u64_array(market.coin_vault),
            price_vault_address: Pubkey::from_u64_array(market.pc_vault),
            req_queue_address: Pubkey::from_u64_array(market.req_q),
            event_queue_address: Pubkey::from_u64_array(market.event_q),
            bids_address: Pubkey::from_u64_array(market.bids),
            asks_address: Pubkey::from_u64_array(market.asks),
            vault_signer_nonce,
            coin_lot: market.coin_lot_size,
            price_lot: market.pc_lot_size,
        })
    }

    fn load_orders_for_owner(
        &self,
        address: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<Vec<(Pubkey, Account)>> {
        let filter1 = RpcFilterType::Memcmp(Memcmp {
            offset: offset_of!(OpenOrderData, market),
            bytes: MemcmpEncodedBytes::Base58(address.to_string()),
            encoding: None,
        });

        let filter2 = RpcFilterType::Memcmp(Memcmp {
            offset: offset_of!(OpenOrderData, owner),
            bytes: MemcmpEncodedBytes::Base58(self.payer.pubkey().to_string()),
            encoding: None,
        });

        let filter3 = RpcFilterType::DataSize(size_of::<OpenOrderData>() as u64);

        let filters = Some(vec![filter1, filter2, filter3]);

        let account_config = RpcAccountInfoConfig {
            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
            ..RpcAccountInfoConfig::default()
        };

        let config = RpcProgramAccountsConfig {
            filters,
            account_config,
            with_context: Some(false),
        };

        self.rpc_client
            .get_program_accounts_with_config(program_id, config)
    }

    fn create_dex_account(
        &self,
        program_id: &Pubkey,
        payer: &Pubkey,
        length: usize,
    ) -> Result<(Keypair, Instruction)> {
        let key = Keypair::generate(&mut OsRng);
        let lamports = self
            .rpc_client
            .get_minimum_balance_for_rent_exemption(length)?;

        let create_account_instr = solana_sdk::system_instruction::create_account(
            payer,
            &key.pubkey(),
            lamports,
            length as u64,
            program_id,
        );
        Ok((key, create_account_instr))
    }

    fn create_new_order_instruction(
        &self,
        program_id: Pubkey,
        metadata: &MarketMetaData,
        open_order_account: Pubkey,
        side: OrderSide,
        price: Price,
        amount: Amount,
        order_type: OrderType,
        client_order_id: &ClientOrderId,
    ) -> Result<Instruction> {
        let price = metadata.make_price(price);
        let amount = metadata.make_size(amount);
        let max_native_price = metadata.make_max_native(price, amount);
        let client_order_id = u64::from_str(client_order_id.as_str()).with_context(|| {
            format!("Failed to convert client_order_id {client_order_id} to u64")
        })?;

        let new_order = serum_dex::instruction::NewOrderInstructionV3 {
            side: side.to_serum_side(),
            limit_price: NonZeroU64::new(price)
                .with_context(|| format!("Failed to create limit_price {price:?}"))?,
            max_coin_qty: NonZeroU64::new(amount)
                .with_context(|| format!("Failed to create max_coin_qty {amount:?}"))?,
            max_native_pc_qty_including_fees: NonZeroU64::new(max_native_price).with_context(
                || {
                    format!(
                        "Failed to create max_native_pc_qty_including_fees {max_native_price:?}"
                    )
                },
            )?,
            self_trade_behavior: serum_dex::instruction::SelfTradeBehavior::DecrementTake,
            order_type: match order_type {
                OrderType::Limit => serum_dex::matching::OrderType::Limit,
                _ => unimplemented!(),
            },
            client_order_id,
            limit: u16::MAX,
        };

        let wallet = match side {
            OrderSide::Buy => {
                get_associated_token_address(&self.payer.pubkey(), &metadata.price_mint_address)
            }
            OrderSide::Sell => {
                get_associated_token_address(&self.payer.pubkey(), &metadata.coin_mint_address)
            }
        };

        Ok(Instruction {
            program_id,
            data: MarketInstruction::NewOrderV3(new_order).pack(),
            accounts: vec![
                AccountMeta::new(metadata.owner_address, false),
                AccountMeta::new(open_order_account, false),
                AccountMeta::new(metadata.req_queue_address, false),
                AccountMeta::new(metadata.event_queue_address, false),
                AccountMeta::new(metadata.bids_address, false),
                AccountMeta::new(metadata.asks_address, false),
                AccountMeta::new(wallet, false),
                AccountMeta::new_readonly(self.payer.pubkey(), true),
                AccountMeta::new(metadata.coin_vault_address, false),
                AccountMeta::new(metadata.price_vault_address, false),
                AccountMeta::new_readonly(spl_token::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false),
            ],
        })
    }

    pub fn create_settle_funds_instructions(
        &self,
        open_order_accounts: &[Pubkey],
        market: &MarketMetaData,
        market_id: &Pubkey,
        program_id: &Pubkey,
    ) -> Vec<Instruction> {
        open_order_accounts
            .iter()
            .map(|key| {
                let data = MarketInstruction::SettleFunds.pack();
                Instruction {
                    program_id: *program_id,
                    data,
                    accounts: vec![
                        AccountMeta::new(*market_id, false),
                        AccountMeta::new(*key, false),
                        AccountMeta::new_readonly(self.payer.pubkey(), true),
                        AccountMeta::new(market.coin_vault_address, false),
                        AccountMeta::new(market.price_vault_address, false),
                        AccountMeta::new(
                            get_associated_token_address(
                                &self.payer.pubkey(),
                                &market.coin_mint_address,
                            ),
                            false,
                        ),
                        AccountMeta::new(
                            get_associated_token_address(
                                &self.payer.pubkey(),
                                &market.price_mint_address,
                            ),
                            false,
                        ),
                        AccountMeta::new_readonly(market.vault_signer_nonce, false),
                        AccountMeta::new_readonly(spl_token::ID, false),
                    ],
                }
            })
            .collect()
    }

    pub async fn send_instructions(&self, instructions: &[Instruction]) -> Result<()> {
        let recent_hash = self.rpc_client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer],
            recent_hash,
        );

        self.rpc_client.send_transaction(transaction).await?;
        Ok(())
    }

    pub async fn get_exchange_balance_from_account(
        &self,
        currency_code: &CurrencyCode,
        mint_address: &Pubkey,
    ) -> Result<ExchangeBalance> {
        let wallet_address = get_associated_token_address(&self.payer.pubkey(), mint_address);
        let token_amount = self.rpc_client.get_token_account_balance(&wallet_address)?;
        let ui_amount = token_amount.ui_amount.with_context(|| {
            format!("Unable get token amount for payer {}", self.payer.pubkey())
        })?;
        let balance = ui_amount.try_into().with_context(|| {
            format!("Unable get balance decimal value from ui_amount {ui_amount}")
        })?;

        Ok(ExchangeBalance {
            currency_code: *currency_code,
            balance,
        })
    }

    pub async fn do_get_order_info(&self, order: &OrderRef) -> Result<OrderInfo> {
        let client_order_id = order.client_order_id();

        self.get_open_orders_by_currency_pair(order.currency_pair())
            .await?
            .iter()
            .find(|order_info| order_info.client_order_id == client_order_id)
            .cloned()
            .ok_or(anyhow!("Order not found for id {client_order_id}"))
    }

    pub fn get_orders_from_ui_account(
        &self,
        ui_account: UiAccount,
        market_info: &MarketMetaData,
        side: Side,
        currency_pair: CurrencyPair,
    ) -> Result<Vec<OrderInfo>> {
        let mut account: Account = ui_account.decode().with_context(|| {
            format!("Failed to decode account for currency pair {currency_pair}")
        })?;
        let market_state = market_info.state;
        let account_address = match side {
            Side::Ask => &market_info.asks_address,
            Side::Bid => &market_info.bids_address,
        };

        let account_info = (account_address, &mut account).into_account_info();
        let slab = match side {
            Side::Ask => market_state.load_asks_mut(&account_info)?,
            Side::Bid => market_state.load_bids_mut(&account_info)?,
        };

        self.encode_orders(&slab, market_info, side, &currency_pair)
    }

    pub fn encode_orders(
        &self,
        slab: &Slab,
        market_info: &MarketMetaData,
        side: Side,
        currency_pair: &CurrencyPair,
    ) -> Result<Vec<OrderInfo>> {
        let mut orders = Vec::new();
        for i in 0..slab.capacity() {
            let any_node = slab.get(i as u32);
            if let Some(node) = any_node {
                if let Some(leaf) = node.as_leaf() {
                    let client_order_id = leaf.client_order_id().to_string().as_str().into();
                    let exchange_order_id = leaf.order_id().to_string().as_str().into();
                    let price = market_info.encode_price(leaf.price().get())?;
                    let quantity = market_info.encode_size(leaf.quantity())?;
                    orders.push(OrderInfo {
                        currency_pair: *currency_pair,
                        exchange_order_id,
                        client_order_id,
                        order_side: side.to_order_side(),
                        order_status: OrderStatus::Created,
                        price,
                        amount: quantity,
                        average_fill_price: price,
                        filled_amount: dec!(0),
                        commission_currency_code: None,
                        commission_rate: None,
                        commission_amount: None,
                    })
                }
            }
        }

        Ok(orders)
    }

    pub async fn subscribe_to_all_market(&self) {
        let markets_data: HashMap<CurrencyPair, MarketData> = self.markets_data.read().clone();

        join_all(markets_data.iter().map(|(currency_pair, market_data)| {
            self.rpc_client
                .subscribe_to_market(currency_pair, market_data)
        }))
        .await;
    }

    async fn get_order_id(
        &self,
        client_order_id: &ClientOrderId,
        currency_pair: CurrencyPair,
    ) -> Result<ExchangeOrderId> {
        for attempt in 1..=10 {
            let orders = self.get_open_orders_by_currency_pair(currency_pair).await?;
            if let Some(order) = orders
                .iter()
                .find(|order| order.client_order_id == *client_order_id)
            {
                return Ok(order.exchange_order_id.clone());
            }

            log::warn!("Failed to get ExchangeOrderId. Order with client order id {client_order_id} not found. Attempt {attempt}");
            sleep(Duration::from_secs(2)).await;
        }

        bail!("Failed to get ExchangeOrderId by client order id {client_order_id}")
    }

    pub(super) async fn create_order_core(&self, order: OrderCreating) -> Result<ExchangeOrderId> {
        let mut instructions = Vec::new();
        let mut signers = Vec::new();
        let orders_keypair: Keypair;
        let client_order_id = order.header.client_order_id.clone();
        let currency_pair = order.header.currency_pair;

        let market_data = self.get_market_data(currency_pair)?;
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
            &client_order_id,
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

        self.open_orders_by_owner.write().insert(
            client_order_id.clone(),
            OrderSerumInfo {
                currency_pair,
                owner: open_order_account,
                status: OrderStatus::Creating,
                exchange_order_id: "0".into(),
            },
        );

        self.rpc_client.send_transaction(transaction).await?;

        let exchange_order_id = self.get_order_id(&client_order_id, currency_pair).await?;
        Ok(exchange_order_id)
    }

    pub(super) async fn cancel_order_core(&self, order: &OrderCancelling) -> Result<()> {
        let market_data = self.get_market_data(order.header.currency_pair)?;
        let metadata = market_data.metadata;
        let client_order_id = order.header.client_order_id.clone();
        let exchange_order_id = order.exchange_order_id.clone();

        let owner = {
            let mut guard_lock = self.open_orders_by_owner.write();
            let order_info = guard_lock.get_mut(&client_order_id).with_context(|| {
                format!("Unable to get owner by client order id = {client_order_id}")
            })?;

            order_info.status = OrderStatus::Canceling;
            order_info.owner
        };

        let instructions = &[cancel_order(
            &market_data.program_id,
            &metadata.owner_address,
            &metadata.bids_address,
            &metadata.asks_address,
            &owner,
            &self.payer.pubkey(),
            &metadata.event_queue_address,
            order.header.side.to_serum_side(),
            exchange_order_id.to_u128(),
        )?];

        let recent_hash = self.rpc_client.get_latest_blockhash()?;
        let transaction = Transaction::new_signed_with_payer(
            instructions,
            Some(&self.payer.pubkey()),
            &[&self.payer],
            recent_hash,
        );

        self.rpc_client.send_transaction(transaction).await?;

        Ok(())
    }

    pub(super) async fn cancel_all_orders_core(&self, currency_pair: CurrencyPair) -> Result<()> {
        let market_data = self.get_market_data(currency_pair)?;
        let metadata = market_data.metadata;

        let orders = self.get_open_orders_by_currency_pair(currency_pair).await?;

        let instructions: Vec<Instruction> = orders
            .iter()
            .map(|order| {
                let owner = self
                    .open_orders_by_owner
                    .write()
                    .remove(&order.client_order_id)
                    .with_context(|| {
                        format!(
                            "Unable to get owner by client order id = {}",
                            order.client_order_id
                        )
                    })?
                    .owner;

                cancel_order(
                    &market_data.program_id,
                    &metadata.owner_address,
                    &metadata.bids_address,
                    &metadata.asks_address,
                    &owner,
                    &self.payer.pubkey(),
                    &metadata.event_queue_address,
                    order.order_side.to_serum_side(),
                    order.exchange_order_id.to_u128(),
                )
                .map_err(|error| anyhow!(error))
            })
            .try_collect()?;

        join_all(
            instructions
                .chunks(12)
                .map(|ixs| self.send_instructions(ixs)),
        )
        .await
        .into_iter()
        .try_collect()?;

        Ok(())
    }

    pub(super) fn parse_all_symbols(
        &self,
        response: &RestRequestOutcome,
    ) -> Result<Vec<Arc<Symbol>>> {
        let markets: Vec<DeserMarketData> = serde_json::from_str(&response.content)
            .expect("Unable to deserialize response from Serum markets list");

        markets
            .into_iter()
            .filter(|market| !market.deprecated)
            .map(|market| {
                let market_address = market
                    .address
                    .parse()
                    .expect("Invalid address constant string specified");
                let market_program_id = market
                    .program_id
                    .parse()
                    .expect("Invalid program_id constant string specified");

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
}

pub struct SerumBuilder;

impl ExchangeClientBuilder for SerumBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: tokio::sync::broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
    ) -> ExchangeClientBuilderResult {
        let exchange_account_id = exchange_settings.exchange_account_id;
        let empty_response_is_ok = false;

        ExchangeClientBuilderResult {
            client: Box::new(Serum::new(
                exchange_account_id,
                exchange_settings,
                events_channel,
                lifetime_manager,
                NetworkType::Mainnet,
                empty_response_is_ok,
            )) as BoxExchangeClient,
            features: ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::new(RestFillsType::None),
                OrderFeatures::default(),
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                empty_response_is_ok,
                false,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
            ),
        }
    }

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        RequestTimeoutArguments::from_requests_per_minute(240)
    }
}
