use crate::{
    connectivity::{
        connectivity_manager::WebSocketState::Disconnected,
        websocket_connection::{WebSocketConnection, WebSocketParams},
    },
    exchanges::common::ExchangeAccountId,
};
use anyhow::Result;
use futures::Future;
use log::log;
use mmb_utils::{cancellation_token::CancellationToken, send_expected::SendExpectedByRef};
use parking_lot::Mutex;
use std::pin::Pin;
use std::{
    borrow::Borrow,
    ops::DerefMut,
    sync::{Arc, Weak},
};
use tokio::sync::broadcast;

pub const MAX_RETRY_CONNECT_COUNT: u32 = 3;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WebSocketRole {
    Main,
    Secondary,
}

struct WebSocketConnectivity {
    state: WebSocketState,
}

impl WebSocketConnectivity {
    pub fn new() -> WebSocketConnectivity {
        WebSocketConnectivity {
            state: Disconnected,
        }
    }
}

enum WebSocketState {
    Disconnected,
    Connecting {
        finished_sender: broadcast::Sender<()>,
        cancel_websocket_connecting: CancellationToken,
    },
    Connected {
        websocket: Arc<WebSocketConnection>,
        finished_sender: broadcast::Sender<()>,
    },
}

struct WebSockets {
    main: tokio::sync::Mutex<WebSocketConnectivity>,
    secondary: tokio::sync::Mutex<WebSocketConnectivity>,
}

impl WebSockets {
    fn get_websocket_state(
        &self,
        role: WebSocketRole,
    ) -> &tokio::sync::Mutex<WebSocketConnectivity> {
        match role {
            WebSocketRole::Main => &self.main,
            WebSocketRole::Secondary => &self.secondary,
        }
    }
}

// TODO Find more clear names in the future
type Callback0 = Box<dyn Fn() + Send>;
type Callback1<T, U> = Box<dyn Fn(T) -> U + Send>;
pub type GetWSParamsCallback = Box<
    dyn Fn(WebSocketRole) -> Pin<Box<dyn Future<Output = Result<WebSocketParams>>>> + Send + Sync,
>;
type WSMessageReceived = Box<dyn Fn(&str) + Send>;

pub type MsgReceivedCallback = Box<dyn Fn(String)>;

pub struct ConnectivityManager {
    exchange_account_id: ExchangeAccountId,
    callback_get_ws_params: Mutex<GetWSParamsCallback>,
    websockets: WebSockets,

    callback_connecting: Mutex<Callback0>,
    callback_connected: Mutex<Callback0>,
    callback_disconnected: Mutex<Callback1<bool, ()>>,
    callback_msg_received: Mutex<WSMessageReceived>,
}

impl ConnectivityManager {
    pub fn new(exchange_account_id: ExchangeAccountId) -> Arc<ConnectivityManager> {
        Arc::new(Self {
            exchange_account_id,
            websockets: WebSockets {
                main: tokio::sync::Mutex::new(WebSocketConnectivity::new()),
                secondary: tokio::sync::Mutex::new(WebSocketConnectivity::new()),
            },

            callback_connecting: Mutex::new(Box::new(|| {})),
            callback_connected: Mutex::new(Box::new(|| {})),
            callback_disconnected: Mutex::new(Box::new(|_| {})),
            callback_get_ws_params: Mutex::new(Box::new(|_| {
                panic!("callback_get_ws_params has to be set during ConnectivityManager::connect()")
            })),

            callback_msg_received: Mutex::new(Box::new(|_| {
                panic!("callback_msg_received has to be set during ConnectivityManager::connect()")
            })),
        })
    }

    pub fn set_callback_connecting(&self, connecting: Callback0) {
        *self.callback_connecting.lock() = connecting;
    }

    pub fn set_callback_connected(&self, connected: Callback0) {
        *self.callback_connected.lock() = connected;
    }

    pub fn set_callback_disconnected(&self, disconnected: Callback1<bool, ()>) {
        *self.callback_disconnected.lock() = disconnected;
    }

    pub fn set_callback_msg_received(&self, msg_received: WSMessageReceived) {
        *self.callback_msg_received.lock() = msg_received;
    }

    fn set_callback_ws_params(&self, get_websocket_params: GetWSParamsCallback) {
        *self.callback_get_ws_params.lock() = get_websocket_params;
    }

    pub async fn connect(
        self: Arc<Self>,
        is_enabled_secondary_websocket: bool,
        get_websocket_params: GetWSParamsCallback,
    ) -> bool {
        log::trace!(
            "ConnectivityManager '{}' connecting",
            self.exchange_account_id
        );

        self.set_callback_ws_params(get_websocket_params);

        self.callback_connecting.lock().as_mut()();

        let main_websocket_connection_opened =
            self.open_websocket_connection(WebSocketRole::Main).await;

        let secondary_websocket_connection_opened = if is_enabled_secondary_websocket {
            self.open_websocket_connection(WebSocketRole::Secondary)
                .await
        } else {
            true
        };

        let is_connected =
            main_websocket_connection_opened && secondary_websocket_connection_opened;
        if is_connected {
            self.callback_connected.lock().as_mut()();
        }

        is_connected
    }

    pub async fn disconnect(self: Arc<Self>) {
        Self::disconnect_for_websocket(&self.websockets.main).await;
        Self::disconnect_for_websocket(&self.websockets.secondary).await;
    }

    async fn disconnect_for_websocket(
        websocket_connectivity: &tokio::sync::Mutex<WebSocketConnectivity>,
    ) {
        let mut finished_receiver = match &websocket_connectivity.lock().await.state {
            Disconnected => {
                return;
            }

            WebSocketState::Connecting {
                cancel_websocket_connecting,
                finished_sender,
            } => {
                cancel_websocket_connecting.cancel();
                finished_sender.subscribe()
            }
            WebSocketState::Connected {
                websocket,
                finished_sender,
            } => {
                if websocket.is_connected() {
                    let _ = websocket.send_force_close().await;
                    finished_sender.subscribe()
                } else {
                    return;
                }
            }
        };

        let _ = finished_receiver.recv().await;
    }

    pub async fn send(&self, role: WebSocketRole, message: &str) {
        if let WebSocketState::Connected { ref websocket, .. } = self
            .websockets
            .get_websocket_state(role)
            .lock()
            .await
            .borrow()
            .state
        {
            let sending_result = websocket.send_string(message.to_owned()).await;
            if let Err(ref err) = sending_result {
                log::error!(
                    "Error {} happened when sending to websocket {} message: {}",
                    err.to_string(),
                    self.exchange_account_id,
                    message
                )
            }
        } else {
            log::error!(
                "Attempt to send message on {} when websocket is not connected: {}",
                self.exchange_account_id,
                message
            );
        }
    }

    async fn set_disconnected_state(
        finished_sender: broadcast::Sender<()>,
        websocket_connectivity: &tokio::sync::Mutex<WebSocketConnectivity>,
    ) {
        websocket_connectivity.lock().await.deref_mut().state = Disconnected;
        let _ = finished_sender.send(());
    }

    pub async fn notify_connection_closed(&self, websocket_role: WebSocketRole) {
        {
            let websocket_connectivity_arc = self.websockets.get_websocket_state(websocket_role);
            let mut websocket_state_guard = websocket_connectivity_arc.lock().await;

            {
                if let WebSocketState::Connected {
                    ref finished_sender,
                    ..
                } = websocket_state_guard.borrow().state
                {
                    finished_sender.send_expected(());
                }
            }

            websocket_state_guard.deref_mut().state = Disconnected;
        }

        self.callback_disconnected.lock().as_mut()(false);
    }

    pub async fn open_websocket_connection(self: &Arc<Self>, role: WebSocketRole) -> bool {
        let (finished_sender, _) = broadcast::channel(50);

        let cancel_websocket_connecting = CancellationToken::new();

        let websocket_connectivity = self.websockets.get_websocket_state(role);

        {
            websocket_connectivity.lock().await.deref_mut().state = WebSocketState::Connecting {
                finished_sender: finished_sender.clone(),
                cancel_websocket_connecting: cancel_websocket_connecting.clone(),
            };
        }

        let mut attempt = 0;

        while !cancel_websocket_connecting.is_cancellation_requested() {
            log::trace!(
                "Getting WebSocket parameters for {}",
                self.exchange_account_id
            );
            match self.try_get_websocket_params(role).await {
                Ok(params) => {
                    if cancel_websocket_connecting.is_cancellation_requested() {
                        return false;
                    }

                    let notifier = ConnectivityManagerNotifier::new(role, Arc::downgrade(self));

                    let websocket = WebSocketConnection::open_connection(
                        self.exchange_account_id,
                        role,
                        params.clone(),
                        notifier,
                    )
                    .await;

                    match websocket {
                        Ok(websocket) => {
                            websocket_connectivity.lock().await.deref_mut().state =
                                WebSocketState::Connected {
                                    websocket,
                                    finished_sender: finished_sender.clone(),
                                };

                            if attempt > 0 {
                                log::info!(
                                    "Opened websocket connection for {} after {} attempts",
                                    self.exchange_account_id,
                                    attempt
                                );
                            }

                            if cancel_websocket_connecting.is_cancellation_requested() {
                                if let WebSocketState::Connected { websocket, .. } =
                                    &websocket_connectivity.lock().await.borrow().state
                                {
                                    let _ = websocket.send_force_close();
                                }
                            }

                            return true;
                        }
                        Err(error) => {
                            log::warn!("Attempt to connect failed: {:?}", error);
                        }
                    };

                    attempt += 1;

                    let log_level = match attempt < MAX_RETRY_CONNECT_COUNT {
                        true => log::Level::Warn,
                        false => log::Level::Error,
                    };
                    log!(
                        log_level,
                        "Can't open websocket connection for {} {:?}",
                        self.exchange_account_id,
                        params
                    );

                    if attempt == MAX_RETRY_CONNECT_COUNT {
                        panic!(
                            "Can't open websocket connection on {}",
                            self.exchange_account_id
                        );
                    }
                }
                Err(error) => log::warn!(
                    "Error while getting parameters for websocket {:?}: {:#}",
                    role,
                    error
                ),
            }
        }

        Self::set_disconnected_state(finished_sender, websocket_connectivity).await;

        false
    }

    async fn try_get_websocket_params(&self, role: WebSocketRole) -> Result<WebSocketParams> {
        (self.callback_get_ws_params).lock()(role).await
    }
}

#[derive(Clone)]
pub struct ConnectivityManagerNotifier {
    websocket_role: WebSocketRole,

    // option just for testing simplification
    connectivity_manager: Option<Weak<ConnectivityManager>>,
}

impl ConnectivityManagerNotifier {
    pub fn new(
        websocket_role: WebSocketRole,
        connectivity_manager: Weak<ConnectivityManager>,
    ) -> ConnectivityManagerNotifier {
        ConnectivityManagerNotifier {
            websocket_role,
            connectivity_manager: Some(connectivity_manager),
        }
    }

    pub async fn notify_websocket_connection_closed(&self, exchange_account_id: ExchangeAccountId) {
        if let Some(connectivity_manager) = &self.connectivity_manager {
            match connectivity_manager.upgrade() {
                Some(connectivity_manager) => {
                    connectivity_manager
                        .notify_connection_closed(self.websocket_role)
                        .await
                }
                None => {
                    log::info!("Unable to upgrade weak reference to ConnectivityManager instance")
                }
            }
        } else {
            log::info!(
                "WebsocketActor {} {:?} notify about connection closed (in tests)",
                exchange_account_id,
                self.websocket_role
            )
        }
    }

    pub fn message_received(&self, data: &str) {
        if let Some(connectivity_manager) = &self.connectivity_manager {
            match connectivity_manager.upgrade() {
                Some(connectivity_manager) => connectivity_manager.callback_msg_received.lock()(data),
                None => log::info!(
                    "Unable to upgrade weak reference to ConnectivityManager instance. Probably it's dropped",
                ),
            }
        } else {
            log::info!(
                "WebsocketActor '{:?}' notify that new text message accepted",
                data
            )
        }
    }
}

impl Default for ConnectivityManagerNotifier {
    fn default() -> Self {
        Self {
            websocket_role: WebSocketRole::Main,
            connectivity_manager: None,
        }
    }
}
