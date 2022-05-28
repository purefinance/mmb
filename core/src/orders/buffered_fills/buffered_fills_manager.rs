use mmb_utils::infrastructure::WithExpect;
use std::collections::HashMap;

use crate::exchanges::general::handlers::handle_order_filled::FillAmount;
use crate::{
    exchanges::{common::ExchangeAccountId, general::handlers::handle_order_filled::FillEvent},
    orders::order::ExchangeOrderId,
};

use super::buffered_fill::BufferedFill;

#[derive(Default)]
pub struct BufferedFillsManager {
    buffered_fills: HashMap<ExchangeOrderId, Vec<BufferedFill>>,
}

impl BufferedFillsManager {
    pub fn add_fill(&mut self, exchange_account_id: ExchangeAccountId, fill_event: FillEvent) {
        let (is_diff, fill_amount, total_filled_amount) = match fill_event.fill_amount {
            FillAmount::Incremental {
                fill_amount,
                total_filled_amount: total_fill_amount,
            } => (true, fill_amount, total_fill_amount),
            FillAmount::Total {
                total_filled_amount: total_fill_amount,
            } => (false, total_fill_amount, None),
        };

        //likely we got a fill notification before an order creation notification
        let buffered_fill = BufferedFill::new(
            exchange_account_id,
            fill_event.trade_id.expect("trade_id is None"),
            fill_event.exchange_order_id.clone(),
            fill_event.fill_price,
            fill_amount,
            is_diff,
            total_filled_amount,
            fill_event.order_role,
            fill_event
                .commission_currency_code
                .expect("commission_currency_code is None"),
            fill_event.commission_rate,
            fill_event.commission_amount,
            fill_event.order_side,
            fill_event.fill_type,
            fill_event
                .trade_currency_pair
                .expect("trade_currency_pair is None"),
            fill_event.fill_date,
            fill_event.source_type,
        );

        let buffered_fill_vec = self
            .buffered_fills
            .entry(fill_event.exchange_order_id.clone())
            .or_default();

        buffered_fill_vec.push(buffered_fill);

        log::trace!(
            "Buffered a fill for an order which is not in the system {:?}",
            (
                exchange_account_id,
                fill_event.exchange_order_id,
                fill_event.fill_price,
                fill_event.fill_amount,
                fill_event.order_role,
                fill_event.commission_currency_code,
                fill_event.commission_amount
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
