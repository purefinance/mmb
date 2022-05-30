use actix::{Actor, Context};

pub struct WsServerSession;

/// Singleton websocket server session. Required for clients accounting and clients subscriptions
impl Actor for WsServerSession {
    type Context = Context<Self>;
}
