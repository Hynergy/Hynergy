mod connection;
mod device_arena;
mod devices;
mod slot;
mod terminals;
mod wires;

pub use connection::ConnectionRef;
pub use connection::ConnectionType;
pub use device_arena::{DeviceChunkRelocation, DeviceInsertResult, DeviceRemoveResult, DeviceView};

use crate::device::definition::{DefinitionId, DeviceId, TerminalId};
use crate::network::device_arena::DeviceArena;
use crate::network::slot::WireSlot;
use crate::parameter::{ParameterConstraintError, ParameterId};
use hynergy_ids::define_non_zero_id;
use std::num::NonZeroU32;
use thiserror::Error;

define_non_zero_id!(WireId);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NetworkModelError {
    #[error("provided id {id:#?} exceeds max bound {upper_bound:#?}")]
    IdOutOfBound { id: NonZeroU32, upper_bound: usize },

    #[error("id {id:#?} exceeds the 31-bit ID limit")]
    IdExceeds31Bit { id: NonZeroU32 },

    #[error("id {id:#?} is already assigned")]
    IdAlreadyAssigned { id: NonZeroU32 },

    #[error("no {ty:#?} with assigned id {id:#?}")]
    IdNotAssigned { ty: ConnectionType, id: NonZeroU32 },

    #[error("cannot connect wire to itself")]
    WireConnectToSelf,

    #[error("elements are already connected")]
    AlreadyConnected,

    #[error("elements are not connected")]
    NotConnected,

    #[error("terminal is already connected")]
    TerminalAlreadyConnected,

    #[error("invalid terminal")]
    InvalidTerminal,

    #[error("invalid parameter {parameter:#?}")]
    InvalidParameter { parameter: ParameterId },

    #[error("parameter {parameter:#?} violates its constraints: {source}")]
    ParameterConstraintViolation {
        parameter: ParameterId,
        #[source]
        source: ParameterConstraintError,
    },

    #[error("definition {definition:#?} is not registered")]
    UnknownDefinition { definition: DefinitionId },

    #[error("resident device arena exhausted its packed chunk-index range")]
    DeviceArenaExhausted,
}

#[derive(Debug, Default, Clone)]
pub struct Network {
    wires: Vec<Option<WireSlot>>,
    device_arena: DeviceArena,
}

impl Network {
    pub fn new() -> Self {
        Self {
            wires: Vec::new(),
            device_arena: DeviceArena::default(),
        }
    }

    pub fn with_capacity(wires: usize, devices: usize) -> Self {
        Self {
            wires: Vec::with_capacity(wires),
            device_arena: DeviceArena::with_directory_capacity(devices),
        }
    }

    #[inline]
    fn wire_mut(
        wires: &mut [Option<WireSlot>],
        id: WireId,
    ) -> Result<&mut WireSlot, NetworkModelError> {
        wires
            .get_mut(id.index())
            .and_then(Option::as_mut)
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: id.id(),
            })
    }

    #[inline]
    fn terminal_connection(
        &self,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<Option<ConnectionRef>, NetworkModelError> {
        self.device_arena.terminal_connection(device, terminal)
    }

    #[inline]
    fn terminal_ref(
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<ConnectionRef, NetworkModelError> {
        ConnectionRef::terminal(device, terminal).ok_or(NetworkModelError::InvalidTerminal)
    }

    #[inline]
    fn unlink_one_way(&mut self, endpoint: ConnectionRef, peer: ConnectionRef) {
        match endpoint.connection_type() {
            ConnectionType::Wire => {
                let removed = self.wires[endpoint.index()]
                    .as_mut()
                    .expect("stored wire connection should reference a valid wire")
                    .remove_connection(peer);

                debug_assert!(removed, "bidirectional wire connection invariant violated");
            }
            ConnectionType::Device => {
                let (device, terminal) = endpoint
                    .as_terminal()
                    .expect("device connection must unpack as a terminal");

                let current = self
                    .terminal_connection(device, terminal)
                    .expect("stored terminal connection should reference a valid terminal");

                debug_assert_eq!(
                    current,
                    Some(peer),
                    "bidirectional terminal connection invariant violated"
                );

                let removed = self
                    .device_arena
                    .detach_terminal(device, terminal)
                    .expect("stored terminal connection should remain valid");

                debug_assert_eq!(removed, Some(peer));
            }
        }
    }

    #[inline]
    pub fn device(&self, device: DeviceId) -> Result<DeviceView<'_>, NetworkModelError> {
        self.device_arena.device(device)
    }

    #[inline]
    pub fn device_definition_id(
        &self,
        device: DeviceId,
    ) -> Result<DefinitionId, NetworkModelError> {
        Ok(self.device(device)?.definition_id())
    }

    #[inline]
    pub fn iter_devices(&self) -> impl Iterator<Item = (DeviceId, DeviceView<'_>)> {
        self.device_arena.iter_devices()
    }

    #[inline]
    pub fn wire_connections(&self, wire: WireId) -> Result<&[ConnectionRef], NetworkModelError> {
        self.wires
            .get(wire.index())
            .and_then(Option::as_ref)
            .map(WireSlot::connections)
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: wire.id(),
            })
    }

    #[inline]
    pub fn wires(&self) -> &[Option<WireSlot>] {
        &self.wires
    }

    #[inline]
    pub fn iter_device_ids(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_arena.iter_device_ids()
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionRef, Network, NetworkModelError, WireId};
    use crate::device::definition::{DefinitionId, DeviceId, PrimitiveElementKind, TerminalId};
    use crate::device::registry::DefinitionRegistry;
    use crate::parameter::{ParameterConstraintError, ParameterId};

    fn device_id(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    #[test]
    fn device_parameter_validation_preserves_previous_value() {
        let definitions = DefinitionRegistry::new();
        let mut model = Network::new();
        let device = device_id(1);
        let definition = DefinitionId::from(PrimitiveElementKind::Conductance);

        model.add_device(&definitions, device, definition).unwrap();
        assert_eq!(
            model.device(device).unwrap().parameter(ParameterId::new(0)),
            Some(None),
        );

        model
            .set_device_parameter(&definitions, device, ParameterId::new(0), 0.0)
            .unwrap();

        assert_eq!(
            model.device(device).unwrap().parameter(ParameterId::new(0)),
            Some(Some(0.0)),
        );

        model
            .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0)
            .unwrap();

        for (value, error) in [
            (-1.0, ParameterConstraintError::OutOfRange),
            (f64::NAN, ParameterConstraintError::NonFinite),
        ] {
            assert_eq!(
                model.set_device_parameter(&definitions, device, ParameterId::new(0), value,),
                Err(NetworkModelError::ParameterConstraintViolation {
                    parameter: ParameterId::new(0),
                    source: error,
                })
            );

            assert_eq!(
                model.device(device).unwrap().parameter(ParameterId::new(0)),
                Some(Some(1.0)),
            );
        }
    }

    #[test]
    fn wire_connections_exposes_typed_read_only_connections() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let wire_a = WireId::try_from(1).unwrap();
        let wire_b = WireId::try_from(2).unwrap();
        let device = device_id(1);
        let terminal = TerminalId::new(0);

        network.add_wire(wire_a).unwrap();
        network.add_wire(wire_b).unwrap();
        network.connect_wires(wire_a, wire_b).unwrap();
        network
            .add_device(
                &definitions,
                device,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();
        network.attach_terminal(wire_a, device, terminal).unwrap();

        let connections = network.wire_connections(wire_a).unwrap();
        assert!(
            connections
                .iter()
                .any(|connection| connection.as_wire() == Some(wire_b))
        );
        assert!(
            connections
                .iter()
                .any(|connection| connection.as_terminal() == Some((device, terminal)))
        );
    }

    #[test]
    fn detaching_terminal_removes_wire_connection() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let wire = WireId::try_from(1).unwrap();
        let device = device_id(1);
        let terminal = TerminalId::new(0);

        network.add_wire(wire).unwrap();
        network
            .add_device(
                &definitions,
                device,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();
        network.attach_terminal(wire, device, terminal).unwrap();

        network.detach_terminal(wire, device, terminal).unwrap();

        assert!(network.wire_connections(wire).unwrap().is_empty());
    }

    #[test]
    fn same_definition_devices_share_chunks_until_capacity() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let definition = DefinitionId::from(PrimitiveElementKind::Conductance);

        for raw in 1..=65 {
            network
                .add_device(&definitions, device_id(raw), definition)
                .unwrap();
        }

        assert_eq!(network.device_arena.chunk_count(), 2);
        assert_eq!(network.device_arena.chunk_len(0), 64);
        assert_eq!(network.device_arena.chunk_len(1), 1);

        assert_eq!(network.device_arena.chunk_definition(0), definition,);
        assert_eq!(network.device_arena.chunk_definition(1), definition,);

        network.device_arena.assert_consistent(&definitions);
    }

    #[test]
    fn different_definitions_never_share_a_chunk() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        network
            .add_device(
                &definitions,
                device_id(1),
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                device_id(2),
                DefinitionId::from(PrimitiveElementKind::VoltageSource),
            )
            .unwrap();

        assert_eq!(network.device_arena.chunk_count(), 2);

        assert_ne!(
            network.device_arena.chunk_definition(0),
            network.device_arena.chunk_definition(1),
        );
    }

    #[test]
    fn semantic_iteration_visits_each_resident_device_once() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let conductance = DefinitionId::from(PrimitiveElementKind::Conductance);
        let source = DefinitionId::from(PrimitiveElementKind::VoltageSource);

        network
            .add_device(&definitions, device_id(1), conductance)
            .unwrap();

        network
            .add_device(&definitions, device_id(2), source)
            .unwrap();

        network
            .add_device(&definitions, device_id(3), conductance)
            .unwrap();

        let mut ids = network.iter_devices().map(|(id, _)| id).collect::<Vec<_>>();

        ids.sort_unstable_by_key(|id| id.get());

        assert_eq!(ids, vec![device_id(1), device_id(2), device_id(3)]);
    }

    #[test]
    fn semantic_parameter_access_never_exposes_infinity_sentinel() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let id = device_id(1);

        network
            .add_device(
                &definitions,
                id,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        assert_eq!(
            network.device(id).unwrap().parameter(ParameterId::new(0)),
            Some(None),
        );

        network
            .set_device_parameter(&definitions, id, ParameterId::new(0), 1.25)
            .unwrap();

        assert_eq!(
            network.device(id).unwrap().parameter(ParameterId::new(0)),
            Some(Some(1.25)),
        );
    }

    #[test]
    fn removing_middle_row_moves_full_row_and_repairs_directory() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let definition = DefinitionId::from(PrimitiveElementKind::Conductance);

        for raw in 1..=3 {
            network
                .add_device(&definitions, device_id(raw), definition)
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, device_id(3), ParameterId::new(0), 7.0)
            .unwrap();

        let result = network.remove_device(device_id(2)).unwrap();

        assert_eq!(result.moved_device(), Some(device_id(3)));

        assert_eq!(
            network
                .device(device_id(3))
                .unwrap()
                .parameter(ParameterId::new(0)),
            Some(Some(7.0)),
        );

        assert!(matches!(
            network.device(device_id(2)),
            Err(NetworkModelError::IdNotAssigned { .. })
        ));

        network.device_arena.assert_consistent(&definitions);
    }

    #[test]
    fn removing_empty_non_last_chunk_repairs_every_moved_chunk_locator() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let first_definition = DefinitionId::from(PrimitiveElementKind::VoltageSource);

        let moved_definition = DefinitionId::from(PrimitiveElementKind::Conductance);

        network
            .add_device(&definitions, device_id(1), first_definition)
            .unwrap();

        for raw in 2..=4 {
            network
                .add_device(&definitions, device_id(raw), moved_definition)
                .unwrap();
        }

        let result = network.remove_device(device_id(1)).unwrap();

        let relocation = result
            .chunk_relocation()
            .expect("non-last empty chunk should be replaced");

        assert_eq!(relocation.from_chunk(), 1);
        assert_eq!(relocation.to_chunk(), 0);

        for raw in 2..=4 {
            assert_eq!(
                network.device(device_id(raw)).unwrap().definition_id(),
                moved_definition,
            );
        }

        let chunk_count = network.device_arena.chunk_count();

        network
            .add_device(&definitions, device_id(5), moved_definition)
            .unwrap();

        assert_eq!(network.device_arena.chunk_count(), chunk_count,);

        network.device_arena.assert_consistent(&definitions);
    }

    #[test]
    fn semantic_iteration_skips_removed_id_holes_without_duplicates() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let definition = DefinitionId::from(PrimitiveElementKind::Conductance);

        for raw in 1..=4 {
            network
                .add_device(&definitions, device_id(raw), definition)
                .unwrap();
        }

        network.remove_device(device_id(2)).unwrap();

        let mut ids = network.iter_devices().map(|(id, _)| id).collect::<Vec<_>>();

        ids.sort_unstable_by_key(|id| id.get());

        assert_eq!(ids, vec![device_id(1), device_id(3), device_id(4),],);
    }

    #[test]
    fn terminal_attach_detach_survives_device_row_relocation() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let wire = WireId::try_from(1).unwrap();
        let definition = DefinitionId::from(PrimitiveElementKind::Conductance);

        network.add_wire(wire).unwrap();

        for raw in 1..=3 {
            network
                .add_device(&definitions, device_id(raw), definition)
                .unwrap();
        }

        network
            .attach_terminal(wire, device_id(3), TerminalId::new(0))
            .unwrap();

        network.remove_device(device_id(2)).unwrap();

        assert_eq!(
            network.device(device_id(3)).unwrap().terminals()[0],
            Some(ConnectionRef::from(wire)),
        );

        network
            .detach_terminal(wire, device_id(3), TerminalId::new(0))
            .unwrap();

        assert!(network.wire_connections(wire).unwrap().is_empty());
    }
}
