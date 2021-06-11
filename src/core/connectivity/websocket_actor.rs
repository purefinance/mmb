use crate::core::connectivity::connectivity_manager::{ConnectivityManagerNotifier, WebSocketRole};
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::nothing_to_do;
use actix::io::{SinkWrite, WriteHandler};
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message, StreamHandler};
use actix_codec::Framed;
use actix_web::web::Buf;
use actix_web_actors::ws::ProtocolError;
use anyhow::{bail, Result};
use awc::http::StatusCode;
use awc::{
    error::WsProtocolError,
    http,
    http::Uri,
    ws::{self, CloseCode, CloseReason, Codec, Frame},
    BoxedSocket, Client,
};
use bytes::Bytes;
use futures::stream::{SplitSink, StreamExt};
use log::{error, info, trace, warn};
use std::time::{Duration, Instant};

/// Time interval between heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Time interval before lack of client response causes a timeout
const HEARTBEAT_FAIL_TIMEOUT: Duration = Duration::from_secs(10);

const PING_MESSAGE: &'static [u8; 9] = b"heartbeat";

#[derive(Debug, Clone)]
pub struct WebSocketParams {
    url: Uri,
}

impl WebSocketParams {
    pub fn new(url: Uri) -> Self {
        WebSocketParams { url }
    }
}

pub type WebsocketWriter =
    SinkWrite<ws::Message, SplitSink<Framed<BoxedSocket, Codec>, ws::Message>>;

pub struct WebSocketActor {
    exchange_account_id: ExchangeAccountId,
    role: WebSocketRole,
    writer: WebsocketWriter,
    last_heartbeat_time: Instant,
    connectivity_manager_notifier: ConnectivityManagerNotifier,
}

impl WebSocketActor {
    pub async fn open_connection(
        exchange_account_id: ExchangeAccountId,
        role: WebSocketRole,
        params: WebSocketParams,
        connectivity_manager_notifier: ConnectivityManagerNotifier,
    ) -> Result<Addr<WebSocketActor>> {
        let connected_client = Client::builder()
            .max_http_version(http::Version::HTTP_11)
            .finish()
            .ws(params.url)
            .header("Accept-Encoding", "gzip")
            .connect()
            .await;

        match connected_client {
            Err(error) => {
                bail!("Error occurred during websocket connect: {}", error);
            }
            Ok(connected_client) => {
                let (response, framed) = connected_client;

                trace!(
                    "WebsocketActor '{}' connecting status: {}",
                    exchange_account_id,
                    response.status()
                );
                if !(response.status() == StatusCode::SWITCHING_PROTOCOLS) {
                    bail!("Status code is SWITCHING_PROTOCOLS so unable to communicate")
                }

                let (sink, stream) = framed.split();

                let addr = WebSocketActor::create(|ctx| {
                    WebSocketActor::add_stream(stream, ctx);

                    WebSocketActor::new(
                        exchange_account_id,
                        role,
                        SinkWrite::new(sink, ctx),
                        connectivity_manager_notifier,
                    )
                });

                Ok(addr)
            }
        }
    }

    fn new(
        exchange_account_id: ExchangeAccountId,
        role: WebSocketRole,
        writer: WebsocketWriter,
        connectivity_manager_notifier: ConnectivityManagerNotifier,
    ) -> Self {
        Self {
            exchange_account_id,
            role,
            writer,
            last_heartbeat_time: Instant::now(),
            connectivity_manager_notifier,
        }
    }

    fn write(&mut self, msg: ws::Message) {
        match self.writer.write(msg) {
            Ok(_) => nothing_to_do(),
            Err(msg) => error!(
                "WebsocketActor {} {:?} can't send message '{:?}'",
                self.exchange_account_id, self.role, msg
            ),
        }
    }

    fn heartbeat(&self, ctx: &mut <Self as Actor>::Context) {
        let notifier = self.connectivity_manager_notifier.clone();
        let exchange_account_id = self.exchange_account_id.clone();
        let role = self.role;
        ctx.run_interval(HEARTBEAT_INTERVAL, move |act, _ctx| {
            if Instant::now().duration_since(act.last_heartbeat_time) > HEARTBEAT_FAIL_TIMEOUT {
                trace!(
                    "WebsocketActor {} {:?} heartbeat failed, disconnecting!",
                    exchange_account_id,
                    role,
                );

                notifier.notify_websocket_connection_closed(&exchange_account_id);

                return;
            }

            act.write(ws::Message::Ping(Bytes::from_static(PING_MESSAGE)))
        });
    }

    fn close_websocket(&self, ctx: &mut Context<Self>) {
        self.connectivity_manager_notifier
            .notify_websocket_connection_closed(&self.exchange_account_id);
        ctx.stop();
    }

    fn handle_websocket_message(&self, bytes: &Bytes) {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                self.connectivity_manager_notifier
                    .clone()
                    .message_received(text);
            }
            Err(error) => {
                warn!("Unable to parse websocket message: {:?}", error)
            }
        }
    }
}

impl Actor for WebSocketActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        trace!(
            "WebSocketActor {} {:?} started",
            self.exchange_account_id,
            self.role
        );
        self.heartbeat(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!(
            "WebSocketActor {} {:?} stopped",
            self.exchange_account_id,
            self.role
        );

        self.connectivity_manager_notifier
            .notify_websocket_connection_closed(&self.exchange_account_id);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendText(pub String);

#[derive(Message)]
#[rtype(result = "()")]
pub struct ForceClose;

impl Handler<SendText> for WebSocketActor {
    type Result = ();

    fn handle(&mut self, msg: SendText, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            "WebsocketActor {} {:?} send msg: {}",
            self.exchange_account_id, self.role, msg.0
        );
        self.write(ws::Message::Text(msg.0.into()));
    }
}

impl Handler<ForceClose> for WebSocketActor {
    type Result = ();

    fn handle(&mut self, _msg: ForceClose, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            "WebsocketActor {} {:?} received ForceClose message",
            self.exchange_account_id, self.role,
        );

        let close_reason = CloseReason::from(CloseCode::Normal);
        self.write(ws::Message::Close(Some(close_reason)))
    }
}

impl StreamHandler<Result<Frame, WsProtocolError>> for WebSocketActor {
    fn handle(&mut self, msg: Result<Frame, ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(msg) => match msg {
                Frame::Text(ref text) => self.handle_websocket_message(text),
                Frame::Binary(bytes) => trace!(
                    "WebsocketActor {} {:?} got binary message: {:x?}",
                    self.exchange_account_id,
                    self.role,
                    bytes.chunk()
                ),
                Frame::Pong(ref msg) => {
                    if &msg[..] == PING_MESSAGE {
                        self.last_heartbeat_time = Instant::now();
                    } else {
                        error!("WebsocketActor {} {:?} received wrong pong message: {}. We are sending message '{}' only",
                                   self.exchange_account_id,
                                   self.role,
                                   String::from_utf8_lossy(&msg),
                                   String::from_utf8_lossy(PING_MESSAGE)
                            );
                    }
                }
                Frame::Ping(msg) => self.write(ws::Message::Pong(msg)),
                Frame::Close(reason) => {
                    trace!(
                        "Websocket {} {:?} closed with reason: {}",
                        self.exchange_account_id,
                        self.role,
                        reason
                            .clone()
                            .map(|x| x.description)
                            .flatten()
                            .unwrap_or("None".to_string())
                    );
                    self.close_websocket(ctx);
                }
                _ => {}
            },
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
    use crate::core::logger::init_logger;

    // TODO It is not UNIT test
    #[actix_rt::test]
    pub async fn connect_and_send_msg() {
        use tokio::sync::oneshot;

        init_logger();

        let url: Uri = "wss://stream.binance.com:9443/stream?streams=bnbbtc@depth"
            .parse()
            .expect("in test");

        let (websocket_sender, websocket_receiver) = oneshot::channel::<Addr<_>>();
        let (finish_sender, finish_receiver) = oneshot::channel();

        actix_rt::spawn(async {
            let websocket_addr = websocket_receiver.await.expect("in test");

            tokio::time::sleep(Duration::from_secs(1)).await;

            websocket_addr.do_send(SendText(
                r#"{
   "method": "SUBSCRIBE",
   "params": [
     "btcusdt@aggTrade"
   ],
   "id": 1
}"#
                .to_string(),
            ));

            tokio::time::sleep(Duration::from_secs(1)).await;

            websocket_addr.do_send(ForceClose);

            tokio::time::sleep(Duration::from_secs(1)).await;

            assert_eq!(
                websocket_addr.connected(),
                false,
                "websocket should be closed to this moment"
            );

            let _ = finish_sender.send(());
        });

        actix_rt::spawn(async {
            let exchange_id = "Binance0".parse().expect("in test");
            let websocket_addr = WebSocketActor::open_connection(
                exchange_id,
                WebSocketRole::Main,
                WebSocketParams { url },
                Default::default(),
            )
            .await
            .expect("in test");

            assert_eq!(
                websocket_addr.clone().connected(),
                true,
                "websocket should be connected"
            );

            let _ = websocket_sender.send(websocket_addr);
        });

        // Test timeout
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(10)) => panic!("Test time is gone!"),
            _ = finish_receiver => info!("Test finished successfully")
        }
    }
}
