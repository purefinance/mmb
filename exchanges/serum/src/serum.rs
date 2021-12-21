use mmb_core::core::exchanges::common::ExchangeAccountId;
use mmb_core::core::exchanges::events::ExchangeEvent;
use mmb_core::core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::core::exchanges::traits::{ExchangeClientBuilder, ExchangeClientBuilderResult};
use mmb_core::core::lifecycle::application_manager::ApplicationManager;
use mmb_core::core::settings::ExchangeSettings;
use std::sync::Arc;

pub struct Serum {
    pub id: ExchangeAccountId,
    pub settings: ExchangeSettings,
}

impl Serum {
    pub fn new(id: ExchangeAccountId, settings: ExchangeSettings) -> Self {
        Self { id, settings }
    }
}

pub struct SerumBuilder;

impl ExchangeClientBuilder for SerumBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: tokio::sync::broadcast::Sender<ExchangeEvent>,
        application_manager: Arc<ApplicationManager>,
    ) -> ExchangeClientBuilderResult {
        todo!()
    }

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        todo!()
    }
}
