use std::hash::Hash;

use mmb_utils::impl_table_type;
use serde::Serialize;

// An unique name of service, like strategy name or something else.
impl_table_type!(ServiceName, 16);

// An unique key for separate exchanges/currency_pairs into strategy.
impl_table_type!(ServiceConfigurationKey, 16);

/// Entity needed to describe a configuration of trading strategy, which helps to determine which strategy the balance change refers.
#[derive(Hash, Copy, Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfigurationDescriptor {
    /// Trading strategy name
    pub service_name: ServiceName,
    pub service_configuration_key: ServiceConfigurationKey,
}

impl ConfigurationDescriptor {
    pub fn new(
        service_name: ServiceName,
        service_configuration_key: ServiceConfigurationKey,
    ) -> Self {
        Self {
            service_name,
            service_configuration_key,
        }
    }
}
