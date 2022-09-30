use mmb_core::exchanges::traits::HandleOrderFilledCb;

pub struct Handlers {
    pub order_filled_callback: HandleOrderFilledCb,
}

impl Handlers {
    pub fn empty() -> Self {
        Self {
            order_filled_callback: Box::new(|_| {}),
        }
    }
}
