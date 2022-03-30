use crate::connectivity::connectivity_manager::{ConnectivityManagerNotifier, WebSocketRole};
use crate::exchanges::common::ExchangeAccountId;
use std::fmt::Display;

use crate::infrastructure::spawn_future;
use anyhow::{Context as AnyhowContext, Result};
use futures::stream::{SplitSink, SplitStream};
use futures::{FutureExt, SinkExt, StreamExt};
use mmb_utils::infrastructure::SpawnFutureFlags;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time;
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use url::Url;

/// Time interval between heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Time interval before lack of client response causes a timeout
const HEARTBEAT_FAIL_TIMEOUT: Duration = Duration::from_secs(10);

const PING_MESSAGE: &[u8; 9] = b"heartbeat";

#[derive(Debug, Clone)]
pub struct WebSocketParams {
    url: Url,
}

impl WebSocketParams {
    pub fn new(url: Url) -> Self {
        WebSocketParams { url }
    }
}

pub type WebSocketWriter = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
pub type WebSocketReader = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub struct WebSocketConnection {
    exchange_account_id: ExchangeAccountId,
    role: WebSocketRole,
    writer: tokio::sync::Mutex<WebSocketWriter>,
    last_heartbeat_time: Mutex<Instant>,
    connectivity_manager_notifier: ConnectivityManagerNotifier,
    is_connected: Mutex<bool>,
}

impl WebSocketConnection {
    pub async fn open_connection(
        exchange_account_id: ExchangeAccountId,
        role: WebSocketRole,
        params: WebSocketParams,
        connectivity_manager_notifier: ConnectivityManagerNotifier,
    ) -> Result<Arc<Self>> {
        let (ws_stream, response) = connect_async(params.url)
            .await
            .context("Error occurred during websocket connect")?;

        log::trace!(
            "Websocket {} {:?} connecting status: {}",
            exchange_account_id,
            role,
            response.status()
        );

        let (writer, reader) = ws_stream.split();

        let ws = Arc::new(WebSocketConnection::new(
            exchange_account_id,
            role,
            writer,
            connectivity_manager_notifier,
            true,
        ));

        spawn_future(
            "Run read message from websocket",
            SpawnFutureFlags::STOP_BY_TOKEN,
            Self::read_message_from_websocket(ws.clone(), reader).boxed(),
        );

        spawn_future(
            "Run heartbeat for websocket",
            SpawnFutureFlags::STOP_BY_TOKEN,
            Self::heartbeat(ws.clone()).boxed(),
        );

        Ok(ws)
    }

    pub async fn send_string(&self, text: String) -> std::result::Result<(), Error> {
        log::info!(
            "WebsocketActor {} {:?} send msg: {}",
            self.exchange_account_id,
            self.role,
            text
        );
        self.send(Message::Text(text)).await
    }

    pub async fn send_force_close(&self) -> std::result::Result<(), Error> {
        log::info!(
            "WebsocketActor {} {:?} received ForceClose message",
            self.exchange_account_id,
            self.role,
        );
        self.send(Message::Close(None)).await?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        *self.is_connected.lock()
    }

    fn new(
        exchange_account_id: ExchangeAccountId,
        role: WebSocketRole,
        writer: WebSocketWriter,
        connectivity_manager_notifier: ConnectivityManagerNotifier,
        is_connected: bool,
    ) -> Self {
        Self {
            exchange_account_id,
            role,
            writer: tokio::sync::Mutex::new(writer),
            last_heartbeat_time: Mutex::new(Instant::now()),
            connectivity_manager_notifier,
            is_connected: Mutex::new(is_connected),
        }
    }

    async fn send(&self, msg: Message) -> std::result::Result<(), Error> {
        let mut writer = self.writer.lock().await;
        writer.send(msg).await
    }

    async fn send_pong(&self, msg: Vec<u8>) {
        let send_result = self.send(Message::Pong(msg)).await;
        if let Err(err) = send_result {
            log::error!(
                "Websocket {} {:?} can't send pong message '{}'",
                self.exchange_account_id,
                self.role,
                err.to_string()
            )
        }
    }

    async fn read_message_from_websocket(
        this: Arc<WebSocketConnection>,
        reader: WebSocketReader,
    ) -> Result<()> {
        reader
            .for_each(|msg| async {
                match msg {
                    Ok(msg) => this.handle_websocket_message(msg).await,
                    Err(err) => {
                        log::error!("Websocket received wrong message {}", err.to_string());
                        this.close_websocket().await
                    }
                }
            })
            .await;

        Ok(())
    }

    async fn heartbeat(this: Arc<WebSocketConnection>) -> Result<()> {
        let mut heartbeat_interval = time::interval(HEARTBEAT_INTERVAL);
        loop {
            heartbeat_interval.tick().await;
            let last_heartbeat_time = *this.last_heartbeat_time.lock();
            if Instant::now().duration_since(last_heartbeat_time) > HEARTBEAT_FAIL_TIMEOUT {
                log::trace!(
                    "Websocket {} {:?} heartbeat failed, disconnecting!",
                    this.exchange_account_id,
                    this.role,
                );
                this.close_websocket().await;
                break;
            }

            let sending_result = this.send(Message::Ping(PING_MESSAGE.to_vec())).await;
            if let Err(err) = sending_result {
                this.close_websocket().await;
                log::error!(
                    "Websocket {} {:?} can't send ping message {}",
                    this.exchange_account_id,
                    this.role,
                    err.to_string()
                )
            }
        }

        Ok(())
    }

    async fn handle_websocket_message(&self, msg: Message) {
        match msg {
            Message::Text(ref text) => self.connectivity_manager_notifier.message_received(text),
            Message::Binary(bytes) => log::trace!(
                "Websocket {} {:?} got binary message: {:x?}",
                self.exchange_account_id,
                self.role,
                bytes
            ),
            Message::Ping(msg) => self.send_pong(msg).await,
            Message::Pong(msg) => {
                if &msg[..] == PING_MESSAGE {
                    *self.last_heartbeat_time.lock() = Instant::now();
                } else {
                    log::error!("Websocket {} {:?} received wrong pong message: {}. We are sending message '{}' only",
                        self.exchange_account_id,
                        self.role, String::from_utf8_lossy(&msg),
                        String::from_utf8_lossy(PING_MESSAGE));
                }
            }
            Message::Close(ref reason) => {
                log::trace!(
                    "Websocket {} {:?} closed with reason: {}",
                    self.exchange_account_id,
                    self.role,
                    match reason {
                        Some(ref reason) => &reason.reason as &dyn Display,
                        None => &"None" as &dyn Display,
                    }
                );
                self.close_websocket().await;
            }
        }
    }

    async fn close_websocket(&self) {
        *self.is_connected.lock() = false;
        self.connectivity_manager_notifier
            .notify_websocket_connection_closed(self.exchange_account_id)
            .await;
        let _ = self.writer.lock().await.close().await;
    }
}
