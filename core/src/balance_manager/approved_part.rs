use mmb_utils::DateTime;

use crate::exchanges::common::Amount;
use crate::orders::order::ClientOrderId;

#[derive(Clone, Debug)]
pub struct ApprovedPart {
    _approve_time: DateTime,
    _client_order_id: ClientOrderId,
    /// Order amount in current CurrencyCode
    pub(crate) amount: Amount,
    pub(crate) is_canceled: bool,
    pub(crate) unreserved_amount: Amount,
}

impl ApprovedPart {
    pub fn new(_approve_time: DateTime, _client_order_id: ClientOrderId, amount: Amount) -> Self {
        Self {
            _approve_time,
            _client_order_id,
            amount,
            is_canceled: false,
            unreserved_amount: amount,
        }
    }
}
