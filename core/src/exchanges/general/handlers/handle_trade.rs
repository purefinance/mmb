use domain::events::{ExchangeEvent, TickDirection, Trade, TradeId, TradesEvent};
use domain::market::CurrencyPair;
use domain::market::MarketId;
use domain::order::snapshot::OrderSide;
use domain::order::snapshot::{Amount, Price};
use itertools::Itertools;
use mmb_utils::DateTime;

use crate::exchanges::{general::exchange::Exchange, timeouts::timeout_manager};

impl Exchange {
    pub fn handle_trade(
        &self,
        currency_pair: CurrencyPair,
        trade_id: TradeId,
        price: Price,
        quantity: Amount,
        side: OrderSide,
        transaction_time: DateTime,
    ) {
        let trades = vec![Trade {
            trade_id,
            price,
            quantity,
            side,
            transaction_time,
            tick_direction: TickDirection::None,
        }];
        let mut trades_event = TradesEvent {
            exchange_account_id: self.exchange_account_id,
            currency_pair,
            trades,
            receipt_time: timeout_manager::now(),
        };

        let market_id = MarketId::new(self.exchange_account_id.exchange_id, currency_pair);

        self.last_trades_update_time
            .insert(market_id, trades_event.receipt_time);

        if self.exchange_client.get_settings().subscribe_to_market_data {
            return;
        }

        if self.symbols.contains_key(&trades_event.currency_pair)
            && !self
                .features
                .trade_option
                .notification_on_each_currency_pair
        {
            log::trace!(
                "Unknown currency pair {} for trades on {}",
                trades_event.currency_pair,
                self.exchange_account_id
            );
        }

        if self.exchange_client.get_settings().request_trades {
            let should_add_event = if let Some(last_trade) = self.last_trades.get_mut(&market_id) {
                let trade_items = trades_event
                    .trades
                    .into_iter()
                    .filter(
                        |item| match self.features.trade_option.supports_trade_incremented_id {
                            true => item.trade_id.get_number() > last_trade.trade_id.get_number(),
                            false => item.transaction_time > last_trade.transaction_time,
                        },
                    )
                    .collect_vec();

                trades_event.trades = trade_items;

                true
            } else {
                false
            };

            match trades_event.trades.first() {
                Some(trade) => self.last_trades.insert(market_id, trade.clone()),
                None => return,
            };

            if !should_add_event {
                return;
            }
        }

        self.events_channel
            .send(ExchangeEvent::Trades(trades_event))
            .expect("Unable to send trades event. Probably receiver is already dropped");

        // TODO DataRecorder.save(trades) if needed;
    }
}
