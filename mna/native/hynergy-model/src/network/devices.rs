use super::{ConnectionType, DeviceInsertResult, DeviceRemoveResult, Network, NetworkModelError};
use crate::device::definition::{DefinitionId, DeviceId, TerminalId};
use crate::device::registry::DefinitionRegistry;
use crate::parameter::ParameterId;
use hynergy_ids::MAX_PACKED_ID;

impl Network {
    pub fn add_device(
        &mut self,
        definition_registry: &DefinitionRegistry,
        id: DeviceId,
        definition_id: DefinitionId,
    ) -> Result<DeviceInsertResult, NetworkModelError> {
        if id.get() > MAX_PACKED_ID {
            return Err(NetworkModelError::IdExceeds31Bit { id: id.id() });
        }

        let index = id.index();
        let len = self.device_arena.directory_len();

        if index > len {
            return Err(NetworkModelError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        if index < len && self.device_arena.is_assigned(id) {
            return Err(NetworkModelError::IdAlreadyAssigned { id: id.id() });
        }

        let definition =
            definition_registry
                .get(definition_id)
                .ok_or(NetworkModelError::UnknownDefinition {
                    definition: definition_id,
                })?;

        self.device_arena.insert(id, definition_id, definition)
    }

    pub fn remove_device(&mut self, id: DeviceId) -> Result<DeviceRemoveResult, NetworkModelError> {
        let index = id.index();
        let len = self.device_arena.directory_len();

        if index >= len {
            return Err(NetworkModelError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        let terminals = self.device(id)?.terminals().to_vec();

        for (terminal_index, connection) in terminals.into_iter().enumerate() {
            let Some(connection) = connection else {
                continue;
            };

            if connection.connection_type() == ConnectionType::Device && connection.index() == index
            {
                continue;
            }

            let terminal = TerminalId::from(
                u32::try_from(terminal_index)
                    .expect("definition terminal count fits in TerminalId"),
            );

            let removed_ref = Self::terminal_ref(id, terminal)
                .expect("stored terminal index fits packed connection");

            self.unlink_one_way(connection, removed_ref);
        }

        self.device_arena.remove(id)
    }

    pub fn set_device_parameter(
        &mut self,
        definition_registry: &DefinitionRegistry,
        device: DeviceId,
        parameter: ParameterId,
        value: f64,
    ) -> Result<(), NetworkModelError> {
        let definition_id = self.device(device)?.definition_id();

        let definition =
            definition_registry
                .get(definition_id)
                .ok_or(NetworkModelError::UnknownDefinition {
                    definition: definition_id,
                })?;

        let constraints = definition
            .parameters()
            .get(parameter.index())
            .ok_or(NetworkModelError::InvalidParameter { parameter })?;

        constraints.validate(value).map_err(|source| {
            NetworkModelError::ParameterConstraintViolation { parameter, source }
        })?;

        debug_assert!(value.is_finite());

        self.device_arena.set_parameter(device, parameter, value)
    }
}
