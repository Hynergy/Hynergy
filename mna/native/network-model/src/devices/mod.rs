use crate::circuits::Circuit;
use crate::{
    ConnectionRef, NodeId, ParameterConstraints, ParameterId, define_id, define_non_zero_id,
};
use smallvec::{SmallVec, smallvec};
use std::num::NonZeroU32;
use thiserror::Error;

pub mod builder;
pub mod registry;

define_id!(TerminalId);
define_non_zero_id!(DefinitionId: NonZeroU32, DeviceId);

impl DefinitionId {
    /// # Safety
    ///
    /// [`n`] must not be zero.
    pub unsafe fn new_unchecked(n: u32) -> Self {
        Self(unsafe { NonZeroU32::new_unchecked(n) })
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegisterDeviceError {
    #[error("the definition registry exhausted the DefinitionId range")]
    DefinitionIdExhausted,

    #[error("primitive device {kind:#?} is built-in and cannot be registered")]
    PrimitiveRegistrationForbidden { kind: PrimitiveElementKind },

    #[error(
        "the definition exposes {terminal_count} terminals, \
         but its circuit contains only {node_count} nodes"
    )]
    TerminalCountExceedsNodeCount {
        terminal_count: u32,
        node_count: u32,
    },

    #[error("definition {definition:#?} is not registered")]
    UnknownDefinition { definition: DefinitionId },
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveElementKind {
    Admittance = 0,
    Impedance = 1,
    AcrossSource = 2,
    ThroughSource = 3,
    ControlledThroughSource = 4,
    ControlledAcrossSource = 5,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceBody {
    Primitive(PrimitiveElementKind),
    Composite(Circuit),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceDefinition {
    body: DeviceBody,
    terminals: SmallVec<[NodeId; 4]>,
    param_constraints: SmallVec<[ParameterConstraints; 1]>,
}

impl DeviceDefinition {
    pub fn new_composite(
        circuit: Circuit,
        terminals: impl Into<SmallVec<[NodeId; 4]>>,
        param_constraints: impl Into<SmallVec<[ParameterConstraints; 1]>>,
    ) -> Self {
        Self {
            body: DeviceBody::Composite(circuit),
            terminals: terminals.into(),
            param_constraints: param_constraints.into(),
        }
    }

    pub fn body(&self) -> &DeviceBody {
        &self.body
    }

    pub fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    pub fn parameters(&self) -> &[ParameterConstraints] {
        &self.param_constraints
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachTerminalError {
    InvalidTerminal,
    AlreadyConnected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSlot {
    definition_id: DefinitionId,
    terminals: SmallVec<[Option<ConnectionRef>; 4]>,
    parameters: SmallVec<[f64; 1]>,
}

impl DeviceSlot {
    #[inline]
    pub fn new(definition_id: DefinitionId, definition: &DeviceDefinition) -> Self {
        Self {
            definition_id,
            terminals: smallvec![
                None;
                definition.terminals.len()
            ],
            parameters: smallvec![0.0; definition.param_constraints.len()],
        }
    }

    pub fn set_device(&mut self, definition_id: DefinitionId, definition: &DeviceDefinition) {
        self.definition_id = definition_id;
        self.terminals = smallvec![
            None;
            definition.terminals.len()
        ];
        self.parameters = smallvec![0.0; definition.param_constraints.len()];
    }

    #[inline]
    pub fn attach_terminal(
        &mut self,
        terminal: TerminalId,
        connection: ConnectionRef,
    ) -> Result<(), AttachTerminalError> {
        let slot = self
            .terminals
            .get_mut(terminal.0 as usize)
            .ok_or(AttachTerminalError::InvalidTerminal)?;

        if slot.is_some() {
            return Err(AttachTerminalError::AlreadyConnected);
        }

        *slot = Some(connection);
        Ok(())
    }

    #[inline]
    pub fn detach_terminal(&mut self, terminal: TerminalId) -> Option<ConnectionRef> {
        self.terminals.get_mut(terminal.0 as usize)?.take()
    }

    #[inline]
    pub fn detach_terminal_if(&mut self, terminal: TerminalId, expected: ConnectionRef) -> bool {
        let Some(slot) = self.terminals.get_mut(terminal.0 as usize) else {
            return false;
        };

        if *slot != Some(expected) {
            return false;
        }

        *slot = None;
        true
    }

    #[inline]
    pub fn set_parameter(&mut self, parameter: ParameterId, value: f64) {
        self.parameters[parameter.0 as usize] = value;
    }

    #[inline]
    pub fn device(&self) -> DefinitionId {
        self.definition_id
    }

    #[inline]
    pub fn terminals(&self) -> &[Option<ConnectionRef>] {
        &self.terminals
    }

    #[inline]
    pub fn parameters(&self) -> &[f64] {
        &self.parameters
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct TerminalRef {
    device: DeviceId,
    terminal: TerminalId,
}
