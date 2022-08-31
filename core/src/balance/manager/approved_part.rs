use mmb_utils::DateTime;

use domain::order::snapshot::Amount;
use domain::order::snapshot::ClientOrderId;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
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
