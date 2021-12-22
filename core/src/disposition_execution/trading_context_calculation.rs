use mmb_utils::DateTime;

use crate::explanation::Explanation;
use crate::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::strategies::disposition_strategy::DispositionStrategy;
use crate::{disposition_execution::TradingContext, exchanges::common::Amount};

pub fn calculate_trading_context(
    max_amount: Amount,
    strategy: &mut dyn DispositionStrategy,
    local_snapshots_service: &LocalSnapshotsService,
    now: DateTime,
) -> Option<TradingContext> {
    // TODO check is balance manager initialized for next calculations

    let mut explanation = Explanation::default();
    explanation.add_reason(format!("Start time utc={}", now.to_rfc2822()));

    // TODO check balance position

    strategy.calculate_trading_context(max_amount, now, local_snapshots_service, &mut explanation)
}
