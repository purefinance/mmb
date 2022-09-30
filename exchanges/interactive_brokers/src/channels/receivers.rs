use crate::channels::channel_type::ChannelType;
use dashmap::DashMap;
use function_name::named;
use ibtwsapi::core::messages::ServerRspMsg;
use tokio::sync::broadcast;
// use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
// use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

pub struct ChannelReceivers {
    channels: DashMap<ChannelType, broadcast::Sender<ServerRspMsg>>,
}

impl ChannelReceivers {
    pub fn new(channels: DashMap<ChannelType, broadcast::Sender<ServerRspMsg>>) -> Self {
        Self { channels }
    }

    #[named]
    pub async fn recv(&self, key: ChannelType) -> ServerRspMsg {
        let f_n = function_name!();

        self.channels
            .get(&key)
            .unwrap_or_else(|| panic!("fn {f_n}: channel: {:?}, Error: channel not found.", key))
            .subscribe()
            .recv()
            .await
            .unwrap_or_else(|e| panic!("fn {f_n}: channel: {:?}, Receive error: {e}.", key))
    }
}
