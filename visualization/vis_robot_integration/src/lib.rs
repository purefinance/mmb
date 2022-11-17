#![deny(
    non_ascii_idents,
    non_shorthand_field_patterns,
    no_mangle_generic_items,
    overflowing_literals,
    path_statements,
    unused_allocation,
    unused_comparisons,
    unused_parens,
    while_true,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_must_use,
    clippy::unwrap_used
)]

mod liquidity_order_book;
mod transaction;

use crate::transaction::{
    transaction_service, TransactionSnapshot, TransactionStatus, TransactionTrade,
};
use anyhow::{Context, Error, Result};
use function_name::named;
use mmb_core::lifecycle::trading_engine::EngineContext;
use mmb_core::order_book::local_snapshot_service::LocalSnapshotsService;
use mmb_domain::events::ExchangeEvent;
use mmb_domain::order::event::OrderEventType;
use mmb_domain::order::snapshot::OrderSnapshot;
use std::sync::Arc;

#[named]
pub async fn start_visualization_data_saving(
    ctx: Arc<EngineContext>,
    strategy_name: &'static str,
) -> Result<(), Error> {
    let mut snapshots_service = LocalSnapshotsService::default();
    let mut events_rx = ctx.get_events_channel();

    let stop_token = ctx.lifetime_manager.stop_token();
    while !stop_token.is_cancellation_requested() {
        let event_res = events_rx.recv().await;
        match event_res {
            Err(err) => {
                log::error!("Error occurred: {err:?}");

                let _ = ctx
                    .lifetime_manager
                    .spawn_graceful_shutdown(concat!("Error in ", function_name!()));
            }
            Ok(event) => {
                let market_account_id = match event {
                    ExchangeEvent::OrderBookEvent(ref ob_event) => {
                        snapshots_service.update(ob_event)
                    }
                    ExchangeEvent::OrderEvent(order_event) => match order_event.event_type {
                        OrderEventType::CreateOrderSucceeded
                        | OrderEventType::OrderCompleted { .. }
                        | OrderEventType::CancelOrderSucceeded => {
                            Some(order_event.order.header().market_account_id())
                        }
                        OrderEventType::OrderFilled { cloned_order } => {
                            save_transaction(
                                &ctx,
                                &cloned_order,
                                TransactionStatus::Finished,
                                strategy_name.to_string(),
                            )
                            .context("in start_visualization_data_saving")?;

                            Some(cloned_order.market_account_id())
                        }
                        _ => None,
                    },
                    _ => None,
                };

                liquidity_order_book::save_liquidity_order_book_if_can(
                    &ctx,
                    &mut snapshots_service,
                    market_account_id,
                )
                .context("in start_visualization_data_saving")?;
            }
        }
    }

    Ok(())
}

fn save_transaction(
    ctx: &EngineContext,
    order_snapshot: &OrderSnapshot,
    status: TransactionStatus,
    strategy_name: String,
) -> Result<()> {
    let mut transaction = TransactionSnapshot::new(
        order_snapshot.market_id(),
        order_snapshot.side(),
        order_snapshot.header.source_price,
        order_snapshot.amount(),
        status,
        strategy_name,
    );

    let exchange_order_id = order_snapshot
        .props
        .exchange_order_id
        .as_ref()
        .expect("`exchange_order_id` must be set before saving transaction")
        .clone();

    let fill = order_snapshot
        .fills
        .fills
        .last()
        .expect("must be existed at least 1 fill on saving transaction");

    transaction.trades.push(TransactionTrade {
        exchange_order_id,
        exchange_id: order_snapshot.header.exchange_account_id.exchange_id,
        price: Some(fill.price()),
        amount: fill.amount(),
        side: fill.side(),
    });

    transaction_service::save(&mut transaction, status, &ctx.event_recorder)
}
