use crate::ValueSlot;
use hynergy_mna::pattern::UnknownIndex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RhsAdd {
    destination: UnknownIndex,
    source: ValueSlot,
    scale: f64,
}

impl RhsAdd {
    #[inline]
    pub const fn new(destination: UnknownIndex, source: ValueSlot, scale: f64) -> Self {
        Self {
            destination,
            source,
            scale,
        }
    }

    #[inline]
    pub const fn destination(self) -> UnknownIndex {
        self.destination
    }

    #[inline]
    pub const fn source(self) -> ValueSlot {
        self.source
    }

    #[inline]
    pub const fn scale(self) -> f64 {
        self.scale
    }
}

#[derive(Debug, Default)]
pub struct RhsProgram {
    ops: Box<[RhsAdd]>,

    required_values: usize,
    required_rhs: usize,
}

impl RhsProgram {
    pub fn new(mut ops: Vec<RhsAdd>) -> Self {
        canonicalize(&mut ops);

        let required_values = ops
            .iter()
            .map(|op| op.source.index() + 1)
            .max()
            .unwrap_or(0);

        let required_rhs = ops
            .iter()
            .map(|op| op.destination.index() + 1)
            .max()
            .unwrap_or(0);

        Self {
            ops: ops.into_boxed_slice(),
            required_values,
            required_rhs,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    #[inline]
    pub fn required_value_count(&self) -> usize {
        self.required_values
    }

    #[inline]
    pub fn ops(&self) -> &[RhsAdd] {
        &self.ops
    }

    pub fn execute(&self, rhs: &mut [f64], values: &[f64]) {
        rhs.fill(0.0);
        self.execute_additive(rhs, values);
    }

    pub fn execute_additive(&self, rhs: &mut [f64], values: &[f64]) {
        assert!(
            values.len() >= self.required_values,
            "IR value workspace is too small"
        );

        assert!(
            rhs.len() >= self.required_rhs,
            "RHS does not match compiled IR"
        );

        for op in &self.ops {
            rhs[op.destination.index()] += values[op.source.index()] * op.scale;
        }
    }
}

fn canonicalize(ops: &mut Vec<RhsAdd>) {
    ops.retain(|op| op.scale != 0.0);

    ops.sort_unstable_by_key(|op| (op.destination.index(), op.source.index()));

    if ops.len() < 2 {
        return;
    }

    let mut write = 0usize;

    for read in 1..ops.len() {
        if ops[write].destination == ops[read].destination && ops[write].source == ops[read].source
        {
            ops[write].scale += ops[read].scale;
        } else {
            write += 1;

            if write != read {
                ops[write] = ops[read];
            }
        }
    }

    ops.truncate(write + 1);
    ops.retain(|op| op.scale != 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use hynergy_mna::pattern::UnknownIndex;

    #[test]
    fn clears_and_executes_rhs_program() {
        let program = RhsProgram::new(vec![
            RhsAdd::new(UnknownIndex::new(0), ValueSlot::new(0), 2.0),
            RhsAdd::new(UnknownIndex::new(1), ValueSlot::new(1), -1.0),
        ]);

        let values = [3.0, 5.0];
        let mut rhs = [100.0, 100.0];

        program.execute(&mut rhs, &values);

        assert_eq!(rhs, [6.0, -5.0]);
    }

    #[test]
    fn additive_execution_preserves_existing_rhs() {
        let program = RhsProgram::new(vec![RhsAdd::new(
            UnknownIndex::new(0),
            ValueSlot::new(0),
            2.0,
        )]);

        let values = [3.0];
        let mut rhs = [10.0];

        program.execute_additive(&mut rhs, &values);

        assert_eq!(rhs, [16.0]);
    }

    #[test]
    fn combines_duplicate_destination_and_source_pairs() {
        let program = RhsProgram::new(vec![
            RhsAdd::new(UnknownIndex::new(0), ValueSlot::new(0), 1.0),
            RhsAdd::new(UnknownIndex::new(0), ValueSlot::new(0), 2.0),
        ]);

        assert_eq!(program.len(), 1);
        assert_eq!(program.ops()[0].scale(), 3.0);
    }
}
