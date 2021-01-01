use crate::core::exchanges::common::{ExchangeId, ExchangeName};
use tungstenite::{
    connect,
    Message,
    WebSocket,
    handshake::client::Request,
    client::AutoStream
};
use hyper::client::conn::handshake;
use std::thread;
use std::sync::{Mutex, Arc};
use std::thread::JoinHandle;

pub struct WebSocketClient {
    exchange_id: ExchangeId,
    exchange_name: ExchangeName,
    websocket: Arc<Mutex<Option<WebSocket<AutoStream>>>>,
}

impl WebSocketClient {
    pub fn new(exchange_id: ExchangeId, exchange_name: ExchangeName) -> Self {
        WebSocketClient {
            exchange_id,
            exchange_name,
            websocket: Default::default()
        }
    }

    pub fn open(&self, uri: &str) {
        let request = Request::builder()
            .uri(uri)
            .header("Accept-Encoding", "gzip")
            .body(())
            .unwrap();

        run_event_loop(request, self.websocket.clone());
    }

    pub fn send(&self, message: String) {
        if let Some(ref mut websocket_lock) = &mut *self.websocket.lock().expect("Can't get lock for websocket") {
            println!("sent: {}", message);
            websocket_lock.write_message(Message::text(message));
        }
    }

    pub fn close(&self) {
        let mut websocket = self.websocket.lock().expect("Can't get lock for websocket").take();
        if let Some(mut websocket) = websocket {
            websocket.write_message(Message::Close(None));
            println!("sent message close");
        }
    }
}

fn run_event_loop(request: Request, websocket: Arc<Mutex<Option<WebSocket<AutoStream>>>>){
    {
        let (mut socket, _) = connect(request).expect("Can't connect to web-socket");
        *websocket.lock().expect("Can't get lock for websocket") = Some(socket);
    }

    println!("Connected to the server");

    let handle = thread::spawn(move || {
        loop {
            let mut websocket_guard = websocket.lock().expect("Can't get lock for websocket");
            if let Some(ref mut websocket) = &mut *websocket_guard
            {
                let msg = websocket.read_message().expect("Error reading message");
                std::mem::drop(websocket_guard);
                println!("Received: {}", msg);
            }
            else {
                // websocket is closed
                break;
            }
        }
    });

    handle.join();
}