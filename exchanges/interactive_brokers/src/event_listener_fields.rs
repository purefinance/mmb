use crate::channels::senders::ChannelSenders;
use crate::handlers::Handlers;
use ibtwsapi::core::client::EClient;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct EventListenerFields {
    pub client: Arc<Mutex<EClient>>,
    pub channel_senders: ChannelSenders,
    pub handlers: Handlers,
}
