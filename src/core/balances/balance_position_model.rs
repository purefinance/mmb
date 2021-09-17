use rust_decimal::Decimal;

pub(crate) struct BalancePositionModel {
    pub(crate) position: Decimal,
    pub(crate) limit: Option<Decimal>,
}
