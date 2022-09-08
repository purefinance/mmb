use crate::disposition_execution::TradingContext;
use crate::explanation::Explanation;
use crate::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::strategies::disposition_strategy::DispositionStrategy;
use mmb_domain::events::ExchangeEvent;
use mmb_utils::DateTime;

pub fn calculate_trading_context(
    event: &ExchangeEvent,
    strategy: &mut dyn DispositionStrategy,
    local_snapshots_service: &LocalSnapshotsService,
    now: DateTime,
) -> Option<TradingContext> {
    // TODO check is balance manager initialized for next calculations

    let mut explanation = Explanation::default();
    explanation.add_reason(format!("Start time utc={}", now.to_rfc2822()));

    // TODO check balance position

    strategy.calculate_trading_context(event, now, local_snapshots_service, &mut explanation)
}
