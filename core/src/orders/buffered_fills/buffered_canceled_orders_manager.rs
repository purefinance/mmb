use std::collections::HashMap;

use crate::{exchanges::common::ExchangeAccountId, orders::order::ExchangeOrderId};

#[derive(Default)]
pub struct BufferedCanceledOrdersManager {
    buffered_orders_by_exchange_order_id: HashMap<ExchangeOrderId, ExchangeAccountId>,
}

impl BufferedCanceledOrdersManager {
    pub fn add_order(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        exchange_order_id: ExchangeOrderId,
    ) {
        let _ = self
            .buffered_orders_by_exchange_order_id
            .insert(exchange_order_id, exchange_account_id);
    }

    pub fn is_order_buffered(&self, exchange_order_id: &ExchangeOrderId) -> bool {
        self.buffered_orders_by_exchange_order_id
            .contains_key(exchange_order_id)
    }

    pub fn remove_order(&mut self, exchange_order_id: &ExchangeOrderId) {
        self.buffered_orders_by_exchange_order_id
            .remove(exchange_order_id);
    }
}
