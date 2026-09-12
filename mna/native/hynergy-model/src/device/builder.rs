use crate::circuit::{Circuit, Element, ElementId, NodeId, ValueRef};
use crate::device::definition::{DefinitionId, DeviceDefinition};
use crate::device::registry::DefinitionRegistry;
use crate::parameter::{ParameterConstraintError, ParameterConstraints, ParameterId};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DeviceDefinitionBuilderError {
    #[error("unknown device {definition_id:?}")]
    UnknownDefinition { definition_id: DefinitionId },

    #[error(
        "element has {actual:#?} terminals, \
         but device {definition_id:?} requires {expected:#?}"
    )]
    TerminalCountMismatch {
        definition_id: DefinitionId,
        expected: Vec<NodeId>,
        actual: Vec<NodeId>,
    },

    #[error(
        "element has {actual} parameters, \
         but device {definition_id:?} requires {expected}"
    )]
    ParameterCountMismatch {
        definition_id: DefinitionId,
        expected: usize,
        actual: usize,
    },

    #[error(
        "terminal {terminal_index} of element \
         references out-of-range node {node:?}"
    )]
    NodeOutOfRange {
        terminal_index: usize,
        node: NodeId,
        node_count: u32,
    },

    #[error(
        "parameter {parameter_index} of element \
         references out-of-range definition parameter {parameter:?}"
    )]
    ParameterOutOfRange {
        parameter_index: usize,
        parameter: ParameterId,
        parameter_count: usize,
    },

    #[error("parameter {parameter_index} violates its constraints: {source}")]
    ParameterConstraint {
        parameter_index: usize,
        #[source]
        source: ParameterConstraintError,
    },

    #[error("exhausted NodeId range")]
    NodeIdExhausted,

    #[error("exhausted ParameterId range")]
    ParameterIdExhausted,

    #[error("exhausted ElementId range")]
    ElementIdExhausted,
}

pub struct DeviceDefinitionBuilder<'a> {
    registry: &'a DefinitionRegistry,
    terminals: Vec<NodeId>,
    param_constraints: Vec<ParameterConstraints>,
    node_count: u32,
    elements: Vec<Element>,
}

impl<'a> DeviceDefinitionBuilder<'a> {
    pub fn new(definition_registry: &'a DefinitionRegistry) -> Self {
        Self {
            registry: definition_registry,
            terminals: Vec::new(),
            param_constraints: Vec::new(),
            node_count: 0,
            elements: Vec::new(),
        }
    }

    #[inline]
    pub fn add_node(&mut self) -> Result<NodeId, DeviceDefinitionBuilderError> {
        let result = Ok(NodeId::new(self.node_count));
        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(DeviceDefinitionBuilderError::NodeIdExhausted)?;
        result
    }

    #[inline]
    pub fn add_terminal(&mut self) -> Result<NodeId, DeviceDefinitionBuilderError> {
        let node_id = NodeId::new(self.node_count);
        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(DeviceDefinitionBuilderError::NodeIdExhausted)?;
        self.terminals.push(node_id);
        Ok(node_id)
    }

    #[inline]
    pub fn add_parameter(
        &mut self,
        parameter: ParameterConstraints,
    ) -> Result<ParameterId, DeviceDefinitionBuilderError> {
        let id = ParameterId::new(
            u32::try_from(self.param_constraints.len())
                .map_err(|_| DeviceDefinitionBuilderError::ParameterIdExhausted)?,
        );
        self.param_constraints.push(parameter);
        Ok(id)
    }

    #[inline]
    pub fn add_element(
        &mut self,
        element: Element,
    ) -> Result<ElementId, DeviceDefinitionBuilderError> {
        self.validate_element(&element)?;
        let id = ElementId::new(
            u32::try_from(self.elements.len())
                .map_err(|_| DeviceDefinitionBuilderError::ElementIdExhausted)?,
        );
        self.elements.push(element);
        Ok(id)
    }

    fn validate_element(&self, element: &Element) -> Result<(), DeviceDefinitionBuilderError> {
        let definition_id = element.definition();

        let definition = self
            .registry
            .get(definition_id)
            .ok_or(DeviceDefinitionBuilderError::UnknownDefinition { definition_id })?;

        let def_nodes = definition.terminals();
        let elem_nodes = element.terminals();

        if elem_nodes.len() != def_nodes.len() {
            return Err(DeviceDefinitionBuilderError::TerminalCountMismatch {
                definition_id,
                expected: def_nodes.to_vec(),
                actual: elem_nodes.to_vec(),
            });
        }

        let def_constr = definition.parameters();
        let elem_params = element.parameters();

        if def_constr.len() != elem_params.len() {
            return Err(DeviceDefinitionBuilderError::ParameterCountMismatch {
                definition_id,
                expected: def_constr.len(),
                actual: elem_params.len(),
            });
        }

        for (terminal_index, &node) in element.terminals().iter().enumerate() {
            if node.id() >= self.node_count {
                return Err(DeviceDefinitionBuilderError::NodeOutOfRange {
                    terminal_index,
                    node,
                    node_count: self.node_count,
                });
            }
        }

        for (parameter_index, (value, constraint)) in elem_params.iter().zip(def_constr).enumerate()
        {
            match value {
                ValueRef::Literal(literal) => {
                    constraint.validate(*literal).map_err(|source| {
                        DeviceDefinitionBuilderError::ParameterConstraint {
                            parameter_index,
                            source,
                        }
                    })?;
                }
                ValueRef::Parameter(parameter_id)
                    if parameter_id.index() >= self.param_constraints.len() =>
                {
                    return Err(DeviceDefinitionBuilderError::ParameterOutOfRange {
                        parameter_index,
                        parameter: *parameter_id,
                        parameter_count: self.param_constraints.len(),
                    });
                }
                _ => continue,
            }
        }

        Ok(())
    }

    #[inline]
    pub fn build_definition(self) -> DeviceDefinition {
        DeviceDefinition::new_composite(
            Circuit::new(self.node_count, self.elements),
            self.terminals,
            self.param_constraints,
        )
    }
}
