use crate::channels::channel_type::ChannelType;
use crate::channels::receivers::ChannelReceivers;
use crate::channels::senders::ChannelSenders;
use dashmap::DashMap;
use tokio::sync::broadcast;

pub mod channel_type;
pub mod receivers;
pub mod senders;

const CHANNEL_CAPACITY: usize = 1024;

pub fn make_channels() -> (ChannelSenders, ChannelReceivers) {
    let channel_senders = DashMap::new();
    let channel_receivers = DashMap::new();

    for channel_type in ChannelType::get_all() {
        let (sender, _receiver) = broadcast::channel(CHANNEL_CAPACITY);

        channel_senders.insert(*channel_type, sender.clone());
        channel_receivers.insert(*channel_type, sender);
    }

    let channel_senders = ChannelSenders::new(channel_senders);
    let channel_receivers = ChannelReceivers::new(channel_receivers);

    (channel_senders, channel_receivers)
}
