use crate::lifecycle::app_lifetime_manager::AppLifetimeManager;
use anyhow::{anyhow, Result};
use core::result::Result::{Err, Ok};
use domain::events::ExchangeEvent;
use domain::market::ExchangeAccountId;
use std::sync::Arc;
use tokio::sync::broadcast;

pub fn send_event(
    events_channel: &broadcast::Sender<ExchangeEvent>,
    lifetime_manager: Arc<AppLifetimeManager>,
    id: ExchangeAccountId,
    event: ExchangeEvent,
) -> Result<()> {
    match events_channel.send(event) {
        Ok(_) => Ok(()),
        Err(error) => {
            let msg = format!("Unable to send exchange event in {}: {}", id, error);
            log::error!("{}", msg);
            lifetime_manager.spawn_graceful_shutdown(&msg);
            Err(anyhow!(msg))
        }
    }
}
