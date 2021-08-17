use std::hash::Hash;

#[derive(Hash, Debug, Clone, Eq)]
pub struct ConfigurationDescriptor {
    pub service_name: String,
    pub service_configuration_key: String,
}

impl ConfigurationDescriptor {
    pub fn new(service_name: String, service_configuration_key: String) -> Self {
        Self {
            service_name: service_name,
            service_configuration_key: service_configuration_key,
        }
    }
}

impl PartialEq for ConfigurationDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.service_name == other.service_name
            && self.service_configuration_key == other.service_configuration_key
    }
}
