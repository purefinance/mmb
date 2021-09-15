use crate::core::DateTime;

#[derive(Clone)]
pub struct DateTimeService {
    #[cfg(test)]
    pub now: DateTime,
}

#[cfg(not(test))]
impl DateTimeService {
    pub fn new() -> Self {
        Self {}
    }
    pub fn now(&self) -> DateTime {
        chrono::Utc::now()
    }
}

#[cfg(test)]
impl DateTimeService {
    pub fn new(now: DateTime) -> Self {
        Self { now }
    }
    pub fn now(&self) -> DateTime {
        self.now
    }
}
