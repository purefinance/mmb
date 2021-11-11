use std::{hash::Hash, sync::Arc};

/// Entity needed to describe a configuration of trading strategy, which helps to determine which strategy the balance change refers.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub struct ConfigurationDescriptor {
    /// Trading strategy name
    pub service_name: String,
    pub service_configuration_key: String,
}

impl ConfigurationDescriptor {
    pub fn new(service_name: String, service_configuration_key: String) -> Arc<Self> {
        Arc::new(Self {
            service_name,
            service_configuration_key,
        })
    }
}
