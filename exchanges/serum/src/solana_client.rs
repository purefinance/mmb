use crate::market::MarketData;
use anyhow::Result;
use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};

use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use serum_dex::matching::Side;

use solana_account_decoder::parse_token::UiTokenAmount;
use solana_account_decoder::{UiAccount, UiAccountEncoding};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_response::Response;
use solana_program::hash::Hash;
use solana_program::pubkey::Pubkey;
use solana_sdk::account::Account;
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_sdk::signature::Signature;
use solana_sdk::transaction::Transaction;
use tokio::join;

use mmb_core::connectivity::connectivity_manager::WebSocketRole;
use mmb_core::exchanges::common::CurrencyPair;
use mmb_core::exchanges::traits::SendWebsocketMessageCb;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::{impl_u64_id, time::get_atomic_current_secs};

pub const ALLOW_FLAG: bool = false;

pub struct SolanaHosts {
    url: String,
    ws: String,
    market_url: String,
}

pub enum NetworkType {
    Mainnet,
    Devnet,
    Testnet,
    Custom(SolanaHosts),
}

impl NetworkType {
    pub fn url(&self) -> &str {
        match self {
            NetworkType::Devnet => "https://api.devnet.solana.com",
            NetworkType::Mainnet => "https://api.mainnet-beta.solana.com",
            NetworkType::Testnet => "https://api.testnet.solana.com",
            NetworkType::Custom(network_opts) => &network_opts.url,
        }
    }

    pub fn ws(&self) -> &str {
        match self {
            NetworkType::Devnet => "ws://api.devnet.solana.com/",
            NetworkType::Mainnet => "ws://api.mainnet-beta.solana.com/",
            NetworkType::Testnet => "ws://api.testnet.solana.com",
            NetworkType::Custom(network_opts) => &network_opts.ws,
        }
    }

    pub fn market_list_url(&self) -> &str {
        match self {
            NetworkType::Devnet => "https://raw.githubusercontent.com/kizeevov/serum_devnet/main/markets.json",
            NetworkType::Custom(network_opts) => &network_opts.market_url,
            _ => "https://raw.githubusercontent.com/project-serum/serum-ts/master/packages/serum/src/markets.json",
        }
    }
}

#[derive(Deserialize, Debug)]
struct SubscribeResult {
    id: RequestId,
    result: RequestId,
}

#[derive(Deserialize, Debug)]
struct AccountNotification {
    params: NotificationParams,
}

#[derive(Deserialize, Debug)]
struct NotificationParams {
    result: Response<UiAccount>,
    subscription: RequestId,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum WebsocketMessage {
    SubscribeResult(SubscribeResult),
    AccountNotification(AccountNotification),
}

#[derive(Debug, Clone)]
struct SubscriptionMarketData {
    currency_pair: CurrencyPair,
    side: Side,
}

impl_u64_id!(RequestId);

pub enum SolanaMessage {
    Unknown,
    Service,
    AccountUpdated(CurrencyPair, Side, UiAccount),
}

/// Wrapper for the solana rpc client with support for asynchronous methods
/// and subscription to order change events
pub struct SolanaClient {
    rpc_client: Arc<RpcClient>,

    send_websocket_message_callback: Mutex<SendWebsocketMessageCb>,
    subscription_requests: RwLock<HashMap<RequestId, SubscriptionMarketData>>,
    subscriptions: RwLock<HashMap<RequestId, SubscriptionMarketData>>,
}

impl SolanaClient {
    pub fn new(network_type: &NetworkType) -> Self {
        let rpc_client = RpcClient::new(network_type.url().to_string());

        Self {
            rpc_client: Arc::new(rpc_client),
            send_websocket_message_callback: Mutex::new(Box::new(|_, _| Box::pin(async {}))),
            subscription_requests: Default::default(),
            subscriptions: Default::default(),
        }
    }

    pub fn set_send_websocket_message_callback(&self, callback: SendWebsocketMessageCb) {
        *self.send_websocket_message_callback.lock() = callback;
    }

    pub fn get_account(&self, pubkey: &Pubkey) -> Result<Account> {
        Ok(self.rpc_client.get_account(pubkey)?)
    }

    pub fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        Ok(self.rpc_client.get_account_data(pubkey)?)
    }

    pub fn get_program_accounts_with_config(
        &self,
        pubkey: &Pubkey,
        config: RpcProgramAccountsConfig,
    ) -> Result<Vec<(Pubkey, Account)>> {
        Ok(self
            .rpc_client
            .get_program_accounts_with_config(pubkey, config)?)
    }

    pub fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> Result<u64> {
        Ok(self
            .rpc_client
            .get_minimum_balance_for_rent_exemption(data_len)?)
    }

    pub fn get_latest_blockhash(&self) -> Result<Hash> {
        Ok(self.rpc_client.get_latest_blockhash()?)
    }

    pub fn get_token_account_balance(&self, pubkey: &Pubkey) -> Result<UiTokenAmount> {
        Ok(self.rpc_client.get_token_account_balance(pubkey)?)
    }

    pub async fn send_transaction(&self, transaction: Transaction) -> Result<Signature> {
        Ok(tokio::task::spawn_blocking({
            let rpc_client = self.rpc_client.clone();
            move || {
                rpc_client.send_and_confirm_transaction_with_spinner_and_config(
                    &transaction,
                    CommitmentConfig {
                        commitment: CommitmentLevel::Confirmed,
                    },
                    rpc_config::RpcSendTransactionConfig {
                        skip_preflight: true,
                        ..rpc_config::RpcSendTransactionConfig::default()
                    },
                )
            }
        })
        .await??)
    }

    pub async fn subscribe_to_market(&self, currency_pair: &CurrencyPair, market: &MarketData) {
        let market_info = market.metadata;

        let ask_request_id = RequestId::generate();
        self.subscription_requests.write().insert(
            ask_request_id,
            SubscriptionMarketData {
                currency_pair: *currency_pair,
                side: Side::Ask,
            },
        );

        let bid_request_id = RequestId::generate();
        self.subscription_requests.write().insert(
            bid_request_id,
            SubscriptionMarketData {
                currency_pair: *currency_pair,
                side: Side::Bid,
            },
        );

        join!(
            self.subscribe_to_address_changed(ask_request_id, &market_info.asks_address),
            self.subscribe_to_address_changed(bid_request_id, &market_info.bids_address)
        );
    }

    pub fn handle_on_message(&self, message: &str) -> SolanaMessage {
        let message: WebsocketMessage = match serde_json::from_str(message) {
            Ok(message) => message,
            Err(err) => {
                log::warn!("Unknown message type. {}. Message: {}", err, message);
                return SolanaMessage::Unknown;
            }
        };

        match message {
            WebsocketMessage::SubscribeResult(subscribe_result) => {
                let subscription_market_data = self
                    .subscription_requests
                    .write()
                    .remove(&subscribe_result.id)
                    .with_expect(|| {
                        format!(
                            "Subscription request was not found for id {}",
                            subscribe_result.id
                        )
                    });

                self.subscriptions
                    .write()
                    .insert(subscribe_result.result, subscription_market_data);

                SolanaMessage::Service
            }
            WebsocketMessage::AccountNotification(account_notification) => {
                let subscription_id = account_notification.params.subscription;
                let read_guard = self.subscriptions.read();
                let subscription_market_data = read_guard
                    .get(&subscription_id)
                    .with_expect(|| format!("Subscription was not found for id {subscription_id}"));

                SolanaMessage::AccountUpdated(
                    subscription_market_data.currency_pair,
                    subscription_market_data.side,
                    account_notification.params.result.value,
                )
            }
        }
    }

    async fn subscribe_to_address_changed(&self, request_id: RequestId, pubkey: &Pubkey) {
        let config = Some(RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::JsonParsed),
            commitment: Some(CommitmentConfig {
                commitment: CommitmentLevel::Confirmed,
            }),
            data_slice: None,
        });

        let message = json!({
            "jsonrpc":"2.0",
            "id":request_id,
            "method":"accountSubscribe",
            "params":[
                pubkey.to_string(),
                config
            ]
        })
        .to_string();

        let send_websocket_message_future =
            (&self.send_websocket_message_callback.lock())(WebSocketRole::Main, message);
        send_websocket_message_future.await
    }
}
