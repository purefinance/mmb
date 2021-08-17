use crate::core::DateTime;

use rust_decimal::Decimal;

#[derive(Clone)]
pub(crate) struct PositionChange {
    client_order_fill_id: String,
    date_time: DateTime,
    portion: Decimal,
}

impl PositionChange {
    pub fn new(client_order_fill_id: String, date_time: DateTime, portion: Decimal) -> Self {
        Self {
            client_order_fill_id: client_order_fill_id,
            date_time: date_time,
            portion: portion,
        }
    }
}
