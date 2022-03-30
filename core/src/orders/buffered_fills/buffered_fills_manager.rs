use mmb_utils::infrastructure::WithExpect;
use std::collections::HashMap;

use crate::{
    exchanges::{common::ExchangeAccountId, general::handlers::handle_order_filled::FillEventData},
    orders::order::ExchangeOrderId,
};

use super::buffered_fill::BufferedFill;

#[derive(Default)]
pub struct BufferedFillsManager {
    buffered_fills: HashMap<ExchangeOrderId, Vec<BufferedFill>>,
}

impl BufferedFillsManager {
    pub fn add_fill(&mut self, exchange_account_id: ExchangeAccountId, event_date: FillEventData) {
        //likely we got a fill notification before an order creation notification
        let buffered_fill = BufferedFill::new(
            exchange_account_id,
            event_date.trade_id.expect("trade_id is None"),
            event_date.exchange_order_id.clone(),
            event_date.fill_price,
            event_date.fill_amount,
            event_date.is_diff,
            event_date.total_filled_amount,
            event_date.order_role,
            event_date
                .commission_currency_code
                .expect("commission_currency_code is None"),
            event_date.commission_rate,
            event_date.commission_amount,
            event_date.order_side,
            event_date.fill_type,
            event_date
                .trade_currency_pair
                .expect("trade_currency_pair is None"),
            event_date.fill_date,
            event_date.source_type,
        );

        let buffered_fill_vec = self
            .buffered_fills
            .entry(event_date.exchange_order_id.clone())
            .or_default();

        buffered_fill_vec.push(buffered_fill);

        log::trace!(
            "Buffered a fill for an order which is not in the system {:?}",
            (
                exchange_account_id,
                event_date.exchange_order_id,
                event_date.fill_price,
                event_date.fill_amount,
                event_date.total_filled_amount,
                event_date.order_role,
                event_date.commission_currency_code,
                event_date.commission_amount
            )
        );
    }

    pub fn get_fills(&self, exchange_order_id: &ExchangeOrderId) -> Option<&Vec<BufferedFill>> {
        self.buffered_fills.get(exchange_order_id)
    }

    pub fn get_fills_expected(&self, exchange_order_id: &ExchangeOrderId) -> &Vec<BufferedFill> {
        self.get_fills(exchange_order_id)
            .with_expect(|| format!("failed to get buffered fills for {}", exchange_order_id))
    }

    pub fn remove_fills(&mut self, exchange_order_id: &ExchangeOrderId) {
        self.buffered_fills.remove(exchange_order_id);
    }
}
