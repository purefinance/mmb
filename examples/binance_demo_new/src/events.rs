use domain::market::{CurrencyPair, ExchangeId, MarketId};
use domain::order::pool::OrdersPool;
use domain::order::snapshot::{Amount, Price};
use domain::order::snapshot::{ClientOrderId, OrderSide, OrderStatus};
use domain::order_book::local_order_book_snapshot::LocalOrderBookSnapshot;
use itertools::Itertools;
use mmb_database::impl_event;
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
        .filter_map(|x| {
            x.fn_ref(|os| match os.props.status {
                OrderStatus::Created | OrderStatus::Canceling => Some(LiquidityOrder {
                    client_order_id: os.header.client_order_id.clone(),
                    side: os.header.side,
                    price: os.price(),
                    amount: os.amount(),
                    remaining_amount: os.amount() - os.filled_amount(),
                }),
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
