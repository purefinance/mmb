use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::exchange::Exchange;
use crate::core::settings::CoreSettings;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::oneshot::Sender;

pub trait Service {
    fn name(&self) -> &str;

    fn graceful_shutdown(&self, service_finished: Sender<()>) {
        let _ = service_finished.send(());
    }
}

pub struct BotContext {
    pub app_settings: CoreSettings,
    pub exchanges: DashMap<ExchangeAccountId, Exchange>,
}

impl BotContext {
    pub fn new(app_settings: CoreSettings) -> Arc<Self> {
        Arc::new(BotContext {
            app_settings,
            exchanges: Default::default(),
        })
    }
}
