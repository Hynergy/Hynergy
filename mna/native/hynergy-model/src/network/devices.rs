use super::slot::DeviceSlot;
use super::{ConnectionType, Network, NetworkModelError};
use crate::device::definition::{DefinitionId, DeviceId, TerminalId};
use crate::device::registry::DefinitionRegistry;
use crate::parameter::ParameterId;

impl Network {
    pub fn add_device(
        &mut self,
        definition_registry: &DefinitionRegistry,
        id: DeviceId,
        definition_id: DefinitionId,
    ) -> Result<(), NetworkModelError> {
        let index = id.index();
        let len = self.devices.len();

        if index > len {
            return Err(NetworkModelError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }
        if index < len && self.devices[index].is_some() {
            return Err(NetworkModelError::IdAlreadyAssigned { id: id.id() });
        }

        let definition =
            definition_registry
                .get(definition_id)
                .ok_or(NetworkModelError::UnknownDefinition {
                    definition: definition_id,
                })?;
        let slot = Some(DeviceSlot::new(definition_id, definition));

        if index == len {
            self.devices.push(slot);
        } else {
            self.devices[index] = slot;
        }

        Ok(())
    }

    pub fn remove_device(&mut self, id: DeviceId) -> Result<(), NetworkModelError> {
        let index = id.index();
        let len = self.devices.len();

        if index >= len {
            return Err(NetworkModelError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        let slot = self.devices[index]
            .take()
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: id.id(),
            })?;

        for (terminal_index, connection) in slot.terminals().iter().enumerate() {
            let Some(connection) = *connection else {
                continue;
            };

            if connection.connection_type() == ConnectionType::Device && connection.index() == index
            {
                continue;
            }

            let terminal = TerminalId::from(
                u32::try_from(terminal_index).expect("device terminal count fits in TerminalId"),
            );
            let removed_ref = Self::terminal_ref(id, terminal)
                .expect("stored terminal index fits packed connection");

            self.unlink_one_way(connection, removed_ref);
        }

        Ok(())
    }

    pub fn set_device_parameter(
        &mut self,
        definition_registry: &DefinitionRegistry,
        device: DeviceId,
        parameter: ParameterId,
        value: f64,
    ) -> Result<(), NetworkModelError> {
        let slot = Self::device_mut(&mut self.devices, device)?;
        let definition =
            definition_registry
                .get(slot.device())
                .ok_or(NetworkModelError::UnknownDefinition {
                    definition: slot.device(),
                })?;
        let constraints = definition
            .parameters()
            .get(parameter.index())
            .ok_or(NetworkModelError::InvalidParameter { parameter })?;

        constraints
            .validate(value)
            .map_err(|source| NetworkModelError::ParameterConstraint { parameter, source })?;

        slot.set_parameter(parameter, value);
        Ok(())
    }
}
