use crate::circuits::{Circuit, CircuitBuilderError, Element, ValueRef};
use crate::devices::DeviceDefinition;
use crate::devices::registry::DefinitionRegistry;
use crate::{NodeId, ParameterConstraints, ParameterId, define_id};

define_id!(ElementId);

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
    pub fn add_node(&mut self) -> Result<NodeId, CircuitBuilderError> {
        let result = Ok(NodeId::new(self.node_count));
        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(CircuitBuilderError::NodeIdExhausted)?;
        result
    }

    #[inline]
    pub fn add_terminal(&mut self) -> Result<NodeId, CircuitBuilderError> {
        let node_id = NodeId::new(self.node_count);
        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(CircuitBuilderError::NodeIdExhausted)?;
        self.terminals.push(node_id);
        Ok(node_id)
    }

    #[inline]
    pub fn add_parameter(
        &mut self,
        parameter: ParameterConstraints,
    ) -> Result<ParameterId, CircuitBuilderError> {
        if self.param_constraints.len() > u32::MAX as usize {
            return Err(CircuitBuilderError::ParameterIdExhausted);
        }

        let result = Ok(ParameterId::new(self.param_constraints.len() as u32));
        self.param_constraints.push(parameter);

        result
    }

    #[inline]
    pub fn add_element(&mut self, element: Element) -> Result<ElementId, CircuitBuilderError> {
        self.validate_element(&element)?;
        let id = ElementId(self.elements.len() as u32);
        self.elements.push(element);
        Ok(id)
    }

    fn validate_element(&self, element: &Element) -> Result<(), CircuitBuilderError> {
        let definition_id = element.definition();

        let definition = self
            .registry
            .get(definition_id)
            .ok_or(CircuitBuilderError::UnknownDefinition { definition_id })?;

        let def_nodes = definition.terminals();
        let elem_nodes = element.terminals();

        if elem_nodes.len() != def_nodes.len() {
            return Err(CircuitBuilderError::TerminalCountMismatch {
                definition_id,
                expected: def_nodes.to_vec(),
                actual: elem_nodes.to_vec(),
            });
        }

        let def_constr = definition.parameters();
        let elem_params = element.parameters();

        if def_constr.len() != elem_params.len() {
            return Err(CircuitBuilderError::ParameterCountMismatch {
                definition_id,
                expected: def_constr.len(),
                actual: elem_params.len(),
            });
        }

        for (terminal_index, &node) in element.terminals().iter().enumerate() {
            if node.id() >= self.node_count {
                return Err(CircuitBuilderError::NodeOutOfRange {
                    terminal_index,
                    node,
                    node_count: self.node_count,
                });
            }
        }

        for (parameter_index, (val, constraint)) in elem_params.iter().zip(def_constr).enumerate() {
            match val {
                ValueRef::Literal(literal) => {
                    constraint.validate(*literal).map_err(|source| {
                        CircuitBuilderError::ParameterConstraint {
                            parameter_index,
                            source,
                        }
                    })?;
                }
                ValueRef::Parameter(parameter_id)
                    if parameter_id.index() >= self.param_constraints.len() =>
                {
                    return Err(CircuitBuilderError::ParameterOutOfRange {
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
