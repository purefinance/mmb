use std::collections::HashMap;

use crate::core::{
    exchanges::common::ExchangeAccountId, infrastructure::WithExpect,
    orders::order::ExchangeOrderId,
};

pub struct BufferedCanceledOrdersManager {
    buffered_orders_by_exchange_id:
        HashMap<ExchangeAccountId, HashMap<ExchangeOrderId, ExchangeAccountId>>,
}

impl BufferedCanceledOrdersManager {
    pub fn new(exchange_account_ids: Vec<ExchangeAccountId>) -> Self {
        Self {
            buffered_orders_by_exchange_id: exchange_account_ids
                .into_iter()
                .map(|eai| (eai, HashMap::<ExchangeOrderId, ExchangeAccountId>::new()))
                .collect(),
        }
    }

    pub fn add_order(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        exchange_order_id: ExchangeOrderId,
    ) {
        let buffered_orders = self
            .buffered_orders_by_exchange_id
            .entry(exchange_account_id)
            .or_insert_with(|| panic!("Exchange {} is unknown", exchange_account_id));

        buffered_orders
            .entry(exchange_order_id)
            .and_modify(|v| *v = exchange_account_id)
            .or_insert(exchange_account_id);
    }

    pub fn is_order_buffered(
        &self,
        exchange_account_id: ExchangeAccountId,
        exchange_order_id: &ExchangeOrderId,
    ) -> bool {
        if let Some(buffered_orders) = self.get_all(exchange_account_id) {
            return buffered_orders.contains_key(&exchange_order_id);
        }
        false
    }

    pub fn remove_order(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        exchange_order_id: &ExchangeOrderId,
    ) {
        self.buffered_orders_by_exchange_id
            .get_mut(&exchange_account_id)
            .with_expect(|| format!("Failed to get buffered orders for {}", exchange_account_id))
            .remove(exchange_order_id);
    }

    pub fn get_all(
        &self,
        exchange_account_id: ExchangeAccountId,
    ) -> Option<HashMap<ExchangeOrderId, ExchangeAccountId>> {
        self.buffered_orders_by_exchange_id
            .get(&exchange_account_id)
            .cloned()
    }

    pub fn get_all_expected(
        &self,
        exchange_account_id: ExchangeAccountId,
    ) -> HashMap<ExchangeOrderId, ExchangeAccountId> {
        self.get_all(exchange_account_id)
            .with_expect(|| format!("Failed to get buffered orders for {}", exchange_account_id))
    }
}
