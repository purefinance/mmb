use std::collections::HashMap;

use crate::core::misc::service_value_tree::ServiceValueTree;

use rust_decimal::Decimal;

type BalanceByExchangeId = HashMap<String, HashMap<String, Decimal>>;

struct VirtualBalanceHolder {
    balance_by_exchange_id: BalanceByExchangeId,
    balance_diff: ServiceValueTree,
}
