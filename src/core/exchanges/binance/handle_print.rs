use anyhow::{bail, Result};
use chrono::TimeZone;
use chrono::Utc;
use log::info;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;

use crate::core::exchanges::events::TickDirection;
use crate::core::exchanges::events::Trade;
use crate::core::exchanges::events::TradesEvent;
use crate::core::{
    exchanges::common::{Amount, CurrencyPair, Price},
    orders::order::OrderSide,
    DateTime,
};

use super::binance::Binance;

// TODO Probably filename have to be changed later

impl Binance {
    pub(crate) fn handle_print_inner(
        &self,
        currency_pair: &CurrencyPair,
        data: &Value,
    ) -> Result<()> {
        let trade_id: u64 = data["t"].to_string().parse()?;

        match self.last_trade_id.get_mut(currency_pair) {
            Some(mut trade_id_from_lasts) => {
                // FIXME add ISReducingMarketData field
                if *trade_id_from_lasts >= trade_id {
                    info!(
                        "Current last_trade_id for currency_pair {} is {} >= print_trade_id {}",
                        currency_pair, *trade_id_from_lasts, trade_id
                    );

                    return Ok(());
                }

                *trade_id_from_lasts = trade_id;

                let price = Decimal::from_str(&data["p"].to_string())?;
                let quantity = Decimal::from_str(&data["q"].to_string())?;
                let order_side = if data["m"] == true {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                let datetime: i64 = data["T"].to_string().parse()?;

                self.handle_print(
                    currency_pair,
                    trade_id.to_string(),
                    price,
                    quantity,
                    order_side,
                    Utc.timestamp_millis(datetime),
                );
            }
            None => bail!(
                "There are trade_id {} for given currency_pair {}",
                trade_id,
                currency_pair
            ),
        }

        todo!()
    }

    fn handle_print(
        &self,
        currency_pair: &CurrencyPair,
        trade_id: String,
        price: Price,
        quantity: Amount,
        side: OrderSide,
        transaction_time: DateTime,
    ) -> () {
        let trades = vec![Trade {
            trade_id,
            price,
            quantity,
            side,
            transaction_time,
            tick_direction: TickDirection::None,
        }];
        let trades_event = TradesEvent {
            exchange_account_id: self.settings.exchange_account_id.clone(),
            currency_pair: currency_pair.clone(),
            trades,
        };
    }
}
