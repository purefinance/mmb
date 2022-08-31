use domain::order::snapshot::ClientOrderFillId;
use serde::Serialize;

use mmb_utils::DateTime;
use rust_decimal::Decimal;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PositionChange {
    pub(crate) client_order_fill_id: ClientOrderFillId,
    pub(crate) change_time: DateTime,
    pub(crate) portion: Decimal,
}

impl PositionChange {
    pub fn new(
        client_order_fill_id: ClientOrderFillId,
        change_time: DateTime,
        portion: Decimal,
    ) -> Self {
        Self {
            client_order_fill_id,
            change_time,
            portion,
        }
    }
}
