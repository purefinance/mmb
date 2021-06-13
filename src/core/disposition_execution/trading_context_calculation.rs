use rust_decimal_macros::dec;

use crate::core::disposition_execution::TradingContext;
use crate::core::explanation::Explanation;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::DateTime;
use crate::strategies::disposition_strategy::DispositionStrategy;

pub fn calculate_trading_context(
    strategy: &mut DispositionStrategy,
    local_snapshots_service: &LocalSnapshotsService,
    now: DateTime,
) -> Option<TradingContext> {
    // TODO check is balance manager initialized for next calculations

    let mut explanation = Explanation::default();
    explanation.add_reason(format!("Start time utc={}", now.to_rfc2822()));

    // TODO get from config
    let max_amount = dec!(1);

    // TODO check balance position

    strategy.calculate_trading_context(max_amount, now, local_snapshots_service, &mut explanation)
}
