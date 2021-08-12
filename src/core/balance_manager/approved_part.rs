use crate::core::exchanges::common::Amount;
use crate::core::orders::order::ClientOrderId;
use crate::core::DateTime;

use rust_decimal::Decimal;

pub(crate) struct ApprovedPart {
    date_time: DateTime,
    client_order_id: ClientOrderId,
    /// Order amount in current CurrencyCode
    amount: Amount,
    is_canceled: bool,
    unreserved_amount: Decimal,
}

impl ApprovedPart {
    pub fn new(
        date_time: DateTime,
        client_order_id: ClientOrderId,
        amount: Amount,
        unreserved_amount: Decimal,
    ) -> Self {
        Self {
            date_time: date_time,
            client_order_id: client_order_id,
            amount: amount,
            is_canceled: false,
            unreserved_amount: unreserved_amount,
        }
    }
}
