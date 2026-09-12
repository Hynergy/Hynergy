use crate::circuit::{Circuit, NodeId};
use crate::ids::{define_id, define_non_zero_id};
use crate::parameter::ParameterConstraints;
use smallvec::SmallVec;
use std::num::NonZeroU32;

define_id!(TerminalId);
define_non_zero_id!(DefinitionId: NonZeroU32, DeviceId);

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

impl From<PrimitiveElementKind> for DefinitionId {
    fn from(kind: PrimitiveElementKind) -> Self {
        let raw = kind as u32 + 1;
        DefinitionId::new(NonZeroU32::new(raw).expect("primitive definition IDs start at one"))
    }
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
    pub(crate) fn new_primitive(
        kind: PrimitiveElementKind,
        terminals: impl Into<SmallVec<[NodeId; 4]>>,
        param_constraints: impl Into<SmallVec<[ParameterConstraints; 1]>>,
    ) -> Self {
        Self {
            body: DeviceBody::Primitive(kind),
            terminals: terminals.into(),
            param_constraints: param_constraints.into(),
        }
    }

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
