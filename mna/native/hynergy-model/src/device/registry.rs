use crate::device::definition::{DefinitionId, DeviceBody, DeviceDefinition, PrimitiveElementKind};
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
    pub const COMPOSITE_DEFINITION_ID_BASE: u32 = PrimitiveElementKind::COUNT + 1;

    pub fn new() -> Self {
        Self {
            definitions: PrimitiveElementKind::ALL
                .into_iter()
                .map(PrimitiveElementKind::definition)
                .collect(),
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
    use crate::device::definition::{
        DefinitionId, DeviceBody, DeviceDefinition, TerminalPartitionLayout,
    };

    fn composite_definition() -> DeviceDefinition {
        DeviceDefinition::new_composite(
            Circuit::new(2, Vec::new()),
            vec![0.into(), 1.into()],
            Vec::new(),
            TerminalPartitionLayout::try_new(vec![0.into(), 0.into()]).unwrap(),
            0,
        )
    }

    #[test]
    fn primitive_definitions_are_registered_under_their_kind_ids() {
        let registry = DefinitionRegistry::new();

        for (index, kind) in PrimitiveElementKind::ALL.into_iter().enumerate() {
            let expected = (index + 1) as u32;
            let id = DefinitionId::from(kind);

            assert_eq!(id.get(), expected);

            assert!(matches!(
                registry.get(id).map(DeviceDefinition::body),
                Some(DeviceBody::Primitive(actual))
                    if *actual == kind
            ));
        }
    }

    #[test]
    fn composite_ids_start_after_primitives_and_are_sequential() {
        let mut registry = DefinitionRegistry::new();

        assert_eq!(
            DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE,
            PrimitiveElementKind::COUNT + 1,
        );

        let first = registry.register(composite_definition()).unwrap();
        let second = registry.register(composite_definition()).unwrap();

        assert_eq!(
            first.get(),
            DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE
        );
        assert_eq!(second.get(), first.get() + 1);

        assert!(registry.get(first).is_some());
        assert!(registry.get(second).is_some());
    }

    #[test]
    fn rejected_registration_does_not_consume_an_id() {
        let mut registry = DefinitionRegistry::new();

        let primitive = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap()
            .clone();

        assert_eq!(
            registry.register(primitive),
            Err(RegisterDeviceError::PrimitiveRegistrationForbidden {
                kind: PrimitiveElementKind::Conductance,
            })
        );

        let first = registry.register(composite_definition()).unwrap();

        assert_eq!(
            first.get(),
            DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE
        );

        assert!(matches!(
            registry.get(first).map(DeviceDefinition::body),
            Some(DeviceBody::Composite(_))
        ));
    }
}
