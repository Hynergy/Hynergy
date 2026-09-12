use super::connection::ConnectionRef;
use crate::device::definition::{DefinitionId, DeviceDefinition, TerminalId};
use crate::parameter::ParameterId;
use smallvec::{SmallVec, smallvec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttachTerminalError {
    InvalidTerminal,
    AlreadyConnected,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DeviceSlot {
    definition_id: DefinitionId,
    terminals: SmallVec<[Option<ConnectionRef>; 4]>,
    parameters: SmallVec<[Option<f64>; 1]>,
}

impl DeviceSlot {
    #[inline]
    pub(super) fn new(definition_id: DefinitionId, definition: &DeviceDefinition) -> Self {
        Self {
            definition_id,
            terminals: smallvec![None; definition.terminals().len()],
            parameters: smallvec![None; definition.parameters().len()],
        }
    }

    #[inline]
    pub(super) fn attach_terminal(
        &mut self,
        terminal: TerminalId,
        connection: ConnectionRef,
    ) -> Result<(), AttachTerminalError> {
        let slot = self
            .terminals
            .get_mut(terminal.index())
            .ok_or(AttachTerminalError::InvalidTerminal)?;

        if slot.is_some() {
            return Err(AttachTerminalError::AlreadyConnected);
        }

        *slot = Some(connection);
        Ok(())
    }

    #[inline]
    pub(super) fn detach_terminal(&mut self, terminal: TerminalId) -> Option<ConnectionRef> {
        self.terminals.get_mut(terminal.index())?.take()
    }

    #[inline]
    pub(super) fn set_parameter(&mut self, parameter: ParameterId, value: f64) {
        self.parameters[parameter.index()] = Some(value);
    }

    #[inline]
    pub(super) fn device(&self) -> DefinitionId {
        self.definition_id
    }

    #[inline]
    pub(super) fn terminals(&self) -> &[Option<ConnectionRef>] {
        &self.terminals
    }

    #[inline]
    #[cfg(test)]
    pub(super) fn parameters(&self) -> &[Option<f64>] {
        &self.parameters
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct WireSlot(SmallVec<[ConnectionRef; 2]>);

impl WireSlot {
    #[inline]
    pub(super) fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub(super) fn add_connection(&mut self, connection: ConnectionRef) {
        self.0.push(connection);
    }

    #[inline]
    pub(super) fn remove_connection(&mut self, connection: ConnectionRef) -> bool {
        let Some(index) = self.0.iter().position(|&item| item == connection) else {
            return false;
        };

        self.0.swap_remove(index);
        true
    }

    #[inline]
    pub(super) fn contains_connection(&self, connection: ConnectionRef) -> bool {
        self.0.contains(&connection)
    }

    #[inline]
    pub(super) fn connections(&self) -> &[ConnectionRef] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceSlot;
    use crate::device::definition::{DefinitionId, PrimitiveElementKind};
    use crate::device::registry::DefinitionRegistry;
    use crate::parameter::ParameterId;

    #[test]
    fn device_parameters_start_unassigned_and_store_values_explicitly() {
        let registry = DefinitionRegistry::new();
        let definition_id = DefinitionId::from(PrimitiveElementKind::Admittance);
        let definition = registry.get(definition_id).unwrap();
        let mut slot = DeviceSlot::new(definition_id, definition);

        assert_eq!(slot.parameters(), &[None]);

        slot.set_parameter(ParameterId::new(0), 1.0);

        assert_eq!(slot.parameters(), &[Some(1.0)]);
    }
}
