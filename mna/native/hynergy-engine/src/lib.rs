pub mod topology;

use crate::topology::DerivedTopology;
use hynergy_model::device::definition::{DefinitionId, DeviceDefinition};
use hynergy_model::device::registry::{DefinitionRegistry, RegisterDeviceError};
use hynergy_model::network::Network;

pub struct Engine {
    definition_registry: DefinitionRegistry,
    universe: Vec<World>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub const COMPOSITE_DEFINITION_ID_BASE: u32 = DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE;

    pub fn new() -> Self {
        Self {
            definition_registry: DefinitionRegistry::new(),
            universe: Vec::new(),
        }
    }

    #[inline]
    pub fn definitions(&self) -> &DefinitionRegistry {
        &self.definition_registry
    }

    #[inline]
    pub fn register_definition(
        &mut self,
        definition: DeviceDefinition,
    ) -> Result<DefinitionId, RegisterDeviceError> {
        self.definition_registry.register(definition)
    }

    #[inline]
    pub fn new_world(&mut self) -> u32 {
        let id = self.universe.len();
        self.universe.push(World::default());
        id as u32
    }
}

#[derive(Debug, Default, Clone)]
pub struct World {
    _network: Network,
    _derived_topology: DerivedTopology,
}
