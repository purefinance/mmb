use rust_decimal_macros::dec;

use crate::core::exchanges::common::{Price, TradePlace};

use super::{
    local_order_book_snapshot::LocalOrderBookSnapshot,
    local_snapshot_service::LocalSnapshotsService,
};

impl LocalSnapshotsService {
    pub fn calculate_middle_price(&self, trade_place: &TradePlace) -> Option<Price> {
        match self.get_snapshot(trade_place) {
            Some(snapshot) => {
                return snapshot.calculate_middle_price(trade_place);
            }
            None => {
                log::warn!(
                    "Can't get snapshot {:?} in LocalSnapshotsService::calculate_middle_price()",
                    trade_place
                );
                None
            }
        }
    }
}
impl LocalOrderBookSnapshot {
    pub fn calculate_middle_price(&self, trade_place: &TradePlace) -> Option<Price> {
        let prices = self.calculate_price();
        let top_ask = match prices.top_ask {
            Some(top_ask) => top_ask,
            None => {
                log::warn!(
                "Can't get top ask price in {:?} in LocalOrderBookSnapshot::calculate_middle_price() {:?}",
                trade_place,
                self
            );
                return None;
            }
        };

        let top_bid = match prices.top_ask {
            Some(top_bid) => top_bid,
            None => {
                log::warn!(
                "Can't get top bid price in {:?} in LocalOrderBookSnapshot::calculate_middle_price() {:?}",
                trade_place,
                self
            );
                return None;
            }
        };

        Some((top_ask + top_bid) / dec!(2))
    }
}
