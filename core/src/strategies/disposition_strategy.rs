use std::sync::Arc;

use anyhow::Result;
use rust_decimal::Decimal;

use crate::core::disposition_execution::{PriceSlot, TradingContext};
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::explanation::Explanation;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::orders::order::OrderSnapshot;
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::core::DateTime;
use mmb_utils::cancellation_token::CancellationToken;

pub trait DispositionStrategy: Send + Sync + 'static {
    fn calculate_trading_context(
        &mut self,
        max_amount: Decimal,
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
