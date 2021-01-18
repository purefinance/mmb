use actix::Addr;
use crate::core::{
    exchanges::{
        common::{ExchangeId, ExchangeName},
        actor::ExchangeActor
    },
    connectivity::websocket_actor::{self, WebSocketActor}
};
// TODO change std::sync::mpsc to tokio::mpsc when it implement method try_recv()
use std::sync::mpsc;
use crate::core::connectivity::websocket_actor::{WebSocketParams, ForceClose};
use log::{error, info, log, trace, Level};
use crate::core::connectivity::connectivity_manager::WebSocketState::Disconnected;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::TryRecvError;
use crate::core::exchanges::actor::GetWebSocketParams;
use std::ops::DerefMut;
use std::borrow::Borrow;

pub const MAX_RETRY_CONNECT_COUNT: u32 = 3;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WebSocketRole {
    Main,
    Secondary
}

struct WebSocketConnectivity {
    role: WebSocketRole,
    state: WebSocketState
}

impl WebSocketConnectivity {
    pub fn new(
        role: WebSocketRole,
    ) -> WebSocketConnectivity {
        WebSocketConnectivity {
            role,
            state: WebSocketState::Disconnected
        }
    }
}


enum WebSocketState{
    Disconnected,
    Connecting {
        finished_receiver: async_channel::Receiver<()>,
        cancellation_sender: mpsc::Sender<()>
    },
    Connected {
        websocket_actor: Addr<WebSocketActor>,
        finished_sender: async_channel::Sender<()>,
        finished_receiver: async_channel::Receiver<()>,
    }
}

struct WebSockets {
    websocket_main: Mutex<WebSocketConnectivity>,
    websocket_secondary: Mutex<WebSocketConnectivity>,
}

impl WebSockets {
    fn get_websocket_state(&self, role: WebSocketRole) -> &Mutex<WebSocketConnectivity> {
        match role {
            WebSocketRole::Main => &self.websocket_main,
            WebSocketRole::Secondary => &self.websocket_secondary
        }
    }
}


type Callback0 = Box<dyn FnMut()>;
type Callback1<T> = Box<dyn FnMut(T)>;

pub struct ConnectivityManager {
    exchange_id: ExchangeId,
    exchange_name: ExchangeName,
    exchange_actor: Addr<ExchangeActor>,

    websockets: WebSockets,

    callback_connecting: Mutex<Option<Callback0>>,
    callback_connected: Mutex<Option<Callback0>>,
    callback_disconnected: Mutex<Option<Callback1<bool>>>,
}

impl ConnectivityManager {
    pub fn new(
        exchange_id: ExchangeId,
        exchange_name: ExchangeName,
        exchange_actor: Addr<ExchangeActor>
    ) -> Arc<ConnectivityManager> {
        Arc::new(
            Self {
                exchange_id,
                exchange_name,
                exchange_actor,
                websockets: WebSockets {
                    websocket_main: Mutex::new(WebSocketConnectivity::new(WebSocketRole::Main)),
                    websocket_secondary: Mutex::new(WebSocketConnectivity::new(WebSocketRole::Secondary)),
                },
                callback_connecting: Mutex::new(None),
                callback_connected: Mutex::new(None),
                callback_disconnected: Mutex::new(None)
            })
    }

    pub fn set_callback_connecting(&self, connecting: Callback0) {
        *self.callback_connecting.lock().unwrap() = Some(connecting);
    }

    pub fn set_callback_connected(&self, connected: Callback0) {
        *self.callback_connected.lock().unwrap() = Some(connected);
    }

    pub fn set_callback_disconnected(&self, disconnected: Callback1<bool>) {
        *self.callback_disconnected.lock().unwrap() = Some(disconnected)
    }

    pub async fn connect(
        self: Arc<Self>,
        is_enabled_secondary_websocket: bool
    ) -> bool {
        trace!("ConnectivityManager '{}' connecting", self.exchange_id);

        if let Some(connecting) = self.callback_connecting.lock().unwrap().as_mut() {
            connecting();
        }

        let main_websocket_connection_opened =  self.clone().open_websocket_connection(WebSocketRole::Main).await;

        let secondary_websocket_connection_opened = if is_enabled_secondary_websocket {
            self.clone().open_websocket_connection(WebSocketRole::Secondary).await
        } else {
            true
        };

        let is_connected = main_websocket_connection_opened && secondary_websocket_connection_opened;
        if is_connected {
            if let Some(connected) = self.callback_connected.lock().unwrap().as_mut() {
                connected();
            }
        }

        is_connected
    }

    pub async fn disconnect(self: Arc<Self>) {
        Self::disconnect_for_websocket(&self.websockets.websocket_main).await;
        Self::disconnect_for_websocket(&self.websockets.websocket_secondary).await;
    }

    async fn disconnect_for_websocket(websocket_connectivity: &Mutex<WebSocketConnectivity>) {
        let guard = websocket_connectivity.lock().unwrap();

        let finished_receiver = match &guard.state {
            Disconnected => { return; }
            WebSocketState::Connecting { cancellation_sender, finished_receiver } => {
                let _ = cancellation_sender.clone().send(());
                finished_receiver.clone()
            }
            WebSocketState::Connected { websocket_actor, finished_receiver, .. } => {
                if websocket_actor.connected() {
                    let _ = websocket_actor.try_send(ForceClose);
                    finished_receiver.clone()
                }
                else {
                    return;
                }
            }
        };

        drop(guard);

        let _ = finished_receiver.recv().await;
    }

    pub fn send(&self, role: WebSocketRole, message: &str) {
        if let WebSocketState::Connected { ref websocket_actor, .. } = self.websockets.get_websocket_state(role).lock().unwrap().borrow().state {
            let sending_result = websocket_actor.try_send(websocket_actor::Send(message.to_owned()));
            if let Err(ref err) = sending_result {
                error!("Error {} happened when sending to websocket {} message: {}", err, self.exchange_id, message)
            }
        }
        else {
            error!("Attempt to send message on {} when websocket is not connected: {}", self.exchange_id, message);
        }
    }

    fn set_disconnected_state(finished_sender: async_channel::Sender<()>, websocket_connectivity: &Mutex<WebSocketConnectivity>) {
        websocket_connectivity.lock().unwrap().deref_mut().state = WebSocketState::Disconnected;
        let _ = finished_sender.try_send(());
    }

    pub fn notify_connection_closed(&self, websocket_role: WebSocketRole) {

        {
            let websocket_connectivity_arc = self.websockets.get_websocket_state(websocket_role);
            let mut websocket_state_guard = websocket_connectivity_arc.lock().unwrap();

            {
                if let WebSocketState::Connected { ref finished_sender, .. } = websocket_state_guard.borrow().state {
                    let _ = finished_sender.try_send(());
                }
            }

            websocket_state_guard.deref_mut().state = Disconnected;
        }

        if let Some(disconnected) = self.callback_disconnected.lock().unwrap().as_mut() {
            disconnected(false);
        }
    }

    pub async fn open_websocket_connection(self: Arc<Self>, role: WebSocketRole) -> bool {
        let (finished_sender, finished_receiver) = async_channel::bounded(1);

        let (cancellation_sender, cancellation_receiver) = mpsc::channel();

        let websocket_connectivity = self.websockets.get_websocket_state(role);

        {
            websocket_connectivity.lock().unwrap().deref_mut().state = WebSocketState::Connecting {
                finished_receiver: finished_receiver.clone(),
                cancellation_sender,
            };
        }

        let mut attempt = 0;

        while let Err(TryRecvError::Empty) = cancellation_receiver.try_recv() {
            trace!("Getting WebSocket parameters for {}", self.exchange_id.clone());
            let params = try_get_websocket_params(self.exchange_actor.clone(), role).await;
            if let Some(params) = params {
                if let Ok(()) = cancellation_receiver.try_recv() {
                    return false;
                }

                let notifier = ConnectivityManagerNotifier::new(role, self.clone());

                let websocket_actor = WebSocketActor::open_connection(self.exchange_id.clone(), params.clone(), notifier).await;
                if let Some(websocket_actor) = websocket_actor {
                    websocket_connectivity.lock().unwrap().deref_mut().state = WebSocketState::Connected {
                        websocket_actor,
                        finished_sender: finished_sender.clone(),
                        finished_receiver: finished_receiver.clone()
                    };

                    if attempt > 0 {
                        info!("Opened websocket connection for {} after {} attempts", self.exchange_id, attempt);
                    }

                    if let Ok(()) = cancellation_receiver.try_recv() {
                        if let WebSocketState::Connected { websocket_actor, ..} = &websocket_connectivity.lock().unwrap().borrow().state {
                            let _ = websocket_actor.try_send(ForceClose);
                        }
                    }

                    return true;
                }

                attempt += 1;

                let log_level = if attempt < MAX_RETRY_CONNECT_COUNT { Level::Warn } else { Level::Error };
                log!(log_level, "Can't open websocket connection for {} {:?}", self.exchange_id, params);

                if attempt == MAX_RETRY_CONNECT_COUNT {
                    panic!("Can't open websocket connection on {}", self.exchange_id);
                }
            }

        }

        Self::set_disconnected_state(finished_sender, &websocket_connectivity);

        false
    }
}

#[derive(Clone)]
pub struct ConnectivityManagerNotifier {
    websocket_role: WebSocketRole,

    // option just for testing simplification
    connectivity_manager: Option<Arc<ConnectivityManager>>
}

impl ConnectivityManagerNotifier {
    pub fn new(websocket_role: WebSocketRole, connectivity_manager: Arc<ConnectivityManager>) -> ConnectivityManagerNotifier {
        ConnectivityManagerNotifier {
            websocket_role,
            connectivity_manager: Some(connectivity_manager)
        }
    }

    pub fn notify_websocket_connection_closed(&self, exchange_id: &ExchangeId) {
        if let Some(connectivity_manager) = &self.connectivity_manager {
            connectivity_manager.notify_connection_closed(self.websocket_role)
        }
        else {
            info!("WebsocketActor '{}' notify about connection closed (in tests)", exchange_id)
        }
    }
}

impl Default for ConnectivityManagerNotifier {
    fn default() -> Self {
        Self {
            websocket_role: WebSocketRole::Main,
            connectivity_manager: None
        }
    }
}

async fn try_get_websocket_params(exchange_actor: Addr<ExchangeActor>, role: WebSocketRole) -> Option<WebSocketParams> {
    exchange_actor.send(GetWebSocketParams(role)).await.unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::logger::init_logger;
    use actix::{Arbiter, Actor};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;
    use tokio::sync::oneshot;
    use tokio::time::sleep;

    #[actix_rt::test]
    pub async fn should_connect_and_reconnect_normally() {
        const EXPECTED_CONNECTED_COUNT: u32 = 3;

        init_logger();

        let (finish_sender, finish_receiver) = oneshot::channel::<()>();

        Arbiter::spawn(async {
            let exchange_id: ExchangeId = "Binance0".into();
            let exchange_name: ExchangeName = "Binance".into();
            let websocket_host = "wss://stream.binance.com:9443".into();
            let currency_pairs = vec![
                "bnbbtc".into(),
                "btcusdt".into()
            ];
            let channels = vec![
                "depth".into(),
                "aggTrade".into()
            ];

            let exchange_actor = ExchangeActor::new(
                exchange_id.clone(),
                websocket_host,
                currency_pairs,
                channels
            ).start();

            let connectivity_manager = ConnectivityManager::new(
                exchange_id.clone(),
                exchange_name.clone(),
                exchange_actor
            );

            let connected_count = Rc::new(RefCell::new(0));
            {
                let connected_count = connected_count.clone();
                connectivity_manager.clone().set_callback_connected(Box::new(move || { connected_count.replace_with(|x| *x + 1); }));
            }

            for _ in 0..EXPECTED_CONNECTED_COUNT {
                let connect_result = connectivity_manager.clone().connect(false).await;
                assert_eq!(connect_result, true, "websocket should connect successfully");

                connectivity_manager.clone().disconnect().await;
            }

            assert_eq!(connected_count.take(), EXPECTED_CONNECTED_COUNT, "we should reconnect expected count times");

            let _ = finish_sender.send(());
        });

        tokio::select! {
            _ = finish_receiver => info!("Test finished successfully"),
            _ = sleep(Duration::from_secs(10)) => panic!("Test time is gone!")
        }
    }
}