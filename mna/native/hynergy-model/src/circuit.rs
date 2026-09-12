use crate::device::definition::DefinitionId;
use crate::ids::define_id;
use crate::parameter::ParameterId;
use smallvec::SmallVec;

define_id!(NodeId, ElementId);

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
        Self {
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
