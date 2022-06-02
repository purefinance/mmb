use crate::ws::actors::ws_client_session::WsClientSession;
use crate::ws::broker_messages::{
    ClearSubscriptions, ClientConnected, ClientDisconnected, GatherSubscriptions,
    GetLiquiditySubscriptions, GetSessionLiquiditySubscription,
};
use crate::ws::subscribes::liquidity::LiquiditySubscription;
use actix::{
    Actor, ActorFutureExt, Addr, Context, ContextFutureSpawner, Handler, MessageResult, Supervised,
    SystemService, WrapFuture,
};
use actix_broker::BrokerSubscribe;
use futures::future::join_all;
use std::collections::HashSet;

#[derive(Default, Clone)]
pub struct SubscriptionManager {
    clients: HashSet<Addr<WsClientSession>>,
    liquidity_subscriptions: HashSet<LiquiditySubscription>,
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
    }
}

impl Handler<ClientDisconnected> for SubscriptionManager {
    type Result = ();

    fn handle(&mut self, msg: ClientDisconnected, _ctx: &mut Context<Self>) {
        self.clients.remove(&msg.data);
    }
}

/// Get all clients subscriptions
impl Handler<GatherSubscriptions> for SubscriptionManager {
    type Result = ();
    fn handle(&mut self, _msg: GatherSubscriptions, ctx: &mut Context<Self>) -> Self::Result {
        let futures = self
            .clients
            .iter()
            .map(|client| client.send(GetSessionLiquiditySubscription));

        join_all(futures)
            .into_actor(self)
            .map(|messages, current_actor, _| {
                for message in messages {
                    match message {
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
                        Err(e) => log::error!("Invalid subscription message {:?}", e),
                    }
                }
            })
            .wait(ctx);
    }
}

/// Clear subscription cache
impl Handler<ClearSubscriptions> for SubscriptionManager {
    type Result = ();
    fn handle(&mut self, _msg: ClearSubscriptions, _ctx: &mut Context<Self>) -> Self::Result {
        self.liquidity_subscriptions.clear();
    }
}

impl Handler<GetLiquiditySubscriptions> for SubscriptionManager {
    type Result = MessageResult<GetLiquiditySubscriptions>;
    fn handle(
        &mut self,
        _msg: GetLiquiditySubscriptions,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        MessageResult(self.liquidity_subscriptions.clone())
    }
}

impl SystemService for SubscriptionManager {}

impl Supervised for SubscriptionManager {}
