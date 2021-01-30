use actix::io::{SinkWrite, WriteHandler};
use actix_codec::Framed;
use actix::{Actor, AsyncContext, Handler, StreamHandler, Message, Context, Addr, ActorContext};
use actix_web_actors::ws::ProtocolError;
use awc::{
    ws::{
        self,
        Frame,
        Codec,
        CloseReason,
        CloseCode
    },
    BoxedSocket,
    Client,
    error::WsProtocolError,
    http,
    http::Uri
};
use bytes::Bytes;
use std::{
    time::{Duration, Instant},
};
use futures::stream::{SplitSink, StreamExt};
use crate::core::exchanges::common::ExchangeAccountId;
use log::{error, info, trace};
use actix_web::web::Buf;

use crate::core::connectivity::connectivity_manager::ConnectivityManagerNotifier;
use awc::http::StatusCode;

/// Time interval between heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Time interval  before lack of client response causes a timeout
const HEARTBEAT_FAIL_TIMEOUT: Duration = Duration::from_secs(10);

const PING_MESSAGE: &'static [u8; 9] = b"heartbeat";

#[derive(Debug, Clone)]
pub struct WebSocketParams {
    url: Uri
}

impl WebSocketParams {
    pub fn new(url: Uri) -> Self {
        WebSocketParams { url }
    }
}

pub type WebsocketWriter = SinkWrite<ws::Message, SplitSink<Framed<BoxedSocket, Codec>, ws::Message>>;

pub struct WebSocketActor {
    exchange_account_id: ExchangeAccountId,
    writer: WebsocketWriter,
    last_heartbeat_time: Instant,
    connectivity_manager_notifier: ConnectivityManagerNotifier
}

impl WebSocketActor {
    pub async fn open_connection(
        exchange_account_id: ExchangeAccountId,
        params: WebSocketParams,
        connectivity_manager_notifier: ConnectivityManagerNotifier
    ) -> Option<Addr<WebSocketActor>> {
        let (response, framed) = Client::builder()
            .max_http_version(http::Version::HTTP_11)
            .finish()
            .ws(params.url.clone())
            .header("Accept-Encoding", "gzip")
            .connect()
            .await
            .map_err(|e| {
                trace!("Error: {}", e.to_string());
            })
            .unwrap();

        trace!("WebsocketActor '{}' connecting status: {}", exchange_account_id, response.status());
        if !(response.status() == StatusCode::SWITCHING_PROTOCOLS) {
            return None;
        }

        let (sink, stream) = framed.split();

        let addr = WebSocketActor::create(|ctx| {
            WebSocketActor::add_stream(stream, ctx);

            WebSocketActor::new(
                exchange_account_id,
                SinkWrite::new(sink, ctx),
                connectivity_manager_notifier
            )
        });

        Some(addr)
    }

    fn new(
        exchange_account_id: ExchangeAccountId,
        writer: WebsocketWriter,
        connectivity_manager_notifier: ConnectivityManagerNotifier
    ) -> Self {
        Self {
            exchange_account_id,
            writer,
            last_heartbeat_time: Instant::now(),
            connectivity_manager_notifier
        }
    }

    fn write(&mut self, msg: ws::Message) {
        match self.writer.write(msg) {
            None => {}
            Some(msg) => error!("WebsocketActor '{}' can't send message '{:?}'", self.exchange_account_id, msg)
        }
    }

    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        let notifier = self.connectivity_manager_notifier.clone();
        let exchange_id = self.exchange_account_id.clone();
        ctx.run_interval(HEARTBEAT_INTERVAL, move |act, _ctx| {
            if Instant::now().duration_since(act.last_heartbeat_time) > HEARTBEAT_FAIL_TIMEOUT {
                trace!("WebsocketActor '{}' heartbeat failed, disconnecting!", exchange_id);

                notifier.notify_websocket_connection_closed(&exchange_id);

                return;
            }

            act.write(ws::Message::Ping(Bytes::from_static(PING_MESSAGE)))
        });
    }

    fn close_websocket(&self, ctx: &mut Context<Self>){
        self.connectivity_manager_notifier.notify_websocket_connection_closed(&self.exchange_account_id);
        ctx.stop();
    }

    fn handle_websocket_message(&self, text: &Bytes) {
        info!("ws text {:?}", text);
        // TODO
    }
}

impl Actor for WebSocketActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        trace!("WebSocketActor '{}' started", self.exchange_account_id);
        self.hb(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("WebSocketActor '{}' stopped", self.exchange_account_id);

        self.connectivity_manager_notifier.notify_websocket_connection_closed(&self.exchange_account_id);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Send(pub String);

#[derive(Message)]
#[rtype(result = "()")]
pub struct ForceClose;

impl Handler<Send> for WebSocketActor {
    type Result = ();

    fn handle(&mut self, msg: Send, _ctx: &mut Self::Context) -> Self::Result {
        info!("WebsocketActor '{}' send msg: {}", self.exchange_account_id, msg.0);
        self.write(ws::Message::Text(msg.0.into()));
    }
}

impl Handler<ForceClose> for WebSocketActor {
    type Result = ();

    fn handle(&mut self, _msg: ForceClose, _ctx: &mut Self::Context) -> Self::Result {
        info!("WebsocketActor '{}' received ForceClose message", self.exchange_account_id);
        self.write(ws::Message::Close(Some(CloseReason::from(CloseCode::Normal))))
    }
}

impl StreamHandler<Result<Frame, WsProtocolError>> for WebSocketActor {
    fn handle(&mut self, msg: Result<Frame, ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(msg) => {
                match msg {
                    Frame::Text(ref text) => self.handle_websocket_message(text),
                    Frame::Binary(bytes) => trace!("WebsocketActor '{}' got binary message: {:x?}", &self.exchange_account_id, &bytes.chunk()),
                    Frame::Pong(ref msg) => {
                        if &msg[..] == PING_MESSAGE {
                            self.last_heartbeat_time = Instant::now();
                        }
                        else {
                            error!("WebsocketActor '{}' received wrong pong message: {}. We are sending message '{}' only",
                                   self.exchange_account_id,
                                   String::from_utf8_lossy(&msg),
                                   String::from_utf8_lossy(PING_MESSAGE)
                            );
                        }
                    },
                    Frame::Ping(msg) => self.write(ws::Message::Pong(msg)),
                    Frame::Close(reason) => {
                        trace!("Websocket {} closed with reason: {}", self.exchange_account_id, reason.clone().map(|x| x.description).flatten().unwrap_or("None".to_string()));
                        self.close_websocket(ctx);
                    },
                    _ => {}
                }
            }
            Err(err) => {
                error!("{}", err.to_string());
                self.close_websocket(ctx);
            }
        }
    }
}

impl WriteHandler<WsProtocolError> for WebSocketActor {}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::Arbiter;
    use crate::core::logger::init_logger;

    #[actix_rt::test]
    pub async fn connect_and_send_msg() {
        use tokio::sync::oneshot;

        init_logger();

        let url: Uri = "wss://stream.binance.com:9443/stream?streams=bnbbtc@depth".parse().unwrap();

        let (websocket_sender, websocket_receiver) = oneshot::channel::<Addr<_>>();
        let (finish_sender, finish_receiver) = oneshot::channel();

        Arbiter::spawn(async {
            let websocket_addr = websocket_receiver.await.unwrap();

            tokio::time::sleep(Duration::from_secs(1)).await;

            websocket_addr.do_send(Send(r#"{
   "method": "SUBSCRIBE",
   "params": [
     "btcusdt@aggTrade"
   ],
   "id": 1
}"#.to_string()));

            tokio::time::sleep(Duration::from_secs(1)).await;

            websocket_addr.do_send(ForceClose);

            tokio::time::sleep(Duration::from_secs(1)).await;

            assert_eq!(websocket_addr.connected(), false, "websocket should be closed to this moment");

            let _ = finish_sender.send(());
        });

        Arbiter::spawn(async {
            let exchange_id = "Binance0".parse().unwrap();
            let websocket_addr = WebSocketActor::open_connection(
                exchange_id,
                WebSocketParams { url },
                Default::default()
            ).await;
            assert_eq!(websocket_addr.clone().unwrap().connected(), true, "websocket should be connected");
            if let Some(addr) = websocket_addr {
                let _ = websocket_sender.send(addr);
            }
        });

        // Test timeout
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(10)) => panic!("Test time is gone!"),
            _ = finish_receiver => info!("Test finished successfully")
        }
    }
}