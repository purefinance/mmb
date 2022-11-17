use anyhow::Context;
use itertools::Itertools;
use mmb_core::lifecycle::trading_engine::EngineContext;
use mmb_core::order_book::local_snapshot_service::LocalSnapshotsService;
use mmb_database::impl_event;
use mmb_domain::market::{CurrencyPair, ExchangeId, MarketAccountId, MarketId};
use mmb_domain::order::pool::OrdersPool;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_domain::order::snapshot::{ClientOrderId, OrderSide, OrderStatus};
use mmb_domain::order_book::local_order_book_snapshot::LocalOrderBookSnapshot;
use mmb_utils::infrastructure::WithExpect;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiquidityOrder {
    client_order_id: ClientOrderId,
    price: Price,
    amount: Amount,
    remaining_amount: Amount,
    side: OrderSide,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PriceLevel {
    price: Price,
    amount: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiquiditySnapshot {
    asks: Vec<PriceLevel>,
    bids: Vec<PriceLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityOrderBook {
    exchange_id: ExchangeId,
    currency_pair: CurrencyPair,
    snapshot: LiquiditySnapshot,
    orders: Vec<LiquidityOrder>,
}

impl_event!(LiquidityOrderBook, "liquidity_order_books");

pub fn create_liquidity_order_book_snapshot(
    order_book_snapshot: &LocalOrderBookSnapshot,
    market_id: MarketId,
    orders_pool: &Arc<OrdersPool>,
) -> LiquidityOrderBook {
    const PRICE_LEVELS_COUNT: usize = 20;

    let orders = orders_pool
        .not_finished
        .iter()
        .filter_map(|pair_ref| {
            let header = pair_ref.header();
            pair_ref.fn_ref(|x| match (x.status(), header.source_price) {
                // save for visualization non-market orders
                (OrderStatus::Created | OrderStatus::Canceling, Some(price)) => {
                    Some(LiquidityOrder {
                        client_order_id: header.client_order_id.clone(),
                        side: header.side,
                        price,
                        amount: header.amount,
                        remaining_amount: header.amount - x.filled_amount(),
                    })
                }
                _ => None,
            })
        })
        .collect_vec();

    LiquidityOrderBook {
        exchange_id: market_id.exchange_id,
        currency_pair: market_id.currency_pair,
        snapshot: LiquiditySnapshot {
            asks: order_book_snapshot
                .get_asks_price_levels()
                .take(PRICE_LEVELS_COUNT)
                .map(|(&price, &amount)| PriceLevel { price, amount })
                .collect(),
            bids: order_book_snapshot
                .get_bids_price_levels()
                .take(PRICE_LEVELS_COUNT)
                .map(|(&price, &amount)| PriceLevel { price, amount })
                .collect(),
        },
        orders,
    }
}

pub fn save_liquidity_order_book_if_can(
    ctx: &EngineContext,
    snapshots_service: &mut LocalSnapshotsService,
    market_account_id: Option<MarketAccountId>,
) -> anyhow::Result<()> {
    if let Some(market_account_id) = market_account_id {
        let market_id = market_account_id.market_id();
        if let Some(snapshot) = snapshots_service.get_snapshot(market_id) {
            let exchange_account_id = market_account_id.exchange_account_id;
            let liquidity_order_book = create_liquidity_order_book_snapshot(
                snapshot,
                market_id,
                &ctx.exchanges.get(&exchange_account_id)
                    .with_expect(|| format!("exchange {exchange_account_id} should exists in `Save order book` events loop"))
                    .orders,
            );
            ctx.event_recorder
                .save(liquidity_order_book)
                .context("failed saving liquidity_order_book")?;
        }
    }

    Ok(())
}
