use std::collections::HashSet;

use actix::{
    Actor, ActorFutureExt, Addr, Context, ContextFutureSpawner, Handler, MessageResult, Supervised,
    SystemService, WrapFuture,
};
use actix_broker::BrokerSubscribe;
use futures::future::join_all;

use crate::ws::actors::ws_client_session::WsClientSession;
use crate::ws::broker_messages::{
    ClearSubscriptions, ClientConnected, ClientDisconnected, GatherSubscriptions,
    GetSessionBalancesSubscription, GetSessionLiquiditySubscription, GetSubscriptions,
    GetSubscriptionsResponse,
};
use crate::ws::subscribes::balance::BalancesSubscription;
use crate::ws::subscribes::liquidity::LiquiditySubscription;

#[derive(Default, Clone)]
pub struct SubscriptionManager {
    clients: HashSet<Addr<WsClientSession>>,
    liquidity_subscriptions: HashSet<LiquiditySubscription>,
    balances_subscriptions: Option<BalancesSubscription>,
}

impl SubscriptionManager {
    pub(crate) fn gather_balances_subscriptions(&self, ctx: &mut Context<SubscriptionManager>) {
        let futures = self
            .clients
            .iter()
            .map(|client| client.send(GetSessionBalancesSubscription));

        join_all(futures)
            .into_actor(self)
            .map(|messages, current_actor, _| {
                for message in messages {
                    match message {
                        Ok(message) => {
                            if message.is_some() {
                                current_actor.balances_subscriptions =
                                    Some(BalancesSubscription {});
                                break;
                            }
                        }
                        Err(e) => log::error!("Invalid subscription message {e:?}"),
                    }
                }
            })
            .wait(ctx);
    }
}

impl SubscriptionManager {
    pub(crate) fn gather_liquidity_subscriptions(&self, ctx: &mut Context<SubscriptionManager>) {
        let futures = self
            .clients
            .iter()
            .map(|client| client.send(GetSessionLiquiditySubscription));

        join_all(futures)
            .into_actor(self)
            .map(|messages, current_actor, _| {
                for message in messages {
                    match message {
                        #[allow(clippy::single_match)]
                        Ok(message) => match message {
                            Some(liquidity_subscription) => {
                                let _ = current_actor
                                    .liquidity_subscriptions
                                    .insert(liquidity_subscription);
                            }
                            None => {
                                // client doesn't have liquidity subscription
                            }
                        },
                        Err(e) => log::error!("Invalid subscription message {e:?}"),
                    }
                }
            })
            .wait(ctx);
    }
}

impl Actor for SubscriptionManager {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<ClientConnected>(ctx);
        self.subscribe_system_async::<ClientDisconnected>(ctx);
        log::info!("Subscription Manager started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("Subscription Manager stopped");
    }
}

impl Handler<ClientConnected> for SubscriptionManager {
    type Result = ();
    fn handle(&mut self, msg: ClientConnected, _ctx: &mut Context<Self>) {
        self.clients.insert(msg.data);
        log::debug!("Client added to subscription manager")
    }
}

impl Handler<ClientDisconnected> for SubscriptionManager {
    type Result = ();

    fn handle(&mut self, msg: ClientDisconnected, _ctx: &mut Context<Self>) {
        self.clients.remove(&msg.data);
        log::debug!("Client removed from subscription manager")
    }
}

/// Get all clients subscriptions
impl Handler<GatherSubscriptions> for SubscriptionManager {
    type Result = ();
    fn handle(&mut self, _msg: GatherSubscriptions, ctx: &mut Context<Self>) -> Self::Result {
        log::debug!("GatherSubscriptions executed");
        self.gather_liquidity_subscriptions(ctx);
        self.gather_balances_subscriptions(ctx);
        log::debug!("GatherSubscriptions finished");
    }
}

/// Clear subscription cache
impl Handler<ClearSubscriptions> for SubscriptionManager {
    type Result = ();
    fn handle(&mut self, _msg: ClearSubscriptions, _ctx: &mut Context<Self>) -> Self::Result {
        log::debug!("ClearSubscriptions executed");
        self.liquidity_subscriptions.clear();
    }
}

impl Handler<GetSubscriptions> for SubscriptionManager {
    type Result = MessageResult<GetSubscriptions>;
    fn handle(&mut self, _msg: GetSubscriptions, _ctx: &mut Context<Self>) -> Self::Result {
        log::debug!("GetSubscriptions executed");
        let response = GetSubscriptionsResponse {
            liquidity: self.liquidity_subscriptions.clone(),
            balances: self.balances_subscriptions.clone(),
        };
        MessageResult(response)
    }
}

impl SystemService for SubscriptionManager {}

impl Supervised for SubscriptionManager {}
