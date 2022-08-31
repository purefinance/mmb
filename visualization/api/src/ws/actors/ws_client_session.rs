use crate::ws::broker_messages::{
    BalancesResponseMessage, ClientConnected, ClientDisconnected, ClientErrorResponseMessage,
    GetSessionBalancesSubscription, GetSessionLiquiditySubscription, LiquidityResponseMessage,
};
use actix::{Actor, ActorContext, AsyncContext, Handler, MessageResult, StreamHandler};
use actix_broker::{BrokerIssue, BrokerSubscribe};
use actix_web::web::Data;
use std::collections::HashSet;

use crate::services::token::TokenService;
use crate::ws::subscribes::balance::BalancesSubscription;
use crate::ws::subscribes::liquidity::LiquiditySubscription;
use crate::ws::subscribes::Subscription;
use actix_web_actors::ws::{Message, ProtocolError, WebsocketContext};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct WsClientSession {
    subscriptions: HashSet<u64>,
    subscribed_liquidity: Option<LiquiditySubscription>,
    subscribed_balances: Option<BalancesSubscription>,
    token_service: Data<TokenService>,
    is_auth: bool,
    hb: Instant,
}

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(3);

impl WsClientSession {
    pub fn new(token_service: Data<TokenService>) -> Self {
        Self {
            subscriptions: HashSet::new(),
            subscribed_liquidity: None,
            subscribed_balances: None,
            token_service,
            is_auth: false,
            hb: Instant::now(),
        }
    }

    fn hb(&mut self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

/// Websocket client session
impl Actor for WsClientSession {
    type Context = WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<LiquidityResponseMessage>(ctx);
        self.subscribe_system_async::<BalancesResponseMessage>(ctx);
        self.subscribe_system_async::<ClientErrorResponseMessage>(ctx);
        let message = ClientConnected {
            data: ctx.address(),
        };
        self.issue_system_async(message);
        log::info!("Websocket client connected");
        self.hb(ctx);
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        let message = ClientDisconnected {
            data: ctx.address(),
        };
        self.issue_system_async(message);
        log::info!("Websocket client disconnected");
    }
}

/// Global message handler. Intercepting raised LiquidityResponseMessage event
impl Handler<LiquidityResponseMessage> for WsClientSession {
    type Result = ();
    fn handle(
        &mut self,
        msg: LiquidityResponseMessage,
        ctx: &mut WebsocketContext<Self>,
    ) -> Self::Result {
        if !self.is_auth {
            return;
        }
        match &self.subscribed_liquidity {
            None => return,
            Some(subscribed_liquidity) => {
                if &msg.subscription != subscribed_liquidity {
                    return;
                }
            }
        };

        match serde_json::to_value(&msg.body) {
            Ok(body) => {
                send_message(ctx, msg.command, body);
            }
            Err(e) => {
                log::error!("Failure convert to json. Error: {e:?}")
            }
        };
    }
}

impl Handler<BalancesResponseMessage> for WsClientSession {
    type Result = ();
    fn handle(
        &mut self,
        msg: BalancesResponseMessage,
        ctx: &mut WebsocketContext<Self>,
    ) -> Self::Result {
        if !self.is_auth {
            return;
        }
        match &self.subscribed_balances {
            None => return,
            Some(subscribed_balances) => {
                if &msg.subscription != subscribed_balances {
                    return;
                }
            }
        };

        match serde_json::to_value(&msg.body) {
            Ok(body) => {
                send_message(ctx, msg.command, body);
            }
            Err(e) => {
                log::error!("Failure convert to json. Error: {e:?}")
            }
        };
    }
}

impl Handler<ClientErrorResponseMessage> for WsClientSession {
    type Result = ();
    fn handle(
        &mut self,
        msg: ClientErrorResponseMessage,
        ctx: &mut WebsocketContext<Self>,
    ) -> Self::Result {
        if self.subscriptions.contains(&msg.subscription) {
            send_message(ctx, msg.command, msg.content);
        }
    }
}

impl Handler<GetSessionLiquiditySubscription> for WsClientSession {
    type Result = MessageResult<GetSessionLiquiditySubscription>;

    fn handle(
        &mut self,
        _msg: GetSessionLiquiditySubscription,
        _ctx: &mut WebsocketContext<Self>,
    ) -> Self::Result {
        MessageResult(self.subscribed_liquidity.clone())
    }
}

impl Handler<GetSessionBalancesSubscription> for WsClientSession {
    type Result = MessageResult<GetSessionBalancesSubscription>;

    fn handle(
        &mut self,
        _msg: GetSessionBalancesSubscription,
        _ctx: &mut WebsocketContext<Self>,
    ) -> Self::Result {
        MessageResult(self.subscribed_balances.clone())
    }
}

impl StreamHandler<Result<Message, ProtocolError>> for WsClientSession {
    fn handle(&mut self, msg: Result<Message, ProtocolError>, ctx: &mut Self::Context) {
        log::info!("Received message: {:?}", msg);

        let msg = match msg {
            Ok(message) => message,
            Err(err) => {
                log::error!("Failure to read message from socket: Error: {err:?}");
                ctx.stop();
                return;
            }
        };

        let msg = match msg {
            Message::Text(message) => message.to_string(),
            Message::Close(reason) => {
                log::info!("Socket close message: Reason: {reason:?}");
                ctx.stop();
                return;
            }
            Message::Ping(_) => {
                ctx.pong(&[]);
                return;
            }
            Message::Pong(_) => {
                self.hb = Instant::now();
                return;
            }
            _ => {
                log::error!("Unhandled socket message: {msg:?}");
                ctx.stop();
                return;
            }
        };

        let mut slices = msg.splitn(2, '|');
        let command = slices.next();
        let body = slices.next().unwrap_or("");

        match command {
            None => {
                log::error!("Failure get command from message: Message: {msg:?}");
                ctx.stop();
            }
            Some(command) => self.route(command, body, ctx),
        }
    }
}

#[derive(Clone, Deserialize)]
struct Auth {
    token: String,
}

impl WsClientSession {
    fn route(&mut self, command: &str, body: &str, ctx: &mut WebsocketContext<WsClientSession>) {
        match command {
            // Authorization
            "Auth" => self.auth(ctx, body),
            "Ping" => self.ping(ctx),
            // Subscription for one record of order book (20 orders) and last 20 transactions
            "SubscribeLiquidity" => self.subscribe_liquidity(ctx, body),
            // Unsubscribe from "SubscribeLiquidity"
            "UnsubscribeLiquidity" => self.unsubscribe_liquidity(),
            "SubscribeBalances" => self.subscribe_balances(),
            "UnsubscribeBalances" => self.unsubscribe_balances(),
            _ => {
                log::error!("Unknown command: {command}, body: {body}");
            }
        };
    }

    fn auth(&mut self, ctx: &mut WebsocketContext<WsClientSession>, body: &str) {
        match serde_json::from_str::<Auth>(body) {
            Ok(auth) => {
                let res = self.token_service.parse_access_token(&auth.token);
                self.is_auth = res.is_ok();
                send_message(ctx, "Authorized", json!({"value": self.is_auth}));
            }
            Err(e) => {
                ctx.stop();
                log::error!("Failed to create Auth from: {body}.  Error: {e:?}")
            }
        };
    }

    fn subscribe_liquidity(&mut self, ctx: &mut WebsocketContext<WsClientSession>, body: &str) {
        match serde_json::from_str::<LiquiditySubscription>(body) {
            Ok(subscription) => {
                self.subscriptions.insert(subscription.get_hash());
                self.subscribed_liquidity = Some(subscription);
            }
            Err(e) => {
                ctx.stop();
                log::error!("Failed to create LiquiditySubscription from: {body}. Error: {e:?}")
            }
        };
    }

    fn subscribe_balances(&mut self) {
        let subscription = BalancesSubscription::default();
        self.subscriptions.insert(subscription.get_hash());
        self.subscribed_balances = Some(BalancesSubscription {});
    }

    fn unsubscribe_balances(&mut self) {
        let subscription = BalancesSubscription::default();
        self.subscriptions.remove(&subscription.get_hash());
        self.subscribed_balances = None;
    }

    fn unsubscribe_liquidity(&mut self) {
        match &self.subscribed_liquidity {
            None => {}
            Some(subscription) => {
                self.subscriptions.remove(&subscription.get_hash());
                self.subscribed_liquidity = None;
            }
        }
    }
    fn ping(&self, ctx: &mut WebsocketContext<WsClientSession>) {
        send_message(ctx, "Pong", Value::Null)
    }
}

fn send_message(ctx: &mut WebsocketContext<WsClientSession>, command: &str, content: Value) {
    let message = format!("{command}|{content}");
    ctx.text(message);
    log::trace!("Sent to client: command={command}, body={content}");
}
