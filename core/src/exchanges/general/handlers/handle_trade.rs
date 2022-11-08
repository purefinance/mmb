use mmb_domain::events::{ExchangeEvent, Trade, TradesEvent};
use mmb_domain::market::CurrencyPair;
use mmb_domain::market::MarketId;

use crate::exchanges::{general::exchange::Exchange, timeouts::timeout_manager};

impl Exchange {
    pub fn handle_trade(&self, currency_pair: CurrencyPair, trade: Trade) {
        if !self.exchange_client.get_settings().subscribe_to_market_data {
            return;
        }

        let mut trades_event = TradesEvent {
            exchange_account_id: self.exchange_account_id,
            currency_pair,
            trades: vec![trade],
            receipt_time: timeout_manager::now(),
        };

        let market_id = MarketId::new(self.exchange_account_id.exchange_id, currency_pair);

        self.last_trades_update_time
            .insert(market_id, trades_event.receipt_time);

        if cfg!(debug_assert) && !self.symbols.contains_key(&trades_event.currency_pair) {
            log::error!(
                "Unknown currency pair {} for trades on {}",
                trades_event.currency_pair,
                self.exchange_account_id
            );
        }

        if self.exchange_client.get_settings().request_trades {
            let should_add_event = if let Some(last_trade) = self.last_trades.get_mut(&market_id) {
                let trades = &mut trades_event.trades;
                match self.features.trade_option.supports_trade_incremented_id {
                    true => trades.retain(|t| t.trade_id.number() > last_trade.trade_id.number()),
                    false => trades.retain(|t| t.transaction_time > last_trade.transaction_time),
                };

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
            .send(ExchangeEvent::Trades(trades_event.clone()))
            .expect("Unable to send trades event. Probably receiver is already dropped");

        self.event_recorder
            .save(trades_event)
            .expect("Failure save trades_event");
    }
}
