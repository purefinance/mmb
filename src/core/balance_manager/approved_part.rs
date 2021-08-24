use crate::core::exchanges::common::Amount;
use crate::core::orders::order::ClientOrderId;
use crate::core::DateTime;

use rust_decimal::Decimal;

#[derive(Clone, Debug)]
pub(crate) struct ApprovedPart {
    date_time: DateTime,
    client_order_id: ClientOrderId,
    /// Order amount in current CurrencyCode
    pub(crate) amount: Amount,
    pub(crate) is_canceled: bool,
    pub(crate) unreserved_amount: Decimal,
}

impl ApprovedPart {
    pub fn new(date_time: DateTime, client_order_id: ClientOrderId, amount: Amount) -> Self {
        Self {
            date_time,
            client_order_id,
            amount,
            is_canceled: false,
            unreserved_amount: amount,
        }
    }
}
