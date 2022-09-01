use mmb_domain::order::snapshot::Amount;
use rust_decimal::Decimal;

use mmb_domain::market::CurrencyCode;

pub(crate) struct BalanceReservationPreset {
    pub(crate) reservation_currency_code: CurrencyCode,
    pub(crate) amount_in_reservation_currency_code: Amount,
    pub(crate) taken_free_amount_in_amount_currency_code: Amount,
    pub(crate) cost_in_reservation_currency_code: Decimal,
    pub(crate) cost_in_amount_currency_code: Decimal,
}

impl BalanceReservationPreset {
    pub fn new(
        reservation_currency_code: CurrencyCode,
        amount_in_reservation_currency_code: Amount,
        taken_free_amount_in_amount_currency_code: Amount,
        cost_in_reservation_currency_code: Decimal,
        cost_in_amount_currency_code: Decimal,
    ) -> Self {
        Self {
            reservation_currency_code,
            amount_in_reservation_currency_code,
            taken_free_amount_in_amount_currency_code,
            cost_in_reservation_currency_code,
            cost_in_amount_currency_code,
        }
    }
}
