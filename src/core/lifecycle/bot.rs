use crate::core::exchanges::actor::ExchangeActor;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::settings::CoreSettings;
use actix::Addr;
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
    pub exchanges: DashMap<ExchangeAccountId, Addr<ExchangeActor>>,
}

impl BotContext {
    pub fn new(app_settings: CoreSettings) -> Arc<Self> {
        Arc::new(BotContext {
            app_settings,
            exchanges: Default::default(),
        })
    }
}
