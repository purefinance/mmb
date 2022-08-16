pub trait TriggerHandler {
    fn handle(&mut self, available_requests_count: usize);
}
