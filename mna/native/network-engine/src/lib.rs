use crate::world::World;
use network_model::devices::registry::DefinitionRegistry;
use network_model::devices::{DefinitionId, DeviceDefinition, RegisterDeviceError};

pub mod world;

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

    pub fn definitions(&self) -> &DefinitionRegistry {
        &self.definition_registry
    }

    pub fn register_definition(
        &mut self,
        id: DefinitionId,
        definition: DeviceDefinition,
    ) -> Result<(), RegisterDeviceError> {
        self.definition_registry.register(id, definition)?;
        Ok(())
    }
}
