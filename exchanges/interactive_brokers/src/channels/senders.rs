use crate::channels::channel_type::ChannelType;
use dashmap::DashMap;
use function_name::named;
use ibtwsapi::core::messages::ServerRspMsg;
use tokio::sync::broadcast;
// use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
// use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

pub struct ChannelSenders {
    channels: DashMap<ChannelType, broadcast::Sender<ServerRspMsg>>,
}

impl ChannelSenders {
    pub fn new(channels: DashMap<ChannelType, broadcast::Sender<ServerRspMsg>>) -> Self {
        Self { channels }
    }

    #[named]
    pub fn send(&self, msg: ServerRspMsg) {
        let f_n = function_name!();

        for key in ChannelType::from_msg(&msg) {
            self.channels
                .get(key)
                .unwrap_or_else(|| {
                    panic!("fn {f_n}: channel: {:?}, Error: channel not found.", key)
                })
                .send(msg.clone())
                .unwrap_or_else(|e| panic!("fn {f_n}: channel: {:?}, Send error: {e}.", key));
        }
    }
}
