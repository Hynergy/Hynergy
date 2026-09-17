use crate::circuit::{Circuit, ElementId, NodeId};
use crate::parameter::{Bound, ParameterConstraintError, ParameterConstraints};
use hynergy_ids::{define_id, define_non_zero_id};
use smallvec::{SmallVec, smallvec};
use thiserror::Error;

define_id!(TerminalId, DefinitionObserverId, DevicePartitionId: u16);
define_non_zero_id!(DefinitionId, DeviceId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObserverQuantity {
    Voltage,
    Current,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionObserverSource {
    Voltage {
        positive: NodeId,
        negative: NodeId,
    },

    Current {
        positive: NodeId,
        negative: NodeId,
    },

    Child {
        element: ElementId,
        observer: DefinitionObserverId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionObserver {
    quantity: ObserverQuantity,
    partition: DevicePartitionId,
    source: DefinitionObserverSource,
}

impl DefinitionObserver {
    #[inline]
    pub(crate) const fn new(
        quantity: ObserverQuantity,
        partition: DevicePartitionId,
        source: DefinitionObserverSource,
    ) -> Self {
        Self {
            quantity,
            partition,
            source,
        }
    }

    #[inline]
    pub const fn quantity(&self) -> ObserverQuantity {
        self.quantity
    }

    #[inline]
    pub const fn partition(&self) -> DevicePartitionId {
        self.partition
    }

    #[inline]
    pub const fn source(&self) -> DefinitionObserverSource {
        self.source
    }
}

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
    Diode = 11,
    Not = 12,
    And2 = 13,
    Nand2 = 14,
    Or2 = 15,
    Nor2 = 16,
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
    pub const ALL: [Self; 17] = [
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
        Self::Diode,
        Self::Not,
        Self::And2,
        Self::Nand2,
        Self::Or2,
        Self::Nor2,
    ];

    pub const COUNT: u32 = Self::ALL.len() as u32;

    pub const fn parameter_count(&self) -> usize {
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
            Self::Diode => 2,
            Self::Not | Self::And2 | Self::Nand2 | Self::Or2 | Self::Nor2 => 3,
        }
    }

    fn observers(self) -> Vec<DefinitionObserver> {
        let p0 = DevicePartitionId::new(0);
        let p1 = DevicePartitionId::new(1);

        let voltage = |positive: u32, negative: u32, partition: DevicePartitionId| {
            DefinitionObserver::new(
                ObserverQuantity::Voltage,
                partition,
                DefinitionObserverSource::Voltage {
                    positive: NodeId::new(positive),
                    negative: NodeId::new(negative),
                },
            )
        };

        let current = |positive: u32, negative: u32, partition: DevicePartitionId| {
            DefinitionObserver::new(
                ObserverQuantity::Current,
                partition,
                DefinitionObserverSource::Current {
                    positive: NodeId::new(positive),
                    negative: NodeId::new(negative),
                },
            )
        };

        match self {
            Self::Resistance
            | Self::Conductance
            | Self::VoltageSource
            | Self::CurrentSource
            | Self::Capacitor
            | Self::Inductor
            | Self::Diode => vec![voltage(0, 1, p0), current(0, 1, p0)],

            Self::Not => vec![voltage(0, 2, p0), voltage(3, 2, p0), current(1, 0, p0)],

            Self::And2 | Self::Nand2 | Self::Or2 | Self::Nor2 => vec![
                voltage(0, 2, p0),
                voltage(3, 2, p0),
                voltage(4, 2, p0),
                current(1, 0, p0),
            ],

            Self::VoltageControlledCurrentSource
            | Self::VoltageControlledVoltageSource
            | Self::VoltageControlledSwitch => {
                vec![voltage(0, 1, p0), voltage(2, 3, p0), current(0, 1, p0)]
            }

            Self::VoltageControlledConductance => {
                vec![voltage(0, 1, p0), voltage(2, 1, p0), current(0, 1, p0)]
            }

            Self::TickDelay => vec![voltage(0, 1, p0), voltage(2, 3, p1), current(2, 3, p1)],
        }
    }

    fn parameter_constraint(&self, index: usize) -> Option<ParameterConstraints> {
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

            Self::Diode => {
                // 0: G_max
                // 1: G_min
                [positive, non_negative].get(index).copied()
            }

            Self::Not | Self::And2 | Self::Nand2 | Self::Or2 | Self::Nor2 => {
                // 0: threshold relative to VSS
                // 1: G_max
                // 2: G_min
                [unrestricted, positive, non_negative].get(index).copied()
            }

            Self::TickDelay => {
                // initial output
                [unrestricted].get(index).copied()
            }
        }
    }

    fn partition_layout(&self) -> (SmallVec<[NodeId; 4]>, DevicePartitionLayout) {
        let (terminals, partitions) = match self {
            Self::Resistance
            | Self::Conductance
            | Self::VoltageSource
            | Self::CurrentSource
            | Self::Capacitor
            | Self::Inductor
            | Self::Diode => (smallvec![0.into(), 1.into()], smallvec![0.into(), 0.into()]),

            Self::Not => (
                smallvec![0.into(), 1.into(), 2.into(), 3.into()],
                smallvec![0.into(), 0.into(), 0.into(), 0.into()],
            ),

            Self::And2 | Self::Nand2 | Self::Or2 | Self::Nor2 => (
                smallvec![0.into(), 1.into(), 2.into(), 3.into(), 4.into()],
                smallvec![0.into(), 0.into(), 0.into(), 0.into(), 0.into()],
            ),

            Self::VoltageControlledCurrentSource
            | Self::VoltageControlledVoltageSource
            | Self::VoltageControlledSwitch => (
                smallvec![0.into(), 1.into(), 2.into(), 3.into()],
                smallvec![0.into(), 0.into(), 0.into(), 0.into()],
            ),

            Self::VoltageControlledConductance => (
                smallvec![0.into(), 1.into(), 2.into()],
                smallvec![0.into(), 0.into(), 0.into()],
            ),

            Self::TickDelay => (
                smallvec![0.into(), 1.into(), 2.into(), 3.into()],
                smallvec![0.into(), 0.into(), 1.into(), 1.into()],
            ),
        };

        let partition_layout = DevicePartitionLayout::try_new(partitions)
            .expect("primitive terminal partitions must be canonical");

        (terminals, partition_layout)
    }

    pub(crate) fn definition(self) -> DeviceDefinition {
        let (terminals, terminal_partition_layout) = self.partition_layout();

        let param_constraints = (0..self.parameter_count())
            .map(|index| {
                self.parameter_constraint(index)
                    .expect("parameter index is within primitive parameter count")
            })
            .collect::<SmallVec<[ParameterConstraints; 1]>>();

        DeviceDefinition::new_primitive(
            self,
            terminals,
            param_constraints,
            terminal_partition_layout,
            self.observers(),
        )
    }

    pub fn validate_parameters(&self, parameters: &[f64]) -> Result<(), PrimitiveParameterError> {
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
        &self,
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
        &self,
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

            Self::Diode if parameters[0] <= parameters[1] => {
                return Err(PrimitiveParameterError::ParameterMustBeGreater {
                    greater: 0,
                    lesser: 1,
                });
            }

            Self::Not | Self::And2 | Self::Nand2 | Self::Or2 | Self::Nor2
                if parameters[1] <= parameters[2] =>
            {
                return Err(PrimitiveParameterError::ParameterMustBeGreater {
                    greater: 1,
                    lesser: 2,
                });
            }

            _ => {}
        }

        Ok(())
    }

    pub const fn state_count(self) -> usize {
        match self {
            Self::Capacitor | Self::Inductor | Self::VoltageControlledSwitch | Self::TickDelay => 1,

            _ => 0,
        }
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
    partition_layout: DevicePartitionLayout,
    state_count: usize,
    observers: Vec<DefinitionObserver>,
}

impl DeviceDefinition {
    pub(crate) fn new_primitive(
        kind: PrimitiveElementKind,
        terminals: impl Into<SmallVec<[NodeId; 4]>>,
        param_constraints: impl Into<SmallVec<[ParameterConstraints; 1]>>,
        partition_layout: DevicePartitionLayout,
        observers: Vec<DefinitionObserver>,
    ) -> Self {
        Self {
            body: DeviceBody::Primitive(kind),
            terminals: terminals.into(),
            param_constraints: param_constraints.into(),
            partition_layout,
            state_count: kind.state_count(),
            observers,
        }
    }

    pub(crate) fn new_composite(
        circuit: Circuit,
        terminals: impl Into<SmallVec<[NodeId; 4]>>,
        param_constraints: impl Into<SmallVec<[ParameterConstraints; 1]>>,
        partition_layout: DevicePartitionLayout,
        state_count: usize,
        observers: Vec<DefinitionObserver>,
    ) -> Self {
        Self {
            body: DeviceBody::Composite(circuit),
            terminals: terminals.into(),
            param_constraints: param_constraints.into(),
            partition_layout,
            state_count,
            observers,
        }
    }

    #[inline]
    pub fn body(&self) -> &DeviceBody {
        &self.body
    }

    #[inline]
    pub fn terminals(&self) -> &[NodeId] {
        &self.terminals
    }

    #[inline]
    pub fn parameters(&self) -> &[ParameterConstraints] {
        &self.param_constraints
    }

    #[inline]
    pub const fn state_count(&self) -> usize {
        self.state_count
    }
    #[inline]
    pub fn terminal_partitions(&self) -> &[DevicePartitionId] {
        self.partition_layout.terminal_partitions()
    }

    #[inline]
    pub fn element_partitions(&self) -> &[DevicePartitionId] {
        self.partition_layout.element_partitions()
    }

    #[inline]
    pub fn partition_count(&self) -> usize {
        self.partition_layout.partition_count()
    }

    #[inline]
    pub(crate) fn exposed_partition_count(&self) -> usize {
        self.partition_layout.exposed_partition_count()
    }

    #[inline]
    pub fn observers(&self) -> &[DefinitionObserver] {
        &self.observers
    }

    #[inline]
    pub fn observer(&self, observer: DefinitionObserverId) -> Option<&DefinitionObserver> {
        self.observers.get(observer.index())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DevicePartitionLayout {
    terminal_partitions: SmallVec<[DevicePartitionId; 4]>,
    element_partitions: SmallVec<[DevicePartitionId; 4]>,
    partition_count: usize,
}

impl DevicePartitionLayout {
    pub(crate) fn try_new(
        terminal_partitions: impl Into<SmallVec<[DevicePartitionId; 4]>>,
    ) -> Option<Self> {
        let terminal_partitions = terminal_partitions.into();
        let partition_count = Self::canonical_exposed_partition_count(&terminal_partitions)?;

        Some(Self {
            terminal_partitions,
            element_partitions: SmallVec::new(),
            partition_count,
        })
    }

    #[inline]
    fn canonical_exposed_partition_count(
        terminal_partitions: &[DevicePartitionId],
    ) -> Option<usize> {
        let mut next_partition = 0usize;

        for &partition in terminal_partitions {
            let index = partition.index();

            if index > next_partition {
                return None;
            }

            if index == next_partition {
                next_partition += 1;
            }
        }

        Some(next_partition)
    }

    pub(crate) fn from_derived_parts(
        terminal_partitions: SmallVec<[DevicePartitionId; 4]>,
        element_partitions: SmallVec<[DevicePartitionId; 4]>,
        partition_count: usize,
    ) -> Self {
        debug_assert!(
            partition_count <= u16::MAX as usize + 1,
            "device partition count exceeds DevicePartitionId range",
        );

        debug_assert!(
            Self::canonical_exposed_partition_count(&terminal_partitions).is_some(),
            "terminal partition IDs must be canonical",
        );

        debug_assert!(
            terminal_partitions
                .iter()
                .chain(&element_partitions)
                .all(|partition| partition.index() < partition_count),
            "partition mapping contains an out of range partition",
        );

        #[cfg(debug_assertions)]
        {
            let mut seen = vec![false; partition_count];

            for &partition in terminal_partitions.iter().chain(&element_partitions) {
                seen[partition.index()] = true;
            }

            debug_assert!(
                seen.into_iter().all(|seen| seen),
                "every device partition must be represented",
            );
        }

        Self {
            terminal_partitions,
            element_partitions,
            partition_count,
        }
    }

    #[inline]
    pub fn terminal_partitions(&self) -> &[DevicePartitionId] {
        &self.terminal_partitions
    }

    #[inline]
    pub fn element_partitions(&self) -> &[DevicePartitionId] {
        &self.element_partitions
    }

    #[inline]
    pub fn exposed_partition_count(&self) -> usize {
        self.terminal_partitions
            .iter()
            .map(|partition| partition.index() + 1)
            .max()
            .unwrap_or(0)
    }

    #[inline]
    pub const fn partition_count(&self) -> usize {
        self.partition_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameter::ParameterConstraintError;

    fn invalid(
        index: usize,
        source: ParameterConstraintError,
    ) -> Result<(), PrimitiveParameterError> {
        Err(PrimitiveParameterError::InvalidParameter { index, source })
    }

    fn assert_primitive_observers(
        kind: PrimitiveElementKind,
        expected: &[(
            ObserverQuantity,
            DevicePartitionId,
            DefinitionObserverSource,
        )],
    ) {
        let definition = kind.definition();

        assert_eq!(definition.observers().len(), expected.len(), "{kind:?}",);

        for (index, &(quantity, partition, source)) in expected.iter().enumerate() {
            let observer = definition
                .observer(DefinitionObserverId::new(index as u32))
                .unwrap();

            assert_eq!(observer.quantity(), quantity, "{kind:?} observer {index}",);

            assert_eq!(observer.partition(), partition, "{kind:?} observer {index}",);

            assert_eq!(observer.source(), source, "{kind:?} observer {index}",);
        }

        assert!(
            definition
                .observer(DefinitionObserverId::new(expected.len() as u32,))
                .is_none(),
            "{kind:?}",
        );
    }

    #[test]
    fn primitive_observer_contract_is_stable() {
        let p0 = DevicePartitionId::new(0);
        let p1 = DevicePartitionId::new(1);

        let output_voltage = DefinitionObserverSource::Voltage {
            positive: NodeId::new(0),
            negative: NodeId::new(1),
        };

        let output_current = DefinitionObserverSource::Current {
            positive: NodeId::new(0),
            negative: NodeId::new(1),
        };

        for kind in [
            PrimitiveElementKind::Resistance,
            PrimitiveElementKind::Conductance,
            PrimitiveElementKind::VoltageSource,
            PrimitiveElementKind::CurrentSource,
            PrimitiveElementKind::Capacitor,
            PrimitiveElementKind::Inductor,
            PrimitiveElementKind::Diode,
        ] {
            assert_primitive_observers(
                kind,
                &[
                    (ObserverQuantity::Voltage, p0, output_voltage),
                    (ObserverQuantity::Current, p0, output_current),
                ],
            );
        }

        let differential_control_voltage = DefinitionObserverSource::Voltage {
            positive: NodeId::new(2),
            negative: NodeId::new(3),
        };

        for kind in [
            PrimitiveElementKind::VoltageControlledCurrentSource,
            PrimitiveElementKind::VoltageControlledVoltageSource,
            PrimitiveElementKind::VoltageControlledSwitch,
        ] {
            assert_primitive_observers(
                kind,
                &[
                    (ObserverQuantity::Voltage, p0, output_voltage),
                    (ObserverQuantity::Voltage, p0, differential_control_voltage),
                    (ObserverQuantity::Current, p0, output_current),
                ],
            );
        }

        assert_primitive_observers(
            PrimitiveElementKind::VoltageControlledConductance,
            &[
                (ObserverQuantity::Voltage, p0, output_voltage),
                (
                    ObserverQuantity::Voltage,
                    p0,
                    DefinitionObserverSource::Voltage {
                        positive: NodeId::new(2),
                        negative: NodeId::new(1),
                    },
                ),
                (ObserverQuantity::Current, p0, output_current),
            ],
        );

        assert_primitive_observers(
            PrimitiveElementKind::Not,
            &[
                (
                    ObserverQuantity::Voltage,
                    p0,
                    DefinitionObserverSource::Voltage {
                        positive: NodeId::new(0),
                        negative: NodeId::new(2),
                    },
                ),
                (
                    ObserverQuantity::Voltage,
                    p0,
                    DefinitionObserverSource::Voltage {
                        positive: NodeId::new(3),
                        negative: NodeId::new(2),
                    },
                ),
                (
                    ObserverQuantity::Current,
                    p0,
                    DefinitionObserverSource::Current {
                        positive: NodeId::new(1),
                        negative: NodeId::new(0),
                    },
                ),
            ],
        );

        let input_b_voltage = DefinitionObserverSource::Voltage {
            positive: NodeId::new(4),
            negative: NodeId::new(2),
        };

        for kind in [
            PrimitiveElementKind::And2,
            PrimitiveElementKind::Nand2,
            PrimitiveElementKind::Or2,
            PrimitiveElementKind::Nor2,
        ] {
            assert_primitive_observers(
                kind,
                &[
                    (
                        ObserverQuantity::Voltage,
                        p0,
                        DefinitionObserverSource::Voltage {
                            positive: NodeId::new(0),
                            negative: NodeId::new(2),
                        },
                    ),
                    (
                        ObserverQuantity::Voltage,
                        p0,
                        DefinitionObserverSource::Voltage {
                            positive: NodeId::new(3),
                            negative: NodeId::new(2),
                        },
                    ),
                    (ObserverQuantity::Voltage, p0, input_b_voltage),
                    (
                        ObserverQuantity::Current,
                        p0,
                        DefinitionObserverSource::Current {
                            positive: NodeId::new(1),
                            negative: NodeId::new(0),
                        },
                    ),
                ],
            );
        }

        assert_primitive_observers(
            PrimitiveElementKind::TickDelay,
            &[
                (
                    ObserverQuantity::Voltage,
                    p0,
                    DefinitionObserverSource::Voltage {
                        positive: NodeId::new(0),
                        negative: NodeId::new(1),
                    },
                ),
                (
                    ObserverQuantity::Voltage,
                    p1,
                    DefinitionObserverSource::Voltage {
                        positive: NodeId::new(2),
                        negative: NodeId::new(3),
                    },
                ),
                (
                    ObserverQuantity::Current,
                    p1,
                    DefinitionObserverSource::Current {
                        positive: NodeId::new(2),
                        negative: NodeId::new(3),
                    },
                ),
            ],
        );
    }

    #[test]
    fn primitive_scalar_parameter_boundaries_match_contract() {
        let cases: &[(
            PrimitiveElementKind,
            &[f64],
            Result<(), PrimitiveParameterError>,
        )] = &[
            (PrimitiveElementKind::Resistance, &[1.0], Ok(())),
            (
                PrimitiveElementKind::Resistance,
                &[0.0],
                invalid(0, ParameterConstraintError::OutOfRange),
            ),
            (PrimitiveElementKind::Conductance, &[0.0], Ok(())),
            (
                PrimitiveElementKind::Conductance,
                &[-1.0],
                invalid(0, ParameterConstraintError::OutOfRange),
            ),
            (PrimitiveElementKind::Diode, &[1.0, 0.0], Ok(())),
            (
                PrimitiveElementKind::Diode,
                &[0.0, 0.0],
                invalid(0, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::Diode,
                &[1.0, -1.0],
                invalid(1, ParameterConstraintError::OutOfRange),
            ),
            (PrimitiveElementKind::Not, &[0.0, 1.0, 0.0], Ok(())),
            (
                PrimitiveElementKind::Not,
                &[0.0, 0.0, 0.0],
                invalid(1, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::Not,
                &[0.0, 1.0, -1.0],
                invalid(2, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::Capacitor,
                &[0.0],
                invalid(0, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::Inductor,
                &[0.0],
                invalid(0, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::VoltageControlledSwitch,
                &[0.0, 0.0, 1.0, 0.0],
                Ok(()),
            ),
            (
                PrimitiveElementKind::VoltageControlledSwitch,
                &[0.0, -1.0, 1.0, 0.0],
                invalid(1, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::VoltageControlledSwitch,
                &[0.0, 0.0, 0.0, 0.0],
                invalid(2, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::VoltageControlledSwitch,
                &[0.0, 0.0, 1.0, -1.0],
                invalid(3, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::VoltageControlledConductance,
                &[0.0, 1.0, 0.0, 1.0],
                Ok(()),
            ),
            (
                PrimitiveElementKind::VoltageControlledConductance,
                &[0.0, 0.0, 0.0, 1.0],
                invalid(1, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::VoltageControlledConductance,
                &[0.0, 1.0, -1.0, 1.0],
                invalid(2, ParameterConstraintError::OutOfRange),
            ),
            (
                PrimitiveElementKind::VoltageControlledConductance,
                &[0.0, 1.0, 0.0, 0.0],
                invalid(3, ParameterConstraintError::OutOfRange),
            ),
        ];

        for &(kind, parameters, ref expected) in cases {
            assert_eq!(kind.validate_parameters(parameters), *expected, "{kind:?}");
        }
    }

    #[test]
    fn primitive_empty_parameters_report_wrong_count() {
        for kind in PrimitiveElementKind::ALL {
            assert_eq!(
                kind.validate_parameters(&[]),
                Err(PrimitiveParameterError::WrongParameterCount {
                    expected: kind.parameter_count(),
                    actual: 0,
                }),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn nonlinear_conductances_require_gmax_above_gmin() {
        let cases = [
            (
                PrimitiveElementKind::VoltageControlledSwitch,
                &[0.0, 0.0, 1.0, 1.0][..],
                2,
                3,
            ),
            (
                PrimitiveElementKind::VoltageControlledConductance,
                &[0.0, 1.0, 1.0, 1.0][..],
                3,
                2,
            ),
            (PrimitiveElementKind::Diode, &[1.0, 1.0][..], 0, 1),
        ];

        for (kind, parameters, greater, lesser) in cases {
            assert_eq!(
                kind.validate_parameters(parameters),
                Err(PrimitiveParameterError::ParameterMustBeGreater { greater, lesser }),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn primitive_discriminants_are_dense() {
        for (index, kind) in PrimitiveElementKind::ALL.into_iter().enumerate() {
            assert_eq!(kind as usize, index);
        }

        assert_eq!(
            PrimitiveElementKind::COUNT,
            PrimitiveElementKind::Nor2 as u32 + 1,
        );
    }

    #[test]
    fn logic_gate_terminal_and_state_contracts_match() {
        assert_eq!(PrimitiveElementKind::Not.definition().terminals().len(), 4);

        for kind in [
            PrimitiveElementKind::And2,
            PrimitiveElementKind::Nand2,
            PrimitiveElementKind::Or2,
            PrimitiveElementKind::Nor2,
        ] {
            assert_eq!(kind.definition().terminals().len(), 5, "{kind:?}");
        }

        for kind in [
            PrimitiveElementKind::Not,
            PrimitiveElementKind::And2,
            PrimitiveElementKind::Nand2,
            PrimitiveElementKind::Or2,
            PrimitiveElementKind::Nor2,
        ] {
            assert_eq!(kind.parameter_count(), 3, "{kind:?}");
            assert_eq!(kind.state_count(), 0, "{kind:?}");
        }
    }

    #[test]
    fn terminal_partition_layout_requires_canonical_dense_ids() {
        let valid = [
            vec![],
            vec![0],
            vec![0, 0],
            vec![0, 1],
            vec![0, 1, 0],
            vec![0, 0, 1, 1],
            vec![0, 1, 0, 2],
        ];

        for partitions in valid {
            let partitions = partitions
                .into_iter()
                .map(DevicePartitionId::new)
                .collect::<SmallVec<[DevicePartitionId; 4]>>();

            assert!(DevicePartitionLayout::try_new(partitions).is_some());
        }

        let invalid = [vec![1], vec![0, 2], vec![0, 1, 3], vec![0, 2, 1]];

        for partitions in invalid {
            let partitions = partitions
                .into_iter()
                .map(DevicePartitionId::new)
                .collect::<SmallVec<[DevicePartitionId; 4]>>();

            assert!(DevicePartitionLayout::try_new(partitions).is_none());
        }
    }

    #[test]
    fn terminal_partition_layout_tracks_partition_count() {
        let layout = DevicePartitionLayout::try_new(smallvec![
            DevicePartitionId::new(0),
            DevicePartitionId::new(1),
            DevicePartitionId::new(0),
            DevicePartitionId::new(2),
        ])
        .unwrap();

        assert_eq!(layout.partition_count(), 3);
        assert_eq!(
            layout.terminal_partitions(),
            &[
                DevicePartitionId::new(0),
                DevicePartitionId::new(1),
                DevicePartitionId::new(0),
                DevicePartitionId::new(2),
            ]
        );
    }

    #[test]
    fn primitive_state_counts_match_contract() {
        for kind in PrimitiveElementKind::ALL {
            let expected = match kind {
                PrimitiveElementKind::Capacitor
                | PrimitiveElementKind::Inductor
                | PrimitiveElementKind::TickDelay
                | PrimitiveElementKind::VoltageControlledSwitch => 1,

                _ => 0,
            };

            assert_eq!(kind.state_count(), expected, "{kind:?}");
            assert_eq!(kind.definition().state_count(), expected, "{kind:?}");
        }
    }
}
