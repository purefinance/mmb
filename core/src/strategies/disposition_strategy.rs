use std::sync::Arc;

use anyhow::Result;
use mmb_utils::DateTime;

use crate::disposition_execution::{PriceSlot, TradingContext};
use crate::explanation::Explanation;
use crate::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use mmb_domain::market::ExchangeAccountId;
use mmb_domain::order::snapshot::OrderSnapshot;
use mmb_utils::cancellation_token::CancellationToken;

pub trait DispositionStrategy: Send + Sync + 'static {
    fn calculate_trading_context(
        &mut self,
        now: DateTime,
        local_snapshots_service: &LocalSnapshotsService,
        explanation: &mut Explanation,
    ) -> Option<TradingContext>;

    fn handle_order_fill(
        &self,
        cloned_order: &Arc<OrderSnapshot>,
        price_slot: &PriceSlot,
        target_eai: ExchangeAccountId,
        cancellation_token: CancellationToken,
    ) -> Result<()>;

    fn configuration_descriptor(&self) -> ConfigurationDescriptor;
}
