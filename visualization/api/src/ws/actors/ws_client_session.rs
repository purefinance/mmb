use crate::ws::broker_messages::{
    ClientConnected, ClientDisconnected, ClientErrorResponseMessage,
    GetSessionLiquiditySubscription, LiquidityResponseMessage,
};
use actix::{Actor, ActorContext, AsyncContext, Handler, MessageResult, StreamHandler};
use actix_broker::{BrokerIssue, BrokerSubscribe};
use actix_web::web::Data;
use std::collections::HashSet;

use crate::services::token::TokenService;
use crate::ws::subscribes::liquidity::{LiquiditySubscription, Subscription};
use actix_web_actors::ws::{Message, ProtocolError, WebsocketContext};
use serde::Deserialize;
use serde_json::Value;

pub struct WsClientSession {
    subscriptions: HashSet<u64>,
    subscribed_liquidity: Option<LiquiditySubscription>,
    token_service: Data<TokenService>,
    is_auth: bool,
}

impl WsClientSession {
    pub fn new(token_service: Data<TokenService>) -> Self {
        Self {
            subscriptions: HashSet::new(),
            subscribed_liquidity: None,
            token_service,
            is_auth: false,
        }
    }
}

/// Websocket client session
impl Actor for WsClientSession {
    type Context = WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<LiquidityResponseMessage>(ctx);
        self.subscribe_system_async::<ClientErrorResponseMessage>(ctx);
        let message = ClientConnected {
            data: ctx.address(),
        };
        self.issue_system_async(message);
        log::info!("Websocket client connected");
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
                send_message(ctx, msg.command, &body);
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
            send_message(ctx, msg.command, &msg.content);
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
            _ => {
                log::error!("Incorrect message type: Message: {msg:?}");
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
            // Subscription for one record of order book (20 orders) and last 20 transactions
            "SubscribeLiquidity" => self.subscribe_liquidity(ctx, body),
            // Unsubscribe from "SubscribeLiquidity"
            "UnsubscribeLiquidity" => self.unsubscribe_liquidity(),
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
                let message = format!("Authorized|{}", self.is_auth);
                ctx.text(message);
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

    fn unsubscribe_liquidity(&mut self) {
        match &self.subscribed_liquidity {
            None => {}
            Some(subscription) => {
                self.subscriptions.remove(&subscription.get_hash());
                self.subscribed_liquidity = None;
            }
        }
    }
}

fn send_message(ctx: &mut WebsocketContext<WsClientSession>, command: &str, content: &Value) {
    let message = format!("{command}|{content}");
    ctx.text(message);
    log::trace!("Sent to client: command={command}, body={content}");
}
