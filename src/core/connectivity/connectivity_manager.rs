use crate::core::{
    connectivity::{
        connectivity_manager::WebSocketState::Disconnected,
        websocket_actor::{self, ForceClose, WebSocketActor, WebSocketParams},
    },
    exchanges::common::ExchangeAccountId,
};

use actix::Addr;
use log::{error, info, log, trace, Level};
use parking_lot::Mutex;
use std::{
    borrow::Borrow,
    ops::DerefMut,
    sync::{
        mpsc, // TODO change std::sync::mpsc to tokio::mpsc when it implement method try_recv()
        mpsc::TryRecvError,
        Arc,
    },
};

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
        finished_receiver: async_channel::Receiver<()>,
        cancellation_sender: mpsc::Sender<()>,
    },
    Connected {
        websocket_actor: Addr<WebSocketActor>,
        finished_sender: async_channel::Sender<()>,
        finished_receiver: async_channel::Receiver<()>,
    },
}

struct WebSockets {
    // FIXME fust main and secondary
    websocket_main: Mutex<WebSocketConnectivity>,
    websocket_secondary: Mutex<WebSocketConnectivity>,
}

impl WebSockets {
    fn get_websocket_state(&self, role: WebSocketRole) -> &Mutex<WebSocketConnectivity> {
        match role {
            WebSocketRole::Main => &self.websocket_main,
            WebSocketRole::Secondary => &self.websocket_secondary,
        }
    }
}

// FIXME What a strange names
type Callback0 = Box<dyn FnMut()>;
type Callback1<T, U> = Box<dyn FnMut(T) -> U>;

pub struct ConnectivityManager {
    exchange_account_id: ExchangeAccountId,
    callback_get_ws_params: Mutex<Callback1<WebSocketRole, Option<WebSocketParams>>>,
    websockets: WebSockets,

    callback_connecting: Mutex<Callback0>,
    callback_connected: Mutex<Callback0>,
    callback_disconnected: Mutex<Callback1<bool, ()>>,
    callback_msg_received: Mutex<Callback1<String, ()>>,
}

impl ConnectivityManager {
    pub fn new(exchange_account_id: ExchangeAccountId) -> Arc<ConnectivityManager> {
        //exchange_actor: Addr<ExchangeActor>,
        Arc::new(Self {
            exchange_account_id,
            websockets: WebSockets {
                websocket_main: Mutex::new(WebSocketConnectivity::new(WebSocketRole::Main)),
                websocket_secondary: Mutex::new(WebSocketConnectivity::new(
                    WebSocketRole::Secondary,
                )),
            },

            // TODO this is bad approach
            callback_connecting: Mutex::new(Box::new(|| {})),
            callback_connected: Mutex::new(Box::new(|| {})),
            callback_disconnected: Mutex::new(Box::new(|_| {})),
            callback_get_ws_params: Mutex::new(Box::new(|_| {
                panic!("This callback has to be set externally")
            })),
            callback_msg_received: Mutex::new(Box::new(|_| {})),
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

    pub fn set_callback_msg_received(&self, data_received: Callback1<String, ()>) {
        *self.callback_msg_received.lock() = data_received;

        if let WebSocketState::Connected {
            ref websocket_actor,
            ..
        } = self.websockets.websocket_main.lock().borrow().state
        {}
    }

    //pub async fn connect(&self, _: bool) -> bool {
    //    // TODO build_websocket_params
    //    // TODO build_second_websocket_params
    //    (self.callback_msg_received).lock()("CALLBACK WORKS!".to_owned());

    //    true
    //}

    pub fn set_callback_ws_params(
        &self,
        get_websocket_params: Callback1<WebSocketRole, Option<WebSocketParams>>,
    ) {
        *self.callback_get_ws_params.lock() = get_websocket_params;
    }

    pub async fn connect(self: Arc<Self>, is_enabled_secondary_websocket: bool) -> bool {
        trace!(
            "ConnectivityManager '{}' connecting",
            self.exchange_account_id
        );

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
            // TODO callback_connected()?
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
        Self::disconnect_for_websocket(&self.websockets.websocket_main).await;
        Self::disconnect_for_websocket(&self.websockets.websocket_secondary).await;
    }

    async fn disconnect_for_websocket(websocket_connectivity: &Mutex<WebSocketConnectivity>) {
        let guard = websocket_connectivity.lock();

        let finished_receiver = match &guard.state {
            Disconnected => {
                return;
            }

            WebSocketState::Connecting {
                cancellation_sender,
                finished_receiver,
            } => {
                let _ = cancellation_sender.clone().send(());
                finished_receiver.clone()
            }
            WebSocketState::Connected {
                websocket_actor,
                finished_receiver,
                ..
            } => {
                if websocket_actor.connected() {
                    let _ = websocket_actor.try_send(ForceClose);
                    finished_receiver.clone()
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
        finished_sender: async_channel::Sender<()>,
        websocket_connectivity: &Mutex<WebSocketConnectivity>,
    ) {
        websocket_connectivity.lock().deref_mut().state = WebSocketState::Disconnected;
        let _ = finished_sender.try_send(());
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
                    let _ = finished_sender.try_send(());
                }
            }

            websocket_state_guard.deref_mut().state = Disconnected;
        }

        self.callback_disconnected.lock().as_mut()(false);
    }

    pub async fn open_websocket_connection(self: Arc<Self>, role: WebSocketRole) -> bool {
        let (finished_sender, finished_receiver) = async_channel::bounded(1);

        let (cancellation_sender, cancellation_receiver) = mpsc::channel();

        let websocket_connectivity = self.websockets.get_websocket_state(role);

        {
            websocket_connectivity.lock().deref_mut().state = WebSocketState::Connecting {
                finished_receiver: finished_receiver.clone(),
                cancellation_sender,
            };
        }

        let mut attempt = 0;

        while let Err(TryRecvError::Empty) = cancellation_receiver.try_recv() {
            trace!(
                "Getting WebSocket parameters for {}",
                self.exchange_account_id.clone()
            );
            let params = self.try_get_websocket_params(role).await;
            if let Some(params) = params {
                if let Ok(()) = cancellation_receiver.try_recv() {
                    return false;
                }

                let notifier = ConnectivityManagerNotifier::new(role, self.clone());

                let websocket_actor = WebSocketActor::open_connection(
                    self.exchange_account_id.clone(),
                    params.clone(),
                    notifier,
                )
                .await;

                if let Some(websocket_actor) = websocket_actor {
                    websocket_connectivity.lock().deref_mut().state = WebSocketState::Connected {
                        websocket_actor,
                        finished_sender: finished_sender.clone(),
                        finished_receiver: finished_receiver.clone(),
                    };

                    if attempt > 0 {
                        info!(
                            "Opened websocket connection for {} after {} attempts",
                            self.exchange_account_id, attempt
                        );
                    }

                    //if let Ok(()) = cancellation_receiver.try_recv() {
                    //    if let WebSocketState::Connected {
                    //        websocket_actor, ..
                    //    } = &websocket_connectivity.lock().borrow().state
                    //    {
                    //        let callback_msg_received = self.callback_msg_received.lock();
                    //        let callback = TextReceivedCallback {
                    //            callback_msg_received,
                    //        };
                    //        //let _ = websocket_actor.try_send(TextReceivedCallback {
                    //        //    callback_msg_received,
                    //        //});
                    //    }
                    //}

                    // TODO Why????
                    //if let Ok(()) = cancellation_receiver.try_recv() {
                    //    if let WebSocketState::Connected {
                    //        websocket_actor, ..
                    //    } = &websocket_connectivity.lock().borrow().state
                    //    {
                    //        let _ = websocket_actor.try_send(ForceClose);
                    //    }
                    //}

                    return true;
                }

                attempt += 1;

                let log_level = if attempt < MAX_RETRY_CONNECT_COUNT {
                    Level::Warn
                } else {
                    Level::Error
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

    async fn try_get_websocket_params(&self, role: WebSocketRole) -> Option<WebSocketParams> {
        (self.callback_get_ws_params).lock()(role)
    }
}

#[derive(Clone)]
pub struct ConnectivityManagerNotifier {
    websocket_role: WebSocketRole,

    // option just for testing simplification
    connectivity_manager: Option<Arc<ConnectivityManager>>,
}

impl ConnectivityManagerNotifier {
    pub fn new(
        websocket_role: WebSocketRole,
        connectivity_manager: Arc<ConnectivityManager>,
    ) -> ConnectivityManagerNotifier {
        ConnectivityManagerNotifier {
            websocket_role,
            connectivity_manager: Some(connectivity_manager),
        }
    }

    pub fn notify_websocket_connection_closed(&self, exchange_account_id: &ExchangeAccountId) {
        if let Some(connectivity_manager) = &self.connectivity_manager {
            connectivity_manager.notify_connection_closed(self.websocket_role)
        } else {
            info!(
                "WebsocketActor '{}' notify about connection closed (in tests)",
                exchange_account_id
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::exchanges::binance::Binance;
    use crate::core::exchanges::exchange::Exchange;
    use crate::core::logger::init_logger;
    use crate::core::settings::ExchangeSettings;
    use actix::Arbiter;
    use std::{cell::RefCell, ops::Deref, rc::Rc, time::Duration};
    use tokio::{sync::oneshot, time::sleep};

    #[actix_rt::test]
    pub async fn should_connect_and_reconnect_normally() {
        const EXPECTED_CONNECTED_COUNT: u32 = 3;

        init_logger();

        let (finish_sender, finish_receiver) = oneshot::channel::<()>();

        let exchange_account_id: ExchangeAccountId = "Binance0".parse().unwrap();
        let websocket_host = "wss://stream.binance.com:9443".into();
        let currency_pairs = vec!["phbbtc".into(), "btcusdt".into()];
        let channels = vec!["depth".into(), "aggTrade".into()];
        let exchange_interaction = Arc::new(Binance::new(
            ExchangeSettings::default(),
            exchange_account_id.clone(),
        ));

        let exchange = Exchange::new(
            exchange_account_id.clone(),
            websocket_host,
            currency_pairs,
            channels,
            exchange_interaction,
        );

        let exchange_weak = Arc::downgrade(&exchange);
        let connectivity_manager = ConnectivityManager::new(exchange_account_id.clone());
        connectivity_manager
            .clone()
            .set_callback_ws_params(Box::new(move |params| {
                let exchange = exchange_weak.upgrade().unwrap();
                exchange.get_websocket_params(params)
            }));

        let connected_count = Rc::new(RefCell::new(0));
        {
            let connected_count = connected_count.clone();
            connectivity_manager
                .clone()
                .set_callback_connected(Box::new(move || {
                    connected_count.replace_with(|x| *x + 1);
                }));
        }

        for _ in 0..EXPECTED_CONNECTED_COUNT {
            let connect_result = connectivity_manager.clone().connect(false).await;
            assert_eq!(
                connect_result, true,
                "websocket should connect successfully"
            );

            connectivity_manager.clone().disconnect().await;
        }

        assert_eq!(
            connected_count.deref().replace(0),
            EXPECTED_CONNECTED_COUNT,
            "we should reconnect expected count times"
        );

        let _ = finish_sender.send(());

        tokio::select! {
            _ = finish_receiver => info!("Test finished successfully"),
            _ = sleep(Duration::from_secs(10)) => panic!("Test time is gone!")
        }
    }
}
