use crate::circuit::{Circuit, Element, ElementId, NodeId, ValueRef};
use crate::device::definition::{
    DefinitionId, DeviceBody, DeviceDefinition, PrimitiveParameterError, TerminalPartitionId,
};
use crate::device::registry::DefinitionRegistry;
use crate::parameter::{ParameterConstraintError, ParameterConstraints, ParameterId};
use smallvec::SmallVec;
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

    #[error("primitive {definition_id:?} has invalid parameter values: {source}")]
    PrimitiveParameters {
        definition_id: DefinitionId,

        #[source]
        source: PrimitiveParameterError,
    },

    #[error("internal node {node:?} is unused")]
    UnusedInternalNode { node: NodeId },

    #[error(
        "instantaneous internal circuit containing node {node:?} \
         is not connected to any exposed terminal"
    )]
    InternalComponentWithoutTerminal { node: NodeId },

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
        let node = NodeId::new(self.node_count);

        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(DeviceDefinitionBuilderError::NodeIdExhausted)?;

        Ok(node)
    }

    #[inline]
    pub fn add_terminal(&mut self) -> Result<NodeId, DeviceDefinitionBuilderError> {
        let node = NodeId::new(self.node_count);

        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(DeviceDefinitionBuilderError::NodeIdExhausted)?;

        self.terminals.push(node);

        Ok(node)
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

        let definition_terminals = definition.terminals();
        let element_terminals = element.terminals();

        if element_terminals.len() != definition_terminals.len() {
            return Err(DeviceDefinitionBuilderError::TerminalCountMismatch {
                definition_id,
                expected: definition_terminals.to_vec(),
                actual: element_terminals.to_vec(),
            });
        }

        let definition_constraints = definition.parameters();
        let element_parameters = element.parameters();

        if element_parameters.len() != definition_constraints.len() {
            return Err(DeviceDefinitionBuilderError::ParameterCountMismatch {
                definition_id,
                expected: definition_constraints.len(),
                actual: element_parameters.len(),
            });
        }

        for (terminal_index, &node) in element_terminals.iter().enumerate() {
            if node.id() >= self.node_count {
                return Err(DeviceDefinitionBuilderError::NodeOutOfRange {
                    terminal_index,
                    node,
                    node_count: self.node_count,
                });
            }
        }

        for (parameter_index, (value, constraint)) in element_parameters
            .iter()
            .zip(definition_constraints)
            .enumerate()
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

                ValueRef::Parameter(_) => {}
            }
        }

        if let DeviceBody::Primitive(kind) = definition.body() {
            let mut parameters = SmallVec::<[f64; 4]>::with_capacity(element_parameters.len());

            let mut all_literal = true;

            for parameter in element_parameters {
                match parameter {
                    ValueRef::Literal(value) => parameters.push(*value),
                    ValueRef::Parameter(_) => {
                        all_literal = false;
                        break;
                    }
                }
            }

            if all_literal {
                kind.validate_parameters(&parameters).map_err(|source| {
                    DeviceDefinitionBuilderError::PrimitiveParameters {
                        definition_id,
                        source,
                    }
                })?;
            }
        }

        Ok(())
    }

    pub fn build_definition(self) -> Result<DeviceDefinition, DeviceDefinitionBuilderError> {
        let terminal_partitions = self.derive_terminal_partitions()?;

        Ok(DeviceDefinition::new_composite(
            Circuit::new(self.node_count, self.elements),
            self.terminals,
            self.param_constraints,
            terminal_partitions,
        ))
    }

    fn derive_terminal_partitions(
        &self,
    ) -> Result<SmallVec<[TerminalPartitionId; 4]>, DeviceDefinitionBuilderError> {
        let node_count = self.node_count as usize;

        let mut union_find = UnionFind::new(node_count);
        let mut referenced = vec![false; node_count];

        for element in &self.elements {
            let definition = self
                .registry
                .get(element.definition())
                .expect("elements are validated before insertion");

            let element_terminals = element.terminals();
            let partitions = definition.terminal_partitions();

            debug_assert_eq!(
                element_terminals.len(),
                partitions.len(),
                "registered definition must have one partition per terminal"
            );

            for &node in element_terminals {
                referenced[node.id() as usize] = true;
            }

            for current in 1..element_terminals.len() {
                if let Some(previous) =
                    (0..current).find(|&previous| partitions[previous] == partitions[current])
                {
                    union_find.union(
                        element_terminals[current].id() as usize,
                        element_terminals[previous].id() as usize,
                    );
                }
            }
        }

        let mut exposed_node = vec![false; node_count];

        for &terminal in &self.terminals {
            exposed_node[terminal.id() as usize] = true;
        }

        let mut externally_reachable_component = vec![false; node_count];

        for &terminal in &self.terminals {
            let root = union_find.find(terminal.id() as usize);
            externally_reachable_component[root] = true;
        }

        for node_index in 0..node_count {
            if !exposed_node[node_index] && !referenced[node_index] {
                return Err(DeviceDefinitionBuilderError::UnusedInternalNode {
                    node: NodeId::new(node_index as u32),
                });
            }
        }

        for (node_index, &is_referenced) in referenced.iter().enumerate() {
            if !is_referenced {
                continue;
            }

            let root = union_find.find(node_index);

            if !externally_reachable_component[root] {
                return Err(
                    DeviceDefinitionBuilderError::InternalComponentWithoutTerminal {
                        node: NodeId::new(node_index as u32),
                    },
                );
            }
        }

        let mut root_partitions = vec![None; node_count];
        let mut terminal_partitions =
            SmallVec::<[TerminalPartitionId; 4]>::with_capacity(self.terminals.len());

        let mut next_partition = 0u32;

        for &terminal in &self.terminals {
            let root = union_find.find(terminal.id() as usize);

            let partition = match root_partitions[root] {
                Some(partition) => partition,

                None => {
                    let partition = TerminalPartitionId::new(next_partition as u16);
                    next_partition += 1;

                    root_partitions[root] = Some(partition);

                    partition
                }
            };

            terminal_partitions.push(partition);
        }

        Ok(terminal_partitions)
    }
}

struct UnionFind {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl UnionFind {
    fn new(len: usize) -> Self {
        Self {
            parents: (0..len).collect(),
            ranks: vec![0; len],
        }
    }

    fn find(&mut self, mut node: usize) -> usize {
        let mut root = node;

        while self.parents[root] != root {
            root = self.parents[root];
        }

        while self.parents[node] != node {
            let parent = self.parents[node];
            self.parents[node] = root;
            node = parent;
        }

        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let mut root_a = self.find(a);
        let mut root_b = self.find(b);

        if root_a == root_b {
            return;
        }

        if self.ranks[root_a] < self.ranks[root_b] {
            std::mem::swap(&mut root_a, &mut root_b);
        }

        self.parents[root_b] = root_a;

        if self.ranks[root_a] == self.ranks[root_b] {
            self.ranks[root_a] += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceDefinitionBuilder, DeviceDefinitionBuilderError};
    use crate::circuit::{Element, ValueRef};
    use crate::device::definition::{DefinitionId, PrimitiveElementKind, TerminalPartitionId};
    use crate::device::registry::DefinitionRegistry;

    #[test]
    fn derives_single_partition_through_series_elements() {
        let registry = DefinitionRegistry::new();
        let mut builder = DeviceDefinitionBuilder::new(&registry);

        let a = builder.add_terminal().unwrap();
        let b = builder.add_terminal().unwrap();
        let c = builder.add_terminal().unwrap();

        builder
            .add_element(Element::new(
                DefinitionId::from(PrimitiveElementKind::Resistance),
                vec![a, b],
                vec![ValueRef::Literal(1.0)],
            ))
            .unwrap();

        builder
            .add_element(Element::new(
                DefinitionId::from(PrimitiveElementKind::Resistance),
                vec![b, c],
                vec![ValueRef::Literal(1.0)],
            ))
            .unwrap();

        let definition = builder.build_definition().unwrap();

        assert_eq!(
            definition.terminal_partitions(),
            &[
                TerminalPartitionId::new(0),
                TerminalPartitionId::new(0),
                TerminalPartitionId::new(0),
            ]
        );
    }

    #[test]
    fn tick_delay_produces_two_partitions() {
        let registry = DefinitionRegistry::new();
        let mut builder = DeviceDefinitionBuilder::new(&registry);

        let input_positive = builder.add_terminal().unwrap();
        let input_negative = builder.add_terminal().unwrap();
        let output_positive = builder.add_terminal().unwrap();
        let output_negative = builder.add_terminal().unwrap();

        builder
            .add_element(Element::new(
                DefinitionId::from(PrimitiveElementKind::TickDelay),
                vec![
                    input_positive,
                    input_negative,
                    output_positive,
                    output_negative,
                ],
                vec![ValueRef::Literal(0.0)],
            ))
            .unwrap();

        let definition = builder.build_definition().unwrap();

        assert_eq!(
            definition.terminal_partitions(),
            &[
                TerminalPartitionId::new(0),
                TerminalPartitionId::new(0),
                TerminalPartitionId::new(1),
                TerminalPartitionId::new(1),
            ]
        );
    }

    #[test]
    fn rejects_unused_internal_node() {
        let registry = DefinitionRegistry::new();
        let mut builder = DeviceDefinitionBuilder::new(&registry);

        let a = builder.add_terminal().unwrap();
        let b = builder.add_terminal().unwrap();
        let unused = builder.add_node().unwrap();

        builder
            .add_element(Element::new(
                DefinitionId::from(PrimitiveElementKind::Resistance),
                vec![a, b],
                vec![ValueRef::Literal(1.0)],
            ))
            .unwrap();

        assert!(matches!(
            builder.build_definition(),
            Err(DeviceDefinitionBuilderError::UnusedInternalNode { node })
                if node == unused
        ));
    }

    #[test]
    fn rejects_internal_component_without_exposed_terminal() {
        let registry = DefinitionRegistry::new();
        let mut builder = DeviceDefinitionBuilder::new(&registry);

        let a = builder.add_terminal().unwrap();
        let b = builder.add_terminal().unwrap();

        let internal_a = builder.add_node().unwrap();
        let internal_b = builder.add_node().unwrap();

        builder
            .add_element(Element::new(
                DefinitionId::from(PrimitiveElementKind::Resistance),
                vec![a, b],
                vec![ValueRef::Literal(1.0)],
            ))
            .unwrap();

        builder
            .add_element(Element::new(
                DefinitionId::from(PrimitiveElementKind::Resistance),
                vec![internal_a, internal_b],
                vec![ValueRef::Literal(1.0)],
            ))
            .unwrap();

        assert!(matches!(
            builder.build_definition(),
            Err(
                DeviceDefinitionBuilderError::InternalComponentWithoutTerminal {
                    node
                }
            ) if node == internal_a
        ));
    }

    #[test]
    fn validates_literal_primitive_parameter_relations() {
        let registry = DefinitionRegistry::new();
        let mut builder = DeviceDefinitionBuilder::new(&registry);

        let control = builder.add_terminal().unwrap();
        let channel_a = builder.add_terminal().unwrap();
        let channel_b = builder.add_terminal().unwrap();

        let result = builder.add_element(Element::new(
            DefinitionId::from(PrimitiveElementKind::VoltageControlledConductance),
            vec![control, channel_a, channel_b],
            vec![
                ValueRef::Literal(5.0),  // V_threshold
                ValueRef::Literal(2.0),  // V_transition
                ValueRef::Literal(10.0), // G_min
                ValueRef::Literal(1.0),  // G_max -- invalid
            ],
        ));

        assert!(matches!(
            result,
            Err(DeviceDefinitionBuilderError::PrimitiveParameters {
                definition_id,
                ..
            }) if definition_id
                == DefinitionId::from(
                    PrimitiveElementKind::VoltageControlledConductance
                )
        ));
    }

    #[test]
    fn nested_definition_uses_child_partition_summary() {
        let mut registry = DefinitionRegistry::new();

        let child = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let input_positive = builder.add_terminal().unwrap();
            let input_negative = builder.add_terminal().unwrap();
            let output_positive = builder.add_terminal().unwrap();
            let output_negative = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::TickDelay),
                    vec![
                        input_positive,
                        input_negative,
                        output_positive,
                        output_negative,
                    ],
                    vec![ValueRef::Literal(0.0)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        let child_id = registry.register(child).unwrap();

        let mut builder = DeviceDefinitionBuilder::new(&registry);

        let input_positive = builder.add_terminal().unwrap();
        let input_negative = builder.add_terminal().unwrap();
        let output_positive = builder.add_terminal().unwrap();
        let output_negative = builder.add_terminal().unwrap();

        builder
            .add_element(Element::new(
                child_id,
                vec![
                    input_positive,
                    input_negative,
                    output_positive,
                    output_negative,
                ],
                Vec::<ValueRef>::new(),
            ))
            .unwrap();

        let parent = builder.build_definition().unwrap();

        assert_eq!(
            parent.terminal_partitions(),
            &[
                TerminalPartitionId::new(0),
                TerminalPartitionId::new(0),
                TerminalPartitionId::new(1),
                TerminalPartitionId::new(1),
            ]
        );
    }
}
