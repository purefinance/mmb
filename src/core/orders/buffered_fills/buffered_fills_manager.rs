use std::collections::HashMap;

use crate::core::{
    exchanges::{common::ExchangeAccountId, general::handlers::handle_order_filled::FillEventData},
    infrastructure::WithExpect,
    orders::order::{ExchangeOrderId, OrderRole},
    DateTime,
};

use super::buffered_fill::BufferedFill;

pub struct BufferedFillsManager {
    buffered_fills_by_exchange_id:
        HashMap<ExchangeAccountId, HashMap<ExchangeOrderId, Vec<BufferedFill>>>,
}

impl BufferedFillsManager {
    pub fn new(exchange_account_id: Vec<ExchangeAccountId>) -> Self {
        Self {
            buffered_fills_by_exchange_id: exchange_account_id
                .into_iter()
                .map(|eai| (eai, HashMap::<ExchangeOrderId, Vec<BufferedFill>>::new()))
                .collect(),
        }
    }

    pub fn add_fill(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        event_date: FillEventData,
        fill_date: Option<DateTime>,
    ) {
        let is_maker = event_date.order_role.map(|x| x == OrderRole::Maker);
        //likely we got a fill notification before an order creation notification
        let buffered_fill = BufferedFill::new(
            exchange_account_id,
            event_date.trade_id.expect("trade_id is None"),
            event_date.exchange_order_id.clone(),
            event_date.fill_price,
            event_date.fill_amount,
            event_date.is_diff,
            event_date.total_filled_amount,
            is_maker,
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
            fill_date,
            event_date.source_type,
        );

        if self
            .buffered_fills_by_exchange_id
            .contains_key(&exchange_account_id)
        {
            panic!("Exchange {} is unknown", exchange_account_id)
        }

        let buffered_fills_by_exchange_order_id = self
            .buffered_fills_by_exchange_id
            .entry(exchange_account_id)
            .or_insert_with(|| panic!("Exchange {} is unknown", exchange_account_id));

        let buffered_fill_vec = buffered_fills_by_exchange_order_id
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
                is_maker,
                event_date.commission_currency_code,
                event_date.commission_amount
            )
        );
    }

    pub fn get_all(
        &self,
        exchange_account_id: ExchangeAccountId,
    ) -> Option<HashMap<ExchangeOrderId, Vec<BufferedFill>>> {
        self.buffered_fills_by_exchange_id
            .get(&exchange_account_id)
            .cloned()
    }

    pub fn get_all_expected(
        &self,
        exchange_account_id: ExchangeAccountId,
    ) -> HashMap<ExchangeOrderId, Vec<BufferedFill>> {
        self.get_all(exchange_account_id).with_expect(|| {
            format!(
                "failed to get buffered fills by exchange_order_id for {}",
                exchange_account_id
            )
        })
    }

    pub fn get_fills(
        &self,
        exchange_account_id: ExchangeAccountId,
        exchange_order_id: &ExchangeOrderId,
    ) -> Option<Vec<BufferedFill>> {
        self.get_all(exchange_account_id)?
            .get(exchange_order_id)
            .cloned()
    }

    pub fn get_fills_expected(
        &self,
        exchange_account_id: ExchangeAccountId,
        exchange_order_id: &ExchangeOrderId,
    ) -> Vec<BufferedFill> {
        self.get_fills(exchange_account_id, exchange_order_id)
            .with_expect(|| {
                format!(
                    "failed to get buffered fills for {} {}",
                    exchange_account_id, exchange_order_id
                )
            })
    }

    pub fn remove_fills(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        exchange_order_id: &ExchangeOrderId,
    ) {
        self.buffered_fills_by_exchange_id
            .get_mut(&exchange_account_id)
            .with_expect(|| {
                format!(
                    "failed to get buffered fills by exchange_order_id for {}",
                    exchange_account_id
                )
            })
            .remove(exchange_order_id);
    }
}
