pub mod topology;

use crate::topology::DerivedTopology;
use hynergy_model::device::definition::{DefinitionId, DeviceDefinition, DeviceId, TerminalId};
use hynergy_model::device::registry::{DefinitionRegistry, RegisterDeviceError};
use hynergy_model::network::{Network, NetworkModelError, WireId};

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
    network: Network,
    derived_topology: DerivedTopology,
}

impl World {
    #[inline]
    pub fn network(&self) -> &Network {
        &self.network
    }

    pub fn add_wire(&mut self, wire: WireId) -> Result<(), NetworkModelError> {
        self.network.add_wire(wire)?;
        self.derived_topology.add_wire(wire);
        self.debug_validate_topology();
        Ok(())
    }

    pub fn remove_wire(&mut self, wire: WireId) -> Result<(), NetworkModelError> {
        self.network.remove_wire(wire)?;
        self.derived_topology.remove_wire(&self.network, wire);
        self.debug_validate_topology();
        Ok(())
    }

    pub fn connect_wires(
        &mut self,
        wire_a: WireId,
        wire_b: WireId,
    ) -> Result<(), NetworkModelError> {
        self.network.connect_wires(wire_a, wire_b)?;
        self.derived_topology.connect_wires(wire_a, wire_b);
        self.debug_validate_topology();
        Ok(())
    }

    pub fn disconnect_wires(
        &mut self,
        wire_a: WireId,
        wire_b: WireId,
    ) -> Result<(), NetworkModelError> {
        self.network.disconnect_wires(wire_a, wire_b)?;
        self.derived_topology
            .disconnect_wires(&self.network, wire_a, wire_b);
        self.debug_validate_topology();
        Ok(())
    }

    pub fn add_device(
        &mut self,
        definitions: &DefinitionRegistry,
        device: DeviceId,
        definition: DefinitionId,
    ) -> Result<(), NetworkModelError> {
        self.network.add_device(definitions, device, definition)?;
        self.derived_topology.add_device(device);
        self.debug_validate_topology();
        Ok(())
    }

    pub fn remove_device(&mut self, device: DeviceId) -> Result<(), NetworkModelError> {
        self.network.remove_device(device)?;
        self.derived_topology.remove_device(&self.network, device);
        self.debug_validate_topology();
        Ok(())
    }

    pub fn attach_terminal(
        &mut self,
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        self.network.attach_terminal(wire, device, terminal)?;
        self.derived_topology.attach_terminal(wire, device);
        self.debug_validate_topology();
        Ok(())
    }

    pub fn detach_terminal(
        &mut self,
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        self.network.detach_terminal(wire, device, terminal)?;
        self.derived_topology
            .detach_terminal(&self.network, wire, device);
        self.debug_validate_topology();
        Ok(())
    }

    #[inline]
    fn debug_validate_topology(&self) {
        #[cfg(debug_assertions)]
        self.derived_topology.assert_consistent(&self.network);
    }
}

#[cfg(test)]
mod tests {
    use super::World;
    use hynergy_model::device::definition::{DeviceId, PrimitiveElementKind, TerminalId};
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::{NetworkModelError, WireId};

    fn wire(raw: u32) -> WireId {
        WireId::try_from(raw).unwrap()
    }

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    #[test]
    fn world_updates_authoritative_network_before_derived_topology() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::default();
        let a = wire(1);
        let b = wire(2);
        let d = device(1);

        world.add_wire(a).unwrap();
        world.add_wire(b).unwrap();
        world.connect_wires(a, b).unwrap();
        world
            .add_device(&definitions, d, PrimitiveElementKind::Admittance.into())
            .unwrap();
        world.attach_terminal(a, d, TerminalId::new(0)).unwrap();
        world.detach_terminal(a, d, TerminalId::new(0)).unwrap();
        world.disconnect_wires(a, b).unwrap();
        world.remove_wire(b).unwrap();
        world.remove_device(d).unwrap();

        world.derived_topology.assert_consistent(&world.network);
    }

    #[test]
    fn failed_model_mutation_does_not_change_derived_topology() {
        let mut world = World::default();
        let a = wire(1);
        let b = wire(2);

        world.add_wire(a).unwrap();
        world.add_wire(b).unwrap();
        world.connect_wires(a, b).unwrap();
        let before = world.derived_topology.clone();

        assert_eq!(
            world.connect_wires(a, b),
            Err(NetworkModelError::AlreadyConnected)
        );
        assert_eq!(world.derived_topology, before);
        world.derived_topology.assert_consistent(&world.network);
    }
}
