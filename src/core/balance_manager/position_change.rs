use crate::core::orders::order::ClientOrderId;
use crate::core::DateTime;

use rust_decimal::Decimal;

#[derive(Clone, Debug)]
pub(crate) struct PositionChange {
    pub(crate) client_order_fill_id: ClientOrderId,
    pub(crate) date_time: DateTime,
    pub(crate) portion: Decimal,
}

impl PositionChange {
    pub fn new(client_order_fill_id: ClientOrderId, date_time: DateTime, portion: Decimal) -> Self {
        Self {
            client_order_fill_id: client_order_fill_id,
            date_time: date_time,
            portion: portion,
        }
    }
}
