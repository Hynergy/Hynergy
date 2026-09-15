use crate::ValueSlot;
use thiserror::Error;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateSlot(u32);

impl StateSlot {
    #[inline]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateWrite {
    destination: StateSlot,
    source: ValueSlot,
}

impl StateWrite {
    #[inline]
    pub const fn new(destination: StateSlot, source: ValueSlot) -> Self {
        Self {
            destination,
            source,
        }
    }

    #[inline]
    pub const fn destination(self) -> StateSlot {
        self.destination
    }

    #[inline]
    pub const fn source(self) -> ValueSlot {
        self.source
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum StateProgramError {
    #[error("state slot {destination:?} has more than one next-state producer")]
    DuplicateDestination { destination: StateSlot },
}

#[derive(Debug, Default, PartialEq)]
pub struct StateTransitionProgram {
    writes: Box<[StateWrite]>,

    required_values: usize,
    required_state: usize,
}

impl StateTransitionProgram {
    pub fn new(mut writes: Vec<StateWrite>) -> Result<Self, StateProgramError> {
        writes.sort_unstable_by_key(|write| write.destination.index());

        for pair in writes.windows(2) {
            if pair[0].destination == pair[1].destination {
                return Err(StateProgramError::DuplicateDestination {
                    destination: pair[0].destination,
                });
            }
        }

        let required_values = writes
            .iter()
            .map(|write| write.source.index() + 1)
            .max()
            .unwrap_or(0);

        let required_state = writes
            .iter()
            .map(|write| write.destination.index() + 1)
            .max()
            .unwrap_or(0);

        Ok(Self {
            writes: writes.into_boxed_slice(),
            required_values,
            required_state,
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.writes.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    #[inline]
    pub fn writes(&self) -> &[StateWrite] {
        &self.writes
    }

    #[inline]
    pub fn required_value_count(&self) -> usize {
        self.required_values
    }

    #[inline]
    pub fn required_state_count(&self) -> usize {
        self.required_state
    }

    pub fn execute(&self, next_state: &mut [f64], values: &[f64]) {
        assert!(
            values.len() >= self.required_values,
            "IR value workspace is too small",
        );

        assert!(
            next_state.len() >= self.required_state,
            "next-state workspace is too small",
        );

        for write in &self.writes {
            next_state[write.destination.index()] = values[write.source.index()];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ValueProgramBuilder;

    #[test]
    fn writes_computed_values_to_next_state() {
        let mut values = ValueProgramBuilder::new();

        let input = values.tick_input().unwrap();
        let two = values.constant(2.0).unwrap();

        let next = values.mul(input.value(), two).unwrap();

        let value_program = values.finish();

        let transition =
            StateTransitionProgram::new(vec![StateWrite::new(StateSlot::new(0), next)]).unwrap();

        let mut workspace = value_program.new_workspace();

        workspace.set_input(input, 3.0);

        value_program.execute_tick(&mut workspace);

        let mut next_state = [0.0];

        transition.execute(&mut next_state, workspace.values());

        assert_eq!(next_state, [6.0]);
    }

    #[test]
    fn rejects_multiple_producers_for_same_state() {
        let mut values = ValueProgramBuilder::new();

        let a = values.constant(1.0).unwrap();
        let b = values.constant(2.0).unwrap();

        assert_eq!(
            StateTransitionProgram::new(vec![
                StateWrite::new(StateSlot::new(0), a,),
                StateWrite::new(StateSlot::new(0), b,),
            ]),
            Err(StateProgramError::DuplicateDestination {
                destination: StateSlot::new(0),
            },),
        );
    }

    #[test]
    fn sparse_destination_range_is_allowed() {
        let mut values = ValueProgramBuilder::new();

        let value = values.constant(4.0).unwrap();

        let program =
            StateTransitionProgram::new(vec![StateWrite::new(StateSlot::new(3), value)]).unwrap();

        assert_eq!(program.len(), 1);
        assert_eq!(program.required_state_count(), 4);

        let workspace = values.finish().new_workspace();

        let mut next_state = [10.0, 20.0, 30.0, 40.0];

        program.execute(&mut next_state, workspace.values());

        assert_eq!(next_state, [10.0, 20.0, 30.0, 4.0],);
    }

    #[test]
    fn empty_transition_does_nothing() {
        let program = StateTransitionProgram::new(Vec::new()).unwrap();

        let mut state = [1.0, 2.0];

        program.execute(&mut state, &[]);

        assert_eq!(state, [1.0, 2.0]);
    }
}
