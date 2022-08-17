use crate::services::data_provider::balances::BalancesService;
use crate::services::market_settings::MarketSettingsService;
use crate::ws::actors::error_listener::ErrorListener;
use crate::ws::actors::new_data_listener::NewDataListener;
use crate::ws::actors::subscription_manager::SubscriptionManager;
use crate::ws::broker_messages::{
    ClearSubscriptions, GatherSubscriptions, GetSubscriptions, NewBalancesDataMessage,
    SubscriptionErrorMessage,
};
use crate::ws::subscribes::balance::BalancesSubscription;
use crate::ws::subscribes::liquidity::LiquiditySubscription;
use crate::ws::subscribes::Subscription;
use crate::{LiquidityService, NewLiquidityDataMessage};
use actix::Addr;
use anyhow::Context;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

pub struct DataProvider {
    subscription_manager: Addr<SubscriptionManager>,
    liquidity_service: LiquidityService,
    market_settings_service: Arc<MarketSettingsService>,
    new_data_listener: Addr<NewDataListener>,
    error_listener: Addr<ErrorListener>,
    balances_service: BalancesService,
}

impl DataProvider {
    pub(crate) fn new(
        subscription_manager: Addr<SubscriptionManager>,
        liquidity_service: LiquidityService,
        market_settings_service: Arc<MarketSettingsService>,
        new_data_listener: Addr<NewDataListener>,
        error_listener: Addr<ErrorListener>,
        balances_service: BalancesService,
    ) -> DataProvider {
        Self {
            subscription_manager,
            liquidity_service,
            market_settings_service,
            balances_service,
            new_data_listener,
            error_listener,
        }
    }

    pub async fn step(&self) -> anyhow::Result<()> {
        self.subscription_manager
            .try_send(ClearSubscriptions)
            .with_context(|| "ClearSubscriptions error")?;
        self.subscription_manager
            .try_send(GatherSubscriptions)
            .with_context(|| "GatherSubscriptions error")?;
        let subscriptions_request = self.subscription_manager.send(GetSubscriptions);
        let subscriptions = timeout(Duration::from_millis(1000), subscriptions_request)
            .await
            .with_context(|| "Subscriptions request timeout")??;
        self.send_liquidity(subscriptions.liquidity).await?;
        self.send_balances(subscriptions.balances).await?;
        Ok(())
    }

    async fn send_balances(
        &self,
        balances_subscription: Option<BalancesSubscription>,
    ) -> anyhow::Result<()> {
        if let Some(sub) = balances_subscription {
            let balances = self.balances_service.get_balances().await;
            match balances {
                Ok(balances) => self
                    .new_data_listener
                    .try_send(NewBalancesDataMessage {
                        subscription: sub,
                        data: balances,
                    })
                    .with_context(|| "NewBalancesDataMessage error")?,
                Err(e) => {
                    log::error!(
                        "Failure to load balances data from database. Filters: {sub:?}. Error {e}"
                    );
                    self.send_error_message(sub.get_hash(), "Internal server error".to_string())?;
                }
            }
        }
        Ok(())
    }

    async fn send_liquidity(
        &self,
        liquidity_subscriptions: HashSet<LiquiditySubscription>,
    ) -> anyhow::Result<()> {
        for sub in liquidity_subscriptions {
            let liquidity_data = self
                .liquidity_service
                .get_liquidity_data(&sub.exchange_id, &sub.currency_pair, 20)
                .await;
            match liquidity_data {
                Ok(mut liquidity_data) => {
                    let desired_amount = self
                        .market_settings_service
                        .get_desired_amount(&sub.exchange_id, &sub.currency_pair);

                    match desired_amount {
                        None => {
                            log::error!(
                                "Desired amount is none for {} {}",
                                &sub.exchange_id,
                                &sub.currency_pair
                            );
                            self.send_error_message(sub.get_hash(), "Bad request".to_string())?;
                        }
                        Some(desired_amount) => {
                            liquidity_data.desired_amount = desired_amount;
                            let message = NewLiquidityDataMessage {
                                subscription: sub,
                                data: liquidity_data,
                            };
                            self.new_data_listener
                                .try_send(message)
                                .with_context(|| "NewLiquidityDataMessage error")?
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failure to load liquidity data from database. Filters: {sub:?}. Error: {e:?}");
                    self.send_error_message(sub.get_hash(), "Internal server error".to_string())?;
                }
            }
        }
        Ok(())
    }
    fn send_error_message(&self, subscription: u64, message: String) -> anyhow::Result<()> {
        let message = SubscriptionErrorMessage {
            subscription,
            message,
        };

        self.error_listener
            .try_send(message)
            .with_context(|| "Send error message failure")?;
        Ok(())
    }
}
