use crate::devices::{
    DefinitionId, DeviceBody, DeviceDefinition, PrimitiveElementKind, RegisterDeviceError,
};
use crate::{Bound, NodeId, ParameterConstraints};

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

    pub fn new() -> DefinitionRegistry {
        let positive = ParameterConstraints {
            lower: Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            upper: None,
            non_zero: false,
            reciprocal_range: None,
        };

        let unrestricted = ParameterConstraints {
            lower: None,
            upper: None,
            non_zero: false,
            reciprocal_range: None,
        };

        Self {
            definitions: vec![
                DeviceDefinition {
                    body: DeviceBody::Primitive(PrimitiveElementKind::Admittance),
                    terminals: vec![NodeId(0), NodeId(1)].into(),
                    param_constraints: vec![positive].into(),
                },
                DeviceDefinition {
                    body: DeviceBody::Primitive(PrimitiveElementKind::Impedance),
                    terminals: vec![NodeId(0), NodeId(1)].into(),
                    param_constraints: vec![positive].into(),
                },
                DeviceDefinition {
                    body: DeviceBody::Primitive(PrimitiveElementKind::AcrossSource),
                    terminals: vec![NodeId(0), NodeId(1)].into(),
                    param_constraints: vec![unrestricted].into(),
                },
                DeviceDefinition {
                    body: DeviceBody::Primitive(PrimitiveElementKind::ThroughSource),
                    terminals: vec![NodeId(0), NodeId(1)].into(),
                    param_constraints: vec![unrestricted].into(),
                },
                DeviceDefinition {
                    body: DeviceBody::Primitive(PrimitiveElementKind::ControlledThroughSource),
                    terminals: vec![NodeId(0), NodeId(1), NodeId(2), NodeId(3)].into(),
                    param_constraints: vec![unrestricted].into(),
                },
                DeviceDefinition {
                    body: DeviceBody::Primitive(PrimitiveElementKind::ControlledAcrossSource),
                    terminals: vec![NodeId(0), NodeId(1), NodeId(2), NodeId(3)].into(),
                    param_constraints: vec![unrestricted].into(),
                },
            ],
        }
    }

    pub fn register(
        &mut self,
        id: DefinitionId,
        definition: DeviceDefinition,
    ) -> Result<DefinitionId, RegisterDeviceError> {
        if self.definitions.len() >= u32::MAX as usize {
            return Err(RegisterDeviceError::DefinitionIdExhausted);
        }

        match definition.body {
            DeviceBody::Primitive(kind) => {
                return Err(RegisterDeviceError::PrimitiveRegistrationForbidden { kind });
            }
            DeviceBody::Composite(ref circuit) => {
                if circuit.node_count() < definition.terminals.len() as u32 {
                    return Err(RegisterDeviceError::TerminalCountExceedsNodeCount {
                        terminal_count: definition.terminals.len() as u32,
                        node_count: circuit.node_count(),
                    });
                }

                if let Some(definition) = circuit.elements().iter().find_map(|e| {
                    if self.get(e.definition()).is_none() {
                        Some(e.definition())
                    } else {
                        None
                    }
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
