use anyhow::Result;
use chrono::Utc;

use crate::core::{
    exchanges::{
        common::{Amount, CurrencyPair, Price, TradePlace},
        events::{TickDirection, Trade, TradesEvent},
        general::exchange::Exchange,
    },
    orders::order::OrderSide,
    DateTime,
};

impl Exchange {
    pub fn handle_print(
        &self,
        currency_pair: &CurrencyPair,
        trade_id: String,
        price: Price,
        quantity: Amount,
        side: OrderSide,
        transaction_time: DateTime,
    ) -> Result<()> {
        let trades = vec![Trade {
            trade_id,
            price,
            quantity,
            side,
            transaction_time,
            tick_direction: TickDirection::None,
        }];
        let trades_event = TradesEvent {
            exchange_account_id: self.exchange_account_id.clone(),
            currency_pair: currency_pair.clone(),
            trades,
            datetime: Utc::now(),
        };

        let trade_place = TradePlace::new(
            self.exchange_account_id.exchange_id.clone(),
            currency_pair.clone(),
        );

        self.last_trades_update_time
            .insert(trade_place, trades_event.datetime);

        Ok(())
    }
}
