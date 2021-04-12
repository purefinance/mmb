use crate::core::exchanges::cancellation_token::CancellationToken;
use crate::core::{
    connectivity::{
        connectivity_manager::WebSocketState::Disconnected,
        websocket_actor::{self, ForceClose, WebSocketActor, WebSocketParams},
    },
    exchanges::common::ExchangeAccountId,
};
use actix::Addr;
use anyhow::Result;
use futures::Future;
use log::{error, info, log, trace, warn, Level};
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
    role: WebSocketRole,
    state: WebSocketState,
}

impl WebSocketConnectivity {
    pub fn new(role: WebSocketRole) -> WebSocketConnectivity {
        WebSocketConnectivity {
            role,
            state: WebSocketState::Disconnected,
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
        websocket_actor: Addr<WebSocketActor>,
        finished_sender: broadcast::Sender<()>,
    },
}

struct WebSockets {
    main: Mutex<WebSocketConnectivity>,
    secondary: Mutex<WebSocketConnectivity>,
}

impl WebSockets {
    fn get_websocket_state(&self, role: WebSocketRole) -> &Mutex<WebSocketConnectivity> {
        match role {
            WebSocketRole::Main => &self.main,
            WebSocketRole::Secondary => &self.secondary,
        }
    }
}

// TODO Find more clear names in the future
type Callback0 = Box<dyn FnMut()>;
type Callback1<T, U> = Box<dyn FnMut(T) -> U>;
pub type GetWSParamsCallback = Box<
    dyn FnMut(WebSocketRole) -> Pin<Box<dyn Future<Output = Result<WebSocketParams>>>>
        + Send
        + Sync,
>;
type WSMessageReceived = Box<dyn FnMut(&str)>;

pub type MsgReceivedCallback = Box<dyn FnMut(String)>;

pub struct ConnectivityManager {
    exchange_account_id: ExchangeAccountId,
    callback_get_ws_params: Mutex<GetWSParamsCallback>,
    websockets: WebSockets,

    callback_connecting: Mutex<Callback0>,
    callback_connected: Mutex<Callback0>,
    callback_disconnected: Mutex<Callback1<bool, ()>>,
    pub callback_msg_received: Mutex<WSMessageReceived>,
}

impl ConnectivityManager {
    pub fn new(exchange_account_id: ExchangeAccountId) -> Arc<ConnectivityManager> {
        Arc::new(Self {
            exchange_account_id,
            websockets: WebSockets {
                main: Mutex::new(WebSocketConnectivity::new(WebSocketRole::Main)),
                secondary: Mutex::new(WebSocketConnectivity::new(WebSocketRole::Secondary)),
            },

            callback_connecting: Mutex::new(Box::new(|| {})),
            callback_connected: Mutex::new(Box::new(|| {})),
            callback_disconnected: Mutex::new(Box::new(|_| {})),
            callback_get_ws_params: Mutex::new(Box::new(|_| {
                panic!("This callback has to be set during ConnectivityManager::connect()")
            })),

            callback_msg_received: Mutex::new(Box::new(|_| {
                panic!("This callback has to be set externally")
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
        trace!(
            "ConnectivityManager '{}' connecting",
            self.exchange_account_id
        );

        self.set_callback_ws_params(get_websocket_params);

        self.callback_connecting.lock().as_mut()();

        let main_websocket_connection_opened = self
            .clone()
            .open_websocket_connection(WebSocketRole::Main)
            .await;

        let secondary_websocket_connection_opened = if is_enabled_secondary_websocket {
            self.clone()
                .open_websocket_connection(WebSocketRole::Secondary)
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

    async fn disconnect_for_websocket(websocket_connectivity: &Mutex<WebSocketConnectivity>) {
        let guard = websocket_connectivity.lock();

        let mut finished_receiver = match &guard.state {
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
                websocket_actor,
                finished_sender,
            } => {
                if websocket_actor.connected() {
                    let _ = websocket_actor.try_send(ForceClose);
                    finished_sender.subscribe()
                } else {
                    return;
                }
            }
        };

        drop(guard);

        let _ = finished_receiver.recv().await;
    }

    pub fn send(&self, role: WebSocketRole, message: &str) {
        if let WebSocketState::Connected {
            ref websocket_actor,
            ..
        } = self
            .websockets
            .get_websocket_state(role)
            .lock()
            .borrow()
            .state
        {
            let sending_result =
                websocket_actor.try_send(websocket_actor::SendText(message.to_owned()));
            if let Err(ref err) = sending_result {
                error!(
                    "Error {} happened when sending to websocket {} message: {}",
                    err, self.exchange_account_id, message
                )
            }
        } else {
            error!(
                "Attempt to send message on {} when websocket is not connected: {}",
                self.exchange_account_id, message
            );
        }
    }

    fn set_disconnected_state(
        finished_sender: broadcast::Sender<()>,
        websocket_connectivity: &Mutex<WebSocketConnectivity>,
    ) {
        websocket_connectivity.lock().deref_mut().state = WebSocketState::Disconnected;
        let _ = finished_sender.send(());
    }

    pub fn notify_connection_closed(&self, websocket_role: WebSocketRole) {
        {
            let websocket_connectivity_arc = self.websockets.get_websocket_state(websocket_role);
            let mut websocket_state_guard = websocket_connectivity_arc.lock();

            {
                if let WebSocketState::Connected {
                    ref finished_sender,
                    ..
                } = websocket_state_guard.borrow().state
                {
                    let _ = finished_sender.send(()).expect("Can't send finish message in ConnectivityManager::notify_connection_closed");
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
            websocket_connectivity.lock().deref_mut().state = WebSocketState::Connecting {
                finished_sender: finished_sender.clone(),
                cancel_websocket_connecting: cancel_websocket_connecting.clone(),
            };
        }

        let mut attempt = 0;

        while !cancel_websocket_connecting.check_cancellation_requested() {
            trace!(
                "Getting WebSocket parameters for {}",
                self.exchange_account_id.clone()
            );
            let params = self.try_get_websocket_params(role).await;
            if let Ok(params) = params {
                if cancel_websocket_connecting.check_cancellation_requested() {
                    return false;
                }

                let notifier = ConnectivityManagerNotifier::new(role, Arc::downgrade(self));

                let websocket_actor = WebSocketActor::open_connection(
                    self.exchange_account_id.clone(),
                    params.clone(),
                    notifier,
                )
                .await;

                match websocket_actor {
                    Ok(websocket_actor) => {
                        websocket_connectivity.lock().deref_mut().state =
                            WebSocketState::Connected {
                                websocket_actor,
                                finished_sender: finished_sender.clone(),
                            };

                        if attempt > 0 {
                            info!(
                                "Opened websocket connection for {} after {} attempts",
                                self.exchange_account_id, attempt
                            );
                        }

                        if cancel_websocket_connecting.check_cancellation_requested() {
                            if let WebSocketState::Connected {
                                websocket_actor, ..
                            } = &websocket_connectivity.lock().borrow().state
                            {
                                let _ = websocket_actor.try_send(ForceClose);
                            }
                        }

                        return true;
                    }
                    Err(error) => {
                        warn!("Attempt to connect failed: {}", error);
                    }
                };

                attempt += 1;

                let log_level = match attempt < MAX_RETRY_CONNECT_COUNT {
                    true => Level::Warn,
                    false => Level::Error,
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
        }

        Self::set_disconnected_state(finished_sender, &websocket_connectivity);

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

    pub fn notify_websocket_connection_closed(&self, exchange_account_id: &ExchangeAccountId) {
        if let Some(connectivity_manager) = &self.connectivity_manager {
            match connectivity_manager.upgrade() {
                Some(connectivity_manager) => connectivity_manager.notify_connection_closed(self.websocket_role),
                None => info!(
                    "Unable to upgrade weak referene to ConnectivityManager instance. Probably it's dead",
                ),
            }
        } else {
            info!(
                "WebsocketActor '{}' notify about connection closed (in tests)",
                exchange_account_id
            )
        }
    }

    pub fn message_received(&self, data: &str) {
        if let Some(connectivity_manager) = &self.connectivity_manager {
            match connectivity_manager.upgrade() {
                Some(connectivity_manager) => connectivity_manager.callback_msg_received.lock()(data),
                None => info!(
                    "Unable to upgrade weak referene to ConnectivityManager instance. Probably it's dead",
                ),
            }
        } else {
            info!(
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
