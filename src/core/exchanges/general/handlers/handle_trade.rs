use anyhow::{Context, Result};
use itertools::Itertools;
use log::trace;

use crate::core::{
    exchanges::{
        common::{Amount, CurrencyPair, Price, TradePlace},
        events::{ExchangeEvent, TickDirection, Trade, TradesEvent},
        general::exchange::Exchange,
        timeouts::timeout_manager,
    },
    orders::order::OrderSide,
    DateTime,
};

impl Exchange {
    pub fn handle_trade(
        &self,
        currency_pair: &CurrencyPair,
        trade_id: u64,
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
        let mut trades_event = TradesEvent {
            exchange_account_id: self.exchange_account_id.clone(),
            currency_pair: currency_pair.clone(),
            trades,
            receipt_time: timeout_manager::now(),
        };

        let trade_place = TradePlace::new(
            self.exchange_account_id.exchange_id.clone(),
            currency_pair.clone(),
        );

        self.last_trades_update_time
            .insert(trade_place.clone(), trades_event.receipt_time);

        if self.exchange_client.get_settings().subscribe_to_market_data {
            return Ok(());
        }

        if self.symbols.contains_key(&trades_event.currency_pair)
            && !self
                .features
                .trade_option
                .notification_on_each_currency_pair
        {
            trace!(
                "Unknown currency pair {} for trades on {}",
                trades_event.currency_pair,
                self.exchange_account_id
            );
        }

        let mut trade_items = Vec::new();
        if self.exchange_client.get_settings().request_trades {
            let mut should_add_event = false;

            if let Some(last_trade) = self.last_trades.get_mut(&trade_place) {
                // TODO use drain_filter here when it will be stabilized
                trade_items = if self.features.trade_option.supports_trade_incremented_id {
                    trades_event
                        .trades
                        .into_iter()
                        .filter(|item| item.trade_id > last_trade.trade_id)
                        .collect_vec()
                } else {
                    trades_event
                        .trades
                        .into_iter()
                        .filter(|item| item.transaction_time > last_trade.transaction_time)
                        .collect_vec()
                };

                should_add_event = true;
            };

            match trade_items.first() {
                Some(trade) => {
                    self.last_trades.insert(trade_place, trade.clone());
                    trades_event.trades = trade_items;

                    if !should_add_event {
                        return Ok(());
                    }
                }
                None => return Ok(()),
            }
        }

        self.events_channel
            .send(ExchangeEvent::Trades(trades_event))
            .context("Unable to send trades event. Probably receiver is already dropped")?;

        // TODO DataRecorder.save(trades) if needed;

        Ok(())
    }
}
