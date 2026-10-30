//! The registry's two hookups to the kit: a subsystem per profile for the
//! kit's health report, and the scrape source for its `/metrics`.

use std::sync::Arc;

use super::registry::Registry;
use crate::pump::Health;

pub struct ProfileSubsystem {
    name: String,
    registry: Arc<Registry>,
}

impl ProfileSubsystem {
    pub fn new(name: &str, registry: Arc<Registry>) -> Self {
        Self {
            name: name.to_string(),
            registry,
        }
    }
}

impl chassis::Subsystem for ProfileSubsystem {
    fn name(&self) -> &str {
        &self.name
    }
    fn check(&self) -> chassis::SubsystemStatus {
        match self.registry.health_of(&self.name) {
            None | Some(Health::Starting) => chassis::SubsystemStatus::ok("starting"),
            Some(Health::Working) => chassis::SubsystemStatus::ok("working"),
            Some(Health::Failing) => chassis::SubsystemStatus::failing("failing"),
            Some(Health::Denied) => chassis::SubsystemStatus::failing("denied"),
            Some(Health::HubDown) => chassis::SubsystemStatus::failing("hub-down"),
        }
    }
}

/// The `switchboard_*` metrics, appended verbatim to the kit's `/metrics`.
pub struct RegistryMetrics(pub Arc<Registry>);

impl chassis::ScrapeSource for RegistryMetrics {
    fn scrape(&self) -> String {
        self.0.metrics()
    }
}
