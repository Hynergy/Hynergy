use crate::devices::DefinitionId;
use crate::{NodeId, ParameterConstraintError, ParameterId};
use smallvec::SmallVec;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CircuitBuilderError {
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueRef {
    Literal(f64),
    Parameter(ParameterId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    definition_id: DefinitionId,
    terminals: SmallVec<[NodeId; 4]>,
    parameters: SmallVec<[ValueRef; 1]>,
}

impl Element {
    pub fn new(
        definition_id: DefinitionId,
        terminals: impl Into<SmallVec<[NodeId; 4]>>,
        parameters: impl Into<SmallVec<[ValueRef; 1]>>,
    ) -> Self {
        Self {
            definition_id,
            terminals: terminals.into(),
            parameters: parameters.into(),
        }
    }

    pub fn definition(&self) -> DefinitionId {
        self.definition_id
    }

    pub fn terminals(&self) -> &SmallVec<[NodeId; 4]> {
        &self.terminals
    }

    pub fn parameters(&self) -> &SmallVec<[ValueRef; 1]> {
        &self.parameters
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Circuit {
    node_count: u32,
    elements: Vec<Element>,
}

impl Circuit {
    pub fn new(node_count: u32, elements: Vec<Element>) -> Self {
        Circuit {
            node_count,
            elements,
        }
    }

    pub fn node_count(&self) -> u32 {
        self.node_count
    }

    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
}
