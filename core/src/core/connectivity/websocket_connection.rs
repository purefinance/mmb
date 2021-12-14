use crate::core::connectivity::connectivity_manager::{ConnectivityManagerNotifier, WebSocketRole};
use crate::core::exchanges::common::ExchangeAccountId;

use anyhow::{anyhow, Context as AnyhowContext, Result};
use mmb_utils::nothing_to_do;
use parking_lot::Mutex;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Error, Message, WebSocket};
use url::Url;

/// Time interval between heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Time interval before lack of client response causes a timeout
const HEARTBEAT_FAIL_TIMEOUT: Duration = Duration::from_secs(10);

const PING_MESSAGE: &'static [u8; 9] = b"heartbeat";

#[derive(Debug, Clone)]
pub struct WebSocketParams {
    url: Url,
}

impl WebSocketParams {
    pub fn new(url: Url) -> Self {
        WebSocketParams { url }
    }
}

pub type WebSocketStream = WebSocket<MaybeTlsStream<TcpStream>>;

pub struct WebSocketConnection {
    exchange_account_id: ExchangeAccountId,
    role: WebSocketRole,
    stream: WebSocketStream,
    last_heartbeat_time: Instant,
    connectivity_manager_notifier: ConnectivityManagerNotifier,
}

impl WebSocketConnection {
    pub async fn open_connection(
        exchange_account_id: ExchangeAccountId,
        role: WebSocketRole,
        params: WebSocketParams,
        connectivity_manager_notifier: ConnectivityManagerNotifier,
    ) -> Result<Arc<Mutex<Self>>> {
        let (ws_stream, response) = connect(params.url)
            .map_err(|e| anyhow!("{:?}", e))
            .context("Error occurred during websocket connect")?;

        log::trace!(
            "Websocket {} {:?} connecting status: {}",
            exchange_account_id,
            role,
            response.status()
        );

        let ws = Arc::new(Mutex::new(WebSocketConnection::new(
            exchange_account_id,
            role,
            ws_stream,
            connectivity_manager_notifier,
        )));

        tokio::spawn(Self::read_message_from_websocket(ws.clone()));
        tokio::spawn(Self::heartbeat(ws.clone()));

        Ok(ws)
    }

    pub fn send_string(&mut self, text: &str) -> std::result::Result<(), Error> {
        log::info!(
            "WebsocketActor {} {:?} send msg: {}",
            self.exchange_account_id,
            self.role,
            text
        );
        self.send(Message::Text(text.to_owned()))
    }

    pub fn send_force_close(&mut self) -> std::result::Result<(), Error> {
        log::info!(
            "WebsocketActor {} {:?} received ForceClose message",
            self.exchange_account_id,
            self.role,
        );
        self.send(Message::Close(None))
    }

    fn send(&mut self, msg: Message) -> std::result::Result<(), Error> {
        self.stream.write_message(msg)
    }

    fn send_pong(&mut self, msg: Vec<u8>) {
        let send_result = self.send(Message::Pong(msg));
        match send_result {
            Ok(_) => nothing_to_do(),
            Err(err) => log::error!(
                "Websocket {} {:?} can't send pong message '{:?}'",
                self.exchange_account_id,
                self.role,
                err
            ),
        }
    }

    async fn read_message_from_websocket(this: Arc<Mutex<WebSocketConnection>>) {
        loop {
            let message = this.lock().stream.read_message();
            match message {
                Ok(message) => this.lock().handle_websocket_message(message),
                Err(err) => {
                    log::error!("{}", err.to_string());
                    println!("Error {:?}", err);
                    this.lock().close_websocket();
                    break;
                }
            };
        }
    }

    async fn heartbeat(this: Arc<Mutex<WebSocketConnection>>) {
        let exchange_account_id = this.lock().exchange_account_id;
        let role = this.lock().role;
        let mut heartbeat_interval = time::interval(HEARTBEAT_INTERVAL);

        loop {
            heartbeat_interval.tick().await;

            if Instant::now().duration_since(this.lock().last_heartbeat_time)
                > HEARTBEAT_FAIL_TIMEOUT
            {
                log::trace!(
                    "Websocket {} {:?} heartbeat failed, disconnecting!",
                    exchange_account_id,
                    role,
                );
                this.lock().close_websocket();
                break;
            }

            if let Err(err) = this.lock().send(Message::Ping(PING_MESSAGE.to_vec())) {
                log::error!(
                    "Websocket {} {:?} can't send ping message '{:?}'",
                    exchange_account_id,
                    role,
                    err
                );
                this.lock().close_websocket();
                break;
            }
        }
    }

    fn handle_websocket_message(&mut self, msg: Message) {
        match msg {
            Message::Text(ref text) => self.connectivity_manager_notifier.message_received(text),
            Message::Binary(bytes) => log::trace!(
                "Websocket {} {:?} got binary message: {:x?}",
                self.exchange_account_id,
                self.role,
                bytes
            ),
            Message::Ping(msg) => self.send_pong(msg),
            Message::Pong(msg) => {
                if &msg[..] == PING_MESSAGE {
                    self.last_heartbeat_time = Instant::now();
                } else {
                    log::error!("Websocket {} {:?} received wrong pong message: {}. We are sending message '{}' only",
                        self.exchange_account_id,
                        self.role,String::from_utf8_lossy(&msg),
                        String::from_utf8_lossy(PING_MESSAGE));
                }
            }
            Message::Close(reason) => {
                log::trace!(
                    "Websocket {} {:?} closed with reason: {}",
                    self.exchange_account_id,
                    self.role,
                    reason
                        .clone()
                        .map(|x| x.reason.to_string())
                        .unwrap_or("None".to_string())
                );
                self.close_websocket();
            }
        }
    }

    fn close_websocket(&mut self) {
        self.connectivity_manager_notifier
            .notify_websocket_connection_closed(self.exchange_account_id);
        let _ = self.stream.close(None);
    }

    fn new(
        exchange_account_id: ExchangeAccountId,
        role: WebSocketRole,
        stream: WebSocketStream,
        connectivity_manager_notifier: ConnectivityManagerNotifier,
    ) -> Self {
        Self {
            exchange_account_id,
            role,
            stream,
            last_heartbeat_time: Instant::now(),
            connectivity_manager_notifier,
        }
    }
}
