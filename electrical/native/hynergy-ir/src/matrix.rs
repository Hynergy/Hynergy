use crate::ValueSlot;
use hynergy_mna::pattern::MatrixSlot;
use hynergy_mna::system::MatrixValuesMut;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatrixAdd {
    destination: MatrixSlot,
    source: ValueSlot,
    scale: f64,
}

impl MatrixAdd {
    #[inline]
    pub const fn new(destination: MatrixSlot, source: ValueSlot, scale: f64) -> Self {
        Self {
            destination,
            source,
            scale,
        }
    }

    #[inline]
    pub const fn destination(self) -> MatrixSlot {
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
pub struct MatrixProgram {
    ops: Box<[MatrixAdd]>,

    required_values: usize,
    required_matrix_values: usize,
}

impl MatrixProgram {
    pub fn new(mut ops: Vec<MatrixAdd>) -> Self {
        canonicalize(&mut ops);

        let required_values = ops
            .iter()
            .map(|op| op.source.index() + 1)
            .max()
            .unwrap_or(0);

        let required_matrix_values = ops
            .iter()
            .map(|op| op.destination.index() + 1)
            .max()
            .unwrap_or(0);

        Self {
            ops: ops.into_boxed_slice(),
            required_values,
            required_matrix_values,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    #[inline]
    pub fn required_value_count(&self) -> usize {
        self.required_values
    }

    #[inline]
    pub fn ops(&self) -> &[MatrixAdd] {
        &self.ops
    }

    pub fn execute(&self, matrix: &mut MatrixValuesMut<'_>, values: &[f64]) {
        assert!(
            values.len() >= self.required_values,
            "IR value workspace is too small"
        );

        assert!(
            matrix.len() >= self.required_matrix_values,
            "MNA matrix does not match compiled IR"
        );

        for op in &self.ops {
            matrix.add(op.destination, values[op.source.index()] * op.scale);
        }
    }
}

fn canonicalize(ops: &mut Vec<MatrixAdd>) {
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
    use hynergy_mna::pattern::{PatternBuilder, UnknownIndex};
    use hynergy_mna::system::MnaSystem;

    #[test]
    fn executes_matrix_scatter_by_precompiled_slots() {
        let mut builder = PatternBuilder::new(2).unwrap();

        builder
            .request(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        builder
            .request(UnknownIndex::new(1), UnknownIndex::new(1))
            .unwrap();

        let pattern = builder.finish().unwrap();

        let a00 = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let a11 = pattern
            .slot(UnknownIndex::new(1), UnknownIndex::new(1))
            .unwrap();

        let mut system = MnaSystem::new(pattern).unwrap();

        let program = MatrixProgram::new(vec![
            MatrixAdd::new(a00, ValueSlot::new(0), 2.0),
            MatrixAdd::new(a11, ValueSlot::new(1), -1.0),
        ]);

        let values = [3.0, 5.0];

        {
            let mut matrix = system.values_mut();
            program.execute(&mut matrix, &values);
        }

        assert_eq!(system.values(), &[6.0, -5.0]);
    }

    #[test]
    fn combines_duplicate_destination_and_source_pairs() {
        let mut builder = PatternBuilder::new(1).unwrap();

        builder
            .request(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let pattern = builder.finish().unwrap();

        let slot = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let program = MatrixProgram::new(vec![
            MatrixAdd::new(slot, ValueSlot::new(0), 1.0),
            MatrixAdd::new(slot, ValueSlot::new(0), 2.0),
            MatrixAdd::new(slot, ValueSlot::new(0), -0.5),
        ]);

        assert_eq!(program.len(), 1);
        assert_eq!(program.ops()[0].scale(), 2.5);
    }

    #[test]
    fn removes_exactly_cancelled_contributions() {
        let mut builder = PatternBuilder::new(1).unwrap();

        builder
            .request(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let pattern = builder.finish().unwrap();

        let slot = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let program = MatrixProgram::new(vec![
            MatrixAdd::new(slot, ValueSlot::new(0), 1.0),
            MatrixAdd::new(slot, ValueSlot::new(0), -1.0),
        ]);

        assert!(program.is_empty());
    }
}
