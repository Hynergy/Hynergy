use crate::circuit::{Circuit, NodeId};
use crate::parameter::ParameterConstraints;
use hynergy_ids::{define_id, define_non_zero_id};
use smallvec::{SmallVec, smallvec};
use thiserror::Error;

define_id!(TerminalId, TerminalPartitionId: u16);
define_non_zero_id!(DefinitionId, DeviceId);

use crate::parameter::{Bound, ParameterConstraintError};

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveElementKind {
    Resistance = 0,
    Conductance = 1,
    VoltageSource = 2,
    CurrentSource = 3,
    VoltageControlledCurrentSource = 4,
    VoltageControlledVoltageSource = 5,
    Capacitor = 6,
    Inductor = 7,
    VoltageControlledSwitch = 8,
    VoltageControlledConductance = 9,
    TickDelay = 10,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveParameterError {
    #[error("primitive expects {expected} parameters, but {actual} were provided")]
    WrongParameterCount { expected: usize, actual: usize },

    #[error("primitive parameter {index} is invalid: {source}")]
    InvalidParameter {
        index: usize,
        source: ParameterConstraintError,
    },

    #[error("primitive parameter {greater} must be greater than parameter {lesser}")]
    ParameterMustBeGreater { greater: usize, lesser: usize },
}

impl PrimitiveElementKind {
    pub const ALL: [Self; 11] = [
        Self::Resistance,
        Self::Conductance,
        Self::VoltageSource,
        Self::CurrentSource,
        Self::VoltageControlledCurrentSource,
        Self::VoltageControlledVoltageSource,
        Self::Capacitor,
        Self::Inductor,
        Self::VoltageControlledSwitch,
        Self::VoltageControlledConductance,
        Self::TickDelay,
    ];

    pub const COUNT: u32 = Self::ALL.len() as u32;

    pub const fn parameter_count(self) -> usize {
        match self {
            Self::Resistance => 1,
            Self::Conductance => 1,
            Self::VoltageSource => 1,
            Self::CurrentSource => 1,
            Self::VoltageControlledCurrentSource => 1,
            Self::VoltageControlledVoltageSource => 1,
            Self::Capacitor => 1,
            Self::Inductor => 1,
            Self::VoltageControlledSwitch => 4,
            Self::VoltageControlledConductance => 4,
            Self::TickDelay => 1,
        }
    }

    fn parameter_constraint(self, index: usize) -> Option<ParameterConstraints> {
        let unrestricted = ParameterConstraints::default();

        let positive = ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            None,
            false,
            None,
        );

        let non_negative = ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: true,
            }),
            None,
            false,
            None,
        );

        match self {
            Self::Resistance => [positive].get(index).copied(),

            Self::Conductance => [non_negative].get(index).copied(),

            Self::VoltageSource => [unrestricted].get(index).copied(),

            Self::CurrentSource => [unrestricted].get(index).copied(),

            Self::VoltageControlledCurrentSource => {
                // transconductance
                [unrestricted].get(index).copied()
            }

            Self::VoltageControlledVoltageSource => {
                // voltage gain
                [unrestricted].get(index).copied()
            }

            Self::Capacitor => {
                // capacitance
                [positive].get(index).copied()
            }

            Self::Inductor => {
                // inductance
                [positive].get(index).copied()
            }

            Self::VoltageControlledSwitch => {
                // 0: threshold
                // 1: hysteresis
                // 2: G_max
                // 3: G_min
                [unrestricted, non_negative, positive, non_negative]
                    .get(index)
                    .copied()
            }

            Self::VoltageControlledConductance => {
                // 0: V_threshold
                // 1: V_transition
                // 2: G_min
                // 3: G_max
                [unrestricted, positive, non_negative, positive]
                    .get(index)
                    .copied()
            }

            Self::TickDelay => {
                // initial output
                [unrestricted].get(index).copied()
            }
        }
    }

    fn terminal_layout(self) -> (SmallVec<[NodeId; 4]>, SmallVec<[TerminalPartitionId; 4]>) {
        match self {
            Self::Resistance
            | Self::Conductance
            | Self::VoltageSource
            | Self::CurrentSource
            | Self::Capacitor
            | Self::Inductor => (smallvec![0.into(), 1.into()], smallvec![0.into(), 0.into()]),

            Self::VoltageControlledCurrentSource
            | Self::VoltageControlledVoltageSource
            | Self::VoltageControlledSwitch => (
                smallvec![0.into(), 1.into(), 2.into(), 3.into()],
                smallvec![0.into(), 0.into(), 0.into(), 0.into(),],
            ),

            Self::VoltageControlledConductance => (
                smallvec![0.into(), 1.into(), 2.into()],
                smallvec![0.into(), 0.into(), 0.into()],
            ),

            Self::TickDelay => (
                smallvec![0.into(), 1.into(), 2.into(), 3.into()],
                smallvec![0.into(), 0.into(), 1.into(), 1.into(),],
            ),
        }
    }

    pub(crate) fn definition(self) -> DeviceDefinition {
        let (terminals, terminal_partitions) = self.terminal_layout();

        let param_constraints = (0..self.parameter_count())
            .map(|index| {
                self.parameter_constraint(index)
                    .expect("parameter index is within primitive parameter count")
            })
            .collect::<SmallVec<[ParameterConstraints; 1]>>();

        DeviceDefinition::new_primitive(self, terminals, param_constraints, terminal_partitions)
    }

    pub fn validate_parameters(self, parameters: &[f64]) -> Result<(), PrimitiveParameterError> {
        let expected = self.parameter_count();

        if parameters.len() != expected {
            return Err(PrimitiveParameterError::WrongParameterCount {
                expected,
                actual: parameters.len(),
            });
        }

        for (index, &value) in parameters.iter().enumerate() {
            self.parameter_constraint(index)
                .expect("parameter index is within primitive parameter count")
                .validate(value)
                .map_err(|source| PrimitiveParameterError::InvalidParameter { index, source })?;
        }

        self.validate_parameter_relations_unchecked(parameters)
    }

    pub(crate) fn validate_parameter_relations(
        self,
        parameters: &[f64],
    ) -> Result<(), PrimitiveParameterError> {
        let expected = self.parameter_count();

        if parameters.len() != expected {
            return Err(PrimitiveParameterError::WrongParameterCount {
                expected,
                actual: parameters.len(),
            });
        }

        self.validate_parameter_relations_unchecked(parameters)
    }

    fn validate_parameter_relations_unchecked(
        self,
        parameters: &[f64],
    ) -> Result<(), PrimitiveParameterError> {
        debug_assert_eq!(parameters.len(), self.parameter_count());

        match self {
            Self::VoltageControlledSwitch => {
                if parameters[2] <= parameters[3] {
                    return Err(PrimitiveParameterError::ParameterMustBeGreater {
                        greater: 2,
                        lesser: 3,
                    });
                }
            }

            Self::VoltageControlledConductance if parameters[3] <= parameters[2] => {
                return Err(PrimitiveParameterError::ParameterMustBeGreater {
                    greater: 3,
                    lesser: 2,
                });
            }

            _ => {}
        }

        Ok(())
    }
}

impl From<PrimitiveElementKind> for DefinitionId {
    fn from(kind: PrimitiveElementKind) -> Self {
        let raw = kind as u32 + 1;

        DefinitionId::try_from(raw).expect("primitive definition IDs start at one")
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
    terminal_partitions: SmallVec<[TerminalPartitionId; 4]>,
}

impl DeviceDefinition {
    pub(crate) fn new_primitive(
        kind: PrimitiveElementKind,
        terminals: impl Into<SmallVec<[NodeId; 4]>>,
        param_constraints: impl Into<SmallVec<[ParameterConstraints; 1]>>,
        terminal_partitions: impl Into<SmallVec<[TerminalPartitionId; 4]>>,
    ) -> Self {
        Self {
            body: DeviceBody::Primitive(kind),
            terminals: terminals.into(),
            param_constraints: param_constraints.into(),
            terminal_partitions: terminal_partitions.into(),
        }
    }

    pub(crate) fn new_composite(
        circuit: Circuit,
        terminals: impl Into<SmallVec<[NodeId; 4]>>,
        param_constraints: impl Into<SmallVec<[ParameterConstraints; 1]>>,
        terminal_partitions: impl Into<SmallVec<[TerminalPartitionId; 4]>>,
    ) -> Self {
        Self {
            body: DeviceBody::Composite(circuit),
            terminals: terminals.into(),
            param_constraints: param_constraints.into(),
            terminal_partitions: terminal_partitions.into(),
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

    pub fn terminal_partitions(&self) -> &[TerminalPartitionId] {
        &self.terminal_partitions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voltage_controlled_switch_requires_gmax_above_gmin() {
        let kind = PrimitiveElementKind::VoltageControlledSwitch;

        assert_eq!(
            kind.validate_parameters(&[
                5.0,  // threshold
                1.0,  // hysteresis
                10.0, // G_max
                0.01, // G_min
            ]),
            Ok(())
        );

        assert_eq!(
            kind.validate_parameters(&[5.0, 1.0, 0.01, 0.01,]),
            Err(PrimitiveParameterError::ParameterMustBeGreater {
                greater: 2,
                lesser: 3,
            })
        );
    }

    #[test]
    fn voltage_controlled_conductance_requires_gmax_above_gmin() {
        let kind = PrimitiveElementKind::VoltageControlledConductance;

        assert_eq!(
            kind.validate_parameters(&[
                5.0,  // V_threshold
                2.0,  // V_transition
                0.01, // G_min
                10.0, // G_max
            ]),
            Ok(())
        );

        assert_eq!(
            kind.validate_parameters(&[5.0, 2.0, 10.0, 1.0,]),
            Err(PrimitiveParameterError::ParameterMustBeGreater {
                greater: 3,
                lesser: 2,
            })
        );
    }

    #[test]
    fn primitive_count_matches_last_discriminant() {
        assert_eq!(
            PrimitiveElementKind::COUNT,
            PrimitiveElementKind::TickDelay as u32 + 1,
        );
    }
}
