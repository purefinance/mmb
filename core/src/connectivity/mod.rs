use std::fmt::{Display, Formatter};
use thiserror::Error;
use url::Url;

mod websocket;
mod websocket_connection;

#[derive(Error, Debug)]
pub enum ConnectivityError {
    #[error("connectivity manager isn't ready")]
    NotReady,
    #[error("failed to connect socket (`{0}`, `{1}`): `{2}`")]
    FailedToConnect(WebSocketRole, String, tokio_tungstenite::tungstenite::Error),
    #[error("failed to get params for socket `{0}`: `{1}`")]
    FailedToGetParams(WebSocketRole, String),
    #[error("secondary connector is not present")]
    SecondaryConnectorIsNotPresent,
    #[error("not connected")]
    NotConnected,
}

pub type Result<T> = std::result::Result<T, ConnectivityError>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WebSocketRole {
    Main,
    Secondary,
}

impl Display for WebSocketRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WebSocketRole::Main => write!(f, "Main"),
            WebSocketRole::Secondary => write!(f, "Secondary"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebSocketParams {
    url: Url,
}

impl WebSocketParams {
    pub fn new(url: Url) -> Self {
        WebSocketParams { url }
    }
}

pub use websocket::{websocket_open, WsSender};
