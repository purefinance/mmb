use anyhow::Result;

use super::handle_trigger_trait::TriggerHandler;

pub struct EveryRequestsCountChangeTrigger {
    handler: Box<dyn Fn(usize) -> Result<()>>,
    last_count: usize,
}

impl EveryRequestsCountChangeTrigger {
    pub fn new(handler: Box<dyn Fn(usize) -> Result<()>>) -> Self {
        Self {
            handler,
            last_count: 0,
        }
    }
}

impl TriggerHandler for EveryRequestsCountChangeTrigger {
    fn handle(&mut self, available_requests_count: usize) -> Result<()> {
        if self.last_count != available_requests_count {
            (self.handler)(available_requests_count)?;
            self.last_count = available_requests_count;
        }

        Ok(())
    }
}
