use crate::ws::broker_messages::{
    ClientConnected, ClientDisconnected, GetSessionLiquiditySubscription, LiquidityResponseMessage,
};
use actix::{Actor, ActorContext, AsyncContext, Handler, MessageResult, StreamHandler};
use actix_broker::{BrokerIssue, BrokerSubscribe};

use crate::ws::subscribes::liquidity::LiquiditySubscription;
use actix_web_actors::ws::{Message, ProtocolError, WebsocketContext};

#[derive(Default)]
pub struct WsClientSession {
    subscribed_liquidity: Option<LiquiditySubscription>,
}

/// Websocket client session
impl Actor for WsClientSession {
    type Context = WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<LiquidityResponseMessage>(ctx);
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
        match &self.subscribed_liquidity {
            None => return,
            Some(subscribed_liquidity) => {
                if subscribed_liquidity.exchange_id != msg.exchange_id
                    || subscribed_liquidity.currency_pair != msg.currency_pair
                {
                    return;
                }
            }
        }

        let body = serde_json::to_string(&msg.body).expect("Failed to convert json");
        let message = format!("{}|{}", &msg.command, &body);
        ctx.text(message);
        log::info!("Sent to client: command={}, body={:?}", &msg.command, body);
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
                log::error!("Failure to read message from socket: Error: {:?}", err);
                ctx.stop();
                return;
            }
        };

        let msg = match msg {
            Message::Text(message) => message.to_string(),
            _ => {
                log::error!("Incorrect message type: Message: {:?}", msg);
                ctx.stop();
                return;
            }
        };

        let mut slices = msg.splitn(2, '|');
        let command = slices.next();
        let body = slices.next().unwrap_or("");

        match command {
            None => {
                log::error!("Failure get command from message: Message: {:?}", msg);
                ctx.stop();
            }
            Some(command) => self.route(command, body, ctx),
        }
    }
}

impl WsClientSession {
    fn route(&mut self, command: &str, body: &str, ctx: &mut WebsocketContext<WsClientSession>) {
        match command {
            "SubscribeLiquidity" => self.subscribe_liquidity(ctx, body),
            "UnsubscribeLiquidity" => self.unsubscribe_liquidity(),
            _ => {
                log::error!("Unknown command: {}, body: {}", command, body);
            }
        };
    }

    fn subscribe_liquidity(&mut self, ctx: &mut WebsocketContext<WsClientSession>, body: &str) {
        match serde_json::from_str::<LiquiditySubscription>(body) {
            Ok(subscription) => {
                self.subscribed_liquidity = Some(subscription);
            }
            Err(_) => {
                ctx.stop();
                log::error!("Failed to create LiquiditySubscription from: {}", body)
            }
        };
    }

    fn unsubscribe_liquidity(&mut self) {
        self.subscribed_liquidity = None
    }
}
