use std::panic::UnwindSafe;

use super::handle_trigger_trait::TriggerHandler;
use anyhow::Result;

pub struct LessOrEqualsRequestsCountTrigger {
    available_requests_count_threshold: usize,
    handler: Box<dyn Fn() -> Result<()> + Send + UnwindSafe>,
    last_is_less: bool,
}

impl LessOrEqualsRequestsCountTrigger {
    pub fn new(
        available_requests_count_threshold: usize,
        handler: Box<dyn Fn() -> Result<()> + Send + UnwindSafe>,
    ) -> Self {
        Self {
            available_requests_count_threshold,
            handler,
            last_is_less: false,
        }
    }
}

impl TriggerHandler for LessOrEqualsRequestsCountTrigger {
    fn handle(&mut self, available_requests_count: usize) -> Result<()> {
        let is_less = available_requests_count <= self.available_requests_count_threshold;
        let last_is_less = self.last_is_less;
        self.last_is_less = is_less;

        if is_less && !last_is_less {
            (self.handler)()?;
        }

        Ok(())
    }
}
