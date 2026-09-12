use crate::circuit::NodeId;
use crate::device::definition::{DefinitionId, DeviceBody, DeviceDefinition, PrimitiveElementKind};
use crate::parameter::{Bound, ParameterConstraints};
use thiserror::Error;

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

pub struct DefinitionRegistry {
    definitions: Vec<DeviceDefinition>,
}

impl Default for DefinitionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DefinitionRegistry {
    pub const COMPOSITE_DEFINITION_ID_BASE: u32 = 7;

    pub fn new() -> Self {
        let positive = ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            None,
            false,
            None,
        );
        let unrestricted = ParameterConstraints::default();

        Self {
            definitions: vec![
                DeviceDefinition::new_primitive(
                    PrimitiveElementKind::Admittance,
                    vec![NodeId::new(0), NodeId::new(1)],
                    vec![positive],
                ),
                DeviceDefinition::new_primitive(
                    PrimitiveElementKind::Impedance,
                    vec![NodeId::new(0), NodeId::new(1)],
                    vec![positive],
                ),
                DeviceDefinition::new_primitive(
                    PrimitiveElementKind::AcrossSource,
                    vec![NodeId::new(0), NodeId::new(1)],
                    vec![unrestricted],
                ),
                DeviceDefinition::new_primitive(
                    PrimitiveElementKind::ThroughSource,
                    vec![NodeId::new(0), NodeId::new(1)],
                    vec![unrestricted],
                ),
                DeviceDefinition::new_primitive(
                    PrimitiveElementKind::ControlledThroughSource,
                    vec![
                        NodeId::new(0),
                        NodeId::new(1),
                        NodeId::new(2),
                        NodeId::new(3),
                    ],
                    vec![unrestricted],
                ),
                DeviceDefinition::new_primitive(
                    PrimitiveElementKind::ControlledAcrossSource,
                    vec![
                        NodeId::new(0),
                        NodeId::new(1),
                        NodeId::new(2),
                        NodeId::new(3),
                    ],
                    vec![unrestricted],
                ),
            ],
        }
    }

    pub fn register(
        &mut self,
        definition: DeviceDefinition,
    ) -> Result<DefinitionId, RegisterDeviceError> {
        let next = self
            .definitions
            .len()
            .checked_add(1)
            .ok_or(RegisterDeviceError::DefinitionIdExhausted)?;

        let raw = u32::try_from(next).map_err(|_| RegisterDeviceError::DefinitionIdExhausted)?;
        let id =
            DefinitionId::try_from(raw).map_err(|_| RegisterDeviceError::DefinitionIdExhausted)?;

        match definition.body() {
            DeviceBody::Primitive(kind) => {
                return Err(RegisterDeviceError::PrimitiveRegistrationForbidden { kind: *kind });
            }
            DeviceBody::Composite(circuit) => {
                let terminal_count = definition.terminals().len() as u32;
                if terminal_count > circuit.node_count() {
                    return Err(RegisterDeviceError::TerminalCountExceedsNodeCount {
                        terminal_count,
                        node_count: circuit.node_count(),
                    });
                }

                if let Some(definition) = circuit.elements().iter().find_map(|element| {
                    self.get(element.definition())
                        .is_none()
                        .then_some(element.definition())
                }) {
                    return Err(RegisterDeviceError::UnknownDefinition { definition });
                }
            }
        }

        self.definitions.push(definition);

        Ok(id)
    }

    pub fn get(&self, id: DefinitionId) -> Option<&DeviceDefinition> {
        self.definitions.get(id.id().get() as usize - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::{DefinitionRegistry, PrimitiveElementKind, RegisterDeviceError};
    use crate::circuit::Circuit;
    use crate::circuit::NodeId;
    use crate::device::definition::{DefinitionId, DeviceBody, DeviceDefinition};

    fn composite_definition() -> DeviceDefinition {
        DeviceDefinition::new_composite(
            Circuit::new(2, Vec::new()),
            vec![NodeId::new(0), NodeId::new(1)],
            Vec::new(),
        )
    }

    #[test]
    fn primitive_definition_ids_are_one_based() {
        let expected = [
            (PrimitiveElementKind::Admittance, 1),
            (PrimitiveElementKind::Impedance, 2),
            (PrimitiveElementKind::AcrossSource, 3),
            (PrimitiveElementKind::ThroughSource, 4),
            (PrimitiveElementKind::ControlledThroughSource, 5),
            (PrimitiveElementKind::ControlledAcrossSource, 6),
        ];

        for (kind, raw) in expected {
            assert_eq!(DefinitionId::from(kind).get(), raw);
        }
    }

    #[test]
    fn registered_composites_receive_sequential_ids() {
        let mut registry = DefinitionRegistry::new();

        let first = registry.register(composite_definition()).unwrap();
        let second = registry.register(composite_definition()).unwrap();

        assert_eq!(first.id().get(), 7);
        assert_eq!(second.id().get(), 8);
        assert!(registry.get(first).is_some());
        assert!(registry.get(second).is_some());
    }

    #[test]
    fn rejected_registration_does_not_consume_an_id() {
        let mut registry = DefinitionRegistry::new();
        let primitive = registry
            .get(DefinitionId::from(PrimitiveElementKind::Admittance))
            .unwrap()
            .clone();

        assert_eq!(
            registry.register(primitive),
            Err(RegisterDeviceError::PrimitiveRegistrationForbidden {
                kind: PrimitiveElementKind::Admittance,
            })
        );

        let first = registry.register(composite_definition()).unwrap();

        assert_eq!(
            first.id().get(),
            DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE
        );
        assert!(matches!(
            registry.get(first).map(|definition| definition.body()),
            Some(DeviceBody::Composite(_))
        ));
    }
}
