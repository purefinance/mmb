use crate::ws::broker_messages::MessageToClient;
use actix::{Actor, Handler, MessageResult, StreamHandler};
use actix_broker::BrokerSubscribe;
use actix_web_actors::ws;

#[derive(Default)]
pub struct WsClientSession;

/// Websocket client session
impl Actor for WsClientSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("Websocket client connected");
        self.subscribe_system_async::<MessageToClient>(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("Websocket client disconnected");
    }
}

/// Global message handler. Intercepting raised MessageToClient event
impl Handler<MessageToClient> for WsClientSession {
    type Result = MessageResult<MessageToClient>;

    fn handle(
        &mut self,
        msg: MessageToClient,
        ctx: &mut ws::WebsocketContext<Self>,
    ) -> Self::Result {
        let message = format!("{}|{}", &msg.command, &msg.content);
        ctx.text(message);
        log::info!(
            "Sent to client: command={}, content={}",
            &msg.command,
            &msg.content
        );
        MessageResult(())
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsClientSession {
    fn handle(&mut self, _msg: Result<ws::Message, ws::ProtocolError>, _ctx: &mut Self::Context) {}
}
