use rust_decimal::Decimal;

pub trait DecimalInverseSign {
    fn inverse_sign(&mut self);
}

impl DecimalInverseSign for Decimal {
    fn inverse_sign(&mut self) {
        self.set_sign_positive(!self.is_sign_positive());
    }
}
