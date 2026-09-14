use crate::circuit::{Circuit, Element, ElementId, NodeId, ValueRef};
use crate::device::definition::{
    DefinitionId, DeviceBody, DeviceDefinition, PrimitiveParameterError, TerminalPartitionId,
    TerminalPartitionLayout,
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

    #[error(
        "definition parameter {parameter:?} does not satisfy the constraints \
     of parameter {parameter_index} of device {definition_id:?}"
    )]
    ParameterConstraintsIncompatible {
        definition_id: DefinitionId,
        parameter_index: usize,
        parameter: ParameterId,
    },

    #[error("definition parameter {parameter:?} is unused")]
    UnusedParameter { parameter: ParameterId },

    #[error("exhausted TerminalPartitionId range")]
    TerminalPartitionIdExhausted,

    #[error("definition state count exceeds the addressable state range")]
    StateCountExhausted,
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

                ValueRef::Parameter(parameter_id) => {
                    let Some(parameter_constraints) =
                        self.param_constraints.get(parameter_id.index())
                    else {
                        return Err(DeviceDefinitionBuilderError::ParameterOutOfRange {
                            parameter_index,
                            parameter: *parameter_id,
                            parameter_count: self.param_constraints.len(),
                        });
                    };

                    if !parameter_constraints.is_subset_of(constraint) {
                        return Err(
                            DeviceDefinitionBuilderError::ParameterConstraintsIncompatible {
                                definition_id,
                                parameter_index,
                                parameter: *parameter_id,
                            },
                        );
                    }
                }
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
                kind.validate_parameter_relations(&parameters)
                    .map_err(|source| DeviceDefinitionBuilderError::PrimitiveParameters {
                        definition_id,
                        source,
                    })?;
            }
        }

        Ok(())
    }

    #[inline]
    pub fn build_definition(self) -> Result<DeviceDefinition, DeviceDefinitionBuilderError> {
        self.validate_parameter_usage()?;

        let state_count = self.derive_state_count()?;
        let terminal_partition_layout = self.derive_terminal_partition_layout()?;

        Ok(DeviceDefinition::new_composite(
            Circuit::new(self.node_count, self.elements),
            self.terminals,
            self.param_constraints,
            terminal_partition_layout,
            state_count,
        ))
    }

    fn derive_terminal_partition_layout(
        &self,
    ) -> Result<TerminalPartitionLayout, DeviceDefinitionBuilderError> {
        let node_count = self.node_count as usize;

        let mut union_find = UnionFind::new(node_count);
        let mut referenced = vec![false; node_count];

        let mut partition_anchors = SmallVec::<[Option<usize>; 4]>::new();

        for element in &self.elements {
            let definition = self
                .registry
                .get(element.definition())
                .expect("elements are validated before insertion");

            partition_anchors.clear();
            partition_anchors.resize(definition.terminal_partition_count(), None);

            for (&node, &partition) in element
                .terminals()
                .iter()
                .zip(definition.terminal_partitions())
            {
                let node_index = node.index();
                referenced[node_index] = true;

                match &mut partition_anchors[partition.index()] {
                    Some(anchor) => {
                        union_find.union(node_index, *anchor);
                    }

                    anchor => {
                        *anchor = Some(node_index);
                    }
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
                    let raw = u16::try_from(next_partition)
                        .map_err(|_| DeviceDefinitionBuilderError::TerminalPartitionIdExhausted)?;

                    let partition = TerminalPartitionId::new(raw);
                    next_partition += 1;

                    root_partitions[root] = Some(partition);

                    partition
                }
            };

            terminal_partitions.push(partition);
        }

        Ok(TerminalPartitionLayout::from_canonical_parts(
            terminal_partitions,
            next_partition as usize,
        ))
    }

    fn validate_parameter_usage(&self) -> Result<(), DeviceDefinitionBuilderError> {
        if self.param_constraints.is_empty() {
            return Ok(());
        }

        let mut used = vec![false; self.param_constraints.len()];

        for element in &self.elements {
            for value in element.parameters() {
                if let ValueRef::Parameter(parameter) = value {
                    used[parameter.index()] = true;
                }
            }
        }

        if let Some(index) = used.iter().position(|&used| !used) {
            let parameter = ParameterId::new(
                u32::try_from(index)
                    .expect("parameter count is constrained to the ParameterId range"),
            );

            return Err(DeviceDefinitionBuilderError::UnusedParameter { parameter });
        }

        Ok(())
    }

    #[inline]
    fn derive_state_count(&self) -> Result<usize, DeviceDefinitionBuilderError> {
        self.elements.iter().try_fold(0usize, |total, element| {
            let definition = self
                .registry
                .get(element.definition())
                .expect("elements are validated before insertion");

            total
                .checked_add(definition.state_count())
                .ok_or(DeviceDefinitionBuilderError::StateCountExhausted)
        })
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
    use crate::circuit::{Element, NodeId, ValueRef};
    use crate::device::definition::{DefinitionId, PrimitiveElementKind, TerminalPartitionId};
    use crate::device::registry::DefinitionRegistry;

    use crate::parameter::{Bound, ParameterConstraintError, ParameterConstraints, ParameterId};

    fn terminals<const N: usize>(builder: &mut DeviceDefinitionBuilder<'_>) -> [NodeId; N] {
        std::array::from_fn(|_| builder.add_terminal().unwrap())
    }

    fn positive() -> ParameterConstraints {
        ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: false,
            }),
            None,
            false,
            None,
        )
    }

    fn non_negative() -> ParameterConstraints {
        ParameterConstraints::new(
            Some(Bound {
                value: 0.0,
                inclusive: true,
            }),
            None,
            false,
            None,
        )
    }

    #[test]
    fn add_element_rejects_invalid_shape_references_and_values() {
        let registry = DefinitionRegistry::new();
        let resistance = DefinitionId::from(PrimitiveElementKind::Resistance);

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let unknown = DefinitionId::try_from(999).unwrap();

            assert!(matches!(
                builder.add_element(Element::new(
                    unknown,
                    Vec::<NodeId>::new(),
                    Vec::<ValueRef>::new(),
                )),
                Err(DeviceDefinitionBuilderError::UnknownDefinition {
                    definition_id
                }) if definition_id == unknown
            ));
        }

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, _b] = terminals(&mut builder);

            assert!(matches!(
                builder.add_element(Element::new(
                    resistance,
                    vec![a],
                    vec![ValueRef::Literal(1.0)],
                )),
                Err(
                    DeviceDefinitionBuilderError::TerminalCountMismatch {
                        definition_id,
                        ..
                    }
                ) if definition_id == resistance
            ));
        }

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);

            assert!(matches!(
                builder.add_element(Element::new(
                    resistance,
                    vec![a, b],
                    Vec::<ValueRef>::new(),
                )),
                Err(
                    DeviceDefinitionBuilderError::ParameterCountMismatch {
                        definition_id,
                        ..
                    }
                ) if definition_id == resistance
            ));
        }

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a] = terminals(&mut builder);
            let missing = NodeId::new(1);

            assert!(matches!(
                builder.add_element(Element::new(
                    resistance,
                    vec![a, missing],
                    vec![ValueRef::Literal(1.0)],
                )),
                Err(DeviceDefinitionBuilderError::NodeOutOfRange {
                    terminal_index: 1,
                    node,
                    ..
                }) if node == missing
            ));
        }

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);
            let missing = ParameterId::new(0);

            assert!(matches!(
                builder.add_element(Element::new(
                    resistance,
                    vec![a, b],
                    vec![ValueRef::Parameter(missing)],
                )),
                Err(
                    DeviceDefinitionBuilderError::ParameterOutOfRange {
                        parameter_index: 0,
                        parameter,
                        ..
                    }
                ) if parameter == missing
            ));
        }

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);

            assert!(matches!(
                builder.add_element(Element::new(
                    resistance,
                    vec![a, b],
                    vec![ValueRef::Literal(0.0)],
                )),
                Err(DeviceDefinitionBuilderError::ParameterConstraint {
                    parameter_index: 0,
                    source: ParameterConstraintError::OutOfRange,
                })
            ));
        }
    }

    #[test]
    fn parent_parameter_must_satisfy_every_child_use() {
        let registry = DefinitionRegistry::new();

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);

            let parameter = builder.add_parameter(positive()).unwrap();

            builder
                .add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::Resistance),
                    vec![a, b],
                    vec![ValueRef::Parameter(parameter)],
                ))
                .unwrap();

            builder.build_definition().unwrap();
        }

        {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);

            let parameter = builder.add_parameter(non_negative()).unwrap();

            builder
                .add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::Conductance),
                    vec![a, b],
                    vec![ValueRef::Parameter(parameter)],
                ))
                .unwrap();

            assert!(matches!(
                builder.add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::Resistance),
                    vec![a, b],
                    vec![ValueRef::Parameter(parameter)],
                )),
                Err(
                    DeviceDefinitionBuilderError::ParameterConstraintsIncompatible {
                        definition_id,
                        parameter_index: 0,
                        parameter: actual,
                    }
                )
                    if definition_id
                        == DefinitionId::from(
                            PrimitiveElementKind::Resistance
                        )
                        && actual == parameter
            ));
        }
    }
    #[test]
    fn build_rejects_unused_parameter() {
        let registry = DefinitionRegistry::new();
        let mut builder = DeviceDefinitionBuilder::new(&registry);

        let parameter = builder
            .add_parameter(ParameterConstraints::default())
            .unwrap();

        assert_eq!(
            builder.build_definition(),
            Err(DeviceDefinitionBuilderError::UnusedParameter { parameter })
        );
    }

    #[test]
    fn build_rejects_terminal_partition_id_overflow() {
        let registry = DefinitionRegistry::new();
        let mut builder = DeviceDefinitionBuilder::new(&registry);

        for _ in 0..(u16::MAX as usize + 2) {
            builder.add_terminal().unwrap();
        }

        assert_eq!(
            builder.build_definition(),
            Err(DeviceDefinitionBuilderError::TerminalPartitionIdExhausted)
        );
    }

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
    fn add_element_validates_literal_primitive_relations() {
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
    fn primitive_terminal_layouts_have_valid_partition_counts() {
        for kind in PrimitiveElementKind::ALL {
            let definition = kind.definition();

            assert_eq!(
                definition.terminals().len(),
                definition.terminal_partitions().len(),
            );

            let expected_count = match kind {
                PrimitiveElementKind::TickDelay => 2,
                _ => 1,
            };

            assert_eq!(
                definition.terminal_partition_count(),
                expected_count,
                "{kind:?}",
            );
        }
    }

    #[test]
    fn nested_definition_groups_non_adjacent_terminals_in_same_partition() {
        let mut registry = DefinitionRegistry::new();

        let child = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let a = builder.add_terminal().unwrap();
            let _b = builder.add_terminal().unwrap();
            let c = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::Resistance),
                    vec![a, c],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        let child_id = registry.register(child).unwrap();

        assert_eq!(
            registry.get(child_id).unwrap().terminal_partitions(),
            &[
                TerminalPartitionId::new(0),
                TerminalPartitionId::new(1),
                TerminalPartitionId::new(0),
            ]
        );
    }

    #[test]
    fn composite_state_count_accumulates_nested_element_state() {
        let mut registry = DefinitionRegistry::new();

        let child = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);

            builder
                .add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::Capacitor),
                    vec![a, b],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::Inductor),
                    vec![a, b],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(child.state_count(), 2);

        let child_id = registry.register(child).unwrap();

        let parent = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);

            builder
                .add_element(Element::new(child_id, vec![a, b], Vec::<ValueRef>::new()))
                .unwrap();

            builder
                .add_element(Element::new(child_id, vec![a, b], Vec::<ValueRef>::new()))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(parent.state_count(), 4);
    }

    #[test]
    fn composite_state_count_overflow_is_rejected() {
        let mut registry = DefinitionRegistry::new();

        let current_definition = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);
            let [a, b] = terminals(&mut builder);

            builder
                .add_element(Element::new(
                    DefinitionId::from(PrimitiveElementKind::Capacitor),
                    vec![a, b],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(current_definition.state_count(), 1);

        let mut current = registry.register(current_definition).unwrap();

        for _ in 0..usize::BITS {
            let result = {
                let mut builder = DeviceDefinitionBuilder::new(&registry);
                let [a, b] = terminals(&mut builder);

                for _ in 0..2 {
                    builder
                        .add_element(Element::new(current, vec![a, b], Vec::<ValueRef>::new()))
                        .unwrap();
                }

                builder.build_definition()
            };

            match result {
                Ok(definition) => {
                    current = registry.register(definition).unwrap();
                }

                Err(DeviceDefinitionBuilderError::StateCountExhausted) => {
                    return;
                }

                Err(error) => {
                    panic!("unexpected builder error: {error:?}");
                }
            }
        }

        panic!("state count should overflow");
    }
}
