use crate::pattern::{MatrixSlot, MnaPattern};
use faer::{
    MatMut,
    linalg::solvers::Solve,
    sparse::{
        FaerError, SparseColMatRef,
        linalg::{
            LuError,
            solvers::{Lu, SymbolicLu},
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MnaError {
    IndexOverflow,
    OutOfMemory,

    Singular { index: usize },

    NotFactorized,

    RhsLengthMismatch { expected: usize, actual: usize },

    BackendFailure,
}

impl MnaError {
    #[inline]
    fn from_faer(error: FaerError) -> Self {
        match error {
            FaerError::IndexOverflow => Self::IndexOverflow,
            FaerError::OutOfMemory => Self::OutOfMemory,
            _ => Self::BackendFailure,
        }
    }

    #[inline]
    fn from_lu(error: LuError) -> Self {
        match error {
            LuError::SymbolicSingular { index } => Self::Singular { index },
            LuError::Generic(error) => Self::from_faer(error),
        }
    }
}

pub struct MatrixValuesMut<'a> {
    values: &'a mut [f64],
}

impl MatrixValuesMut<'_> {
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.values.fill(0.0);
    }

    #[inline]
    pub fn get(&self, slot: MatrixSlot) -> f64 {
        self.values[slot.index()]
    }

    #[inline]
    pub fn set(&mut self, slot: MatrixSlot, value: f64) {
        self.values[slot.index()] = value;
    }

    #[inline]
    pub fn add(&mut self, slot: MatrixSlot, value: f64) {
        self.values[slot.index()] += value;
    }

    #[inline]
    pub fn copy_from_slice(&mut self, source: &[f64]) {
        self.values.copy_from_slice(source);
    }
}

#[derive(Debug)]
pub struct MnaSystem {
    pattern: MnaPattern,
    values: Box<[f64]>,
    symbolic_lu: Option<SymbolicLu<u32>>,
    numeric_lu: Option<Lu<u32, f64>>,
}

impl MnaSystem {
    pub fn new(pattern: MnaPattern) -> Result<Self, MnaError> {
        let symbolic_lu = if pattern.dimension() == 0 {
            None
        } else {
            Some(SymbolicLu::try_new(pattern.symbolic()).map_err(MnaError::from_faer)?)
        };

        let values = vec![0.0; pattern.nnz()].into_boxed_slice();

        Ok(Self {
            pattern,
            values,
            symbolic_lu,
            numeric_lu: None,
        })
    }

    #[inline]
    pub fn dimension(&self) -> usize {
        self.pattern.dimension()
    }

    #[inline]
    pub fn nnz(&self) -> usize {
        self.pattern.nnz()
    }

    #[inline]
    pub fn pattern(&self) -> &MnaPattern {
        &self.pattern
    }

    #[inline]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    #[inline]
    pub fn values_mut(&mut self) -> MatrixValuesMut<'_> {
        self.numeric_lu = None;

        MatrixValuesMut {
            values: &mut self.values,
        }
    }

    #[inline]
    pub fn is_factorized(&self) -> bool {
        self.dimension() == 0 || self.numeric_lu.is_some()
    }

    pub fn factorize(&mut self) -> Result<(), MnaError> {
        if self.dimension() == 0 {
            return Ok(());
        }

        let symbolic = self
            .symbolic_lu
            .as_ref()
            .expect("non-empty MNA system must own symbolic LU");

        let matrix = SparseColMatRef::new(self.pattern.symbolic(), &self.values);

        let numeric =
            Lu::try_new_with_symbolic(symbolic.clone(), matrix).map_err(MnaError::from_lu)?;

        self.numeric_lu = Some(numeric);

        Ok(())
    }

    pub fn solve_in_place(&self, rhs: &mut [f64]) -> Result<(), MnaError> {
        let dimension = self.dimension();

        if rhs.len() != dimension {
            return Err(MnaError::RhsLengthMismatch {
                expected: dimension,
                actual: rhs.len(),
            });
        }

        if dimension == 0 {
            return Ok(());
        }

        let lu = self.numeric_lu.as_ref().ok_or(MnaError::NotFactorized)?;

        let rhs = MatMut::from_column_major_slice_mut(rhs, dimension, 1);

        lu.solve_in_place(rhs);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{PatternBuilder, UnknownIndex};

    fn full_2x2_pattern() -> MnaPattern {
        let mut builder = PatternBuilder::new(2).unwrap();

        for column in 0..2 {
            for row in 0..2 {
                builder
                    .request(UnknownIndex::new(row), UnknownIndex::new(column))
                    .unwrap();
            }
        }

        builder.finish().unwrap()
    }

    #[test]
    fn solves_sparse_linear_system() {
        // A =
        //
        // [ 4  1 ]
        // [ 2  3 ]
        //
        // b =
        //
        // [ 1 ]
        // [ 2 ]
        //
        // x =
        //
        // [ 0.1 ]
        // [ 0.6 ]

        let pattern = full_2x2_pattern();

        let a00 = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let a10 = pattern
            .slot(UnknownIndex::new(1), UnknownIndex::new(0))
            .unwrap();

        let a01 = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(1))
            .unwrap();

        let a11 = pattern
            .slot(UnknownIndex::new(1), UnknownIndex::new(1))
            .unwrap();

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut values = system.values_mut();

            values.set(a00, 4.0);
            values.set(a10, 2.0);
            values.set(a01, 1.0);
            values.set(a11, 3.0);
        }

        system.factorize().unwrap();

        let mut rhs = [1.0, 2.0];

        system.solve_in_place(&mut rhs).unwrap();

        assert!((rhs[0] - 0.1).abs() < 1.0e-12);
        assert!((rhs[1] - 0.6).abs() < 1.0e-12);
    }

    #[test]
    fn modifying_values_invalidates_only_numeric_factorization() {
        let pattern = full_2x2_pattern();

        let a00 = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let a10 = pattern
            .slot(UnknownIndex::new(1), UnknownIndex::new(0))
            .unwrap();

        let a01 = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(1))
            .unwrap();

        let a11 = pattern
            .slot(UnknownIndex::new(1), UnknownIndex::new(1))
            .unwrap();

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut values = system.values_mut();

            values.set(a00, 4.0);
            values.set(a10, 2.0);
            values.set(a01, 1.0);
            values.set(a11, 3.0);
        }

        system.factorize().unwrap();

        assert!(system.is_factorized());

        {
            let mut values = system.values_mut();
            values.set(a00, 5.0);
        }

        assert!(!system.is_factorized());

        let mut rhs = [1.0, 2.0];

        assert_eq!(
            system.solve_in_place(&mut rhs),
            Err(MnaError::NotFactorized)
        );

        system.factorize().unwrap();

        assert!(system.is_factorized());
    }

    #[test]
    fn factorization_can_solve_multiple_rhs_vectors() {
        let pattern = full_2x2_pattern();

        let a00 = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();

        let a10 = pattern
            .slot(UnknownIndex::new(1), UnknownIndex::new(0))
            .unwrap();

        let a01 = pattern
            .slot(UnknownIndex::new(0), UnknownIndex::new(1))
            .unwrap();

        let a11 = pattern
            .slot(UnknownIndex::new(1), UnknownIndex::new(1))
            .unwrap();

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut values = system.values_mut();

            values.set(a00, 4.0);
            values.set(a10, 2.0);
            values.set(a01, 1.0);
            values.set(a11, 3.0);
        }

        system.factorize().unwrap();

        let mut rhs_a = [1.0, 2.0];
        let mut rhs_b = [5.0, 5.0];

        system.solve_in_place(&mut rhs_a).unwrap();
        system.solve_in_place(&mut rhs_b).unwrap();

        assert!((rhs_a[0] - 0.1).abs() < 1.0e-12);
        assert!((rhs_a[1] - 0.6).abs() < 1.0e-12);

        assert!((rhs_b[0] - 1.0).abs() < 1.0e-12);
        assert!((rhs_b[1] - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn rejects_wrong_rhs_dimension() {
        let pattern = full_2x2_pattern();
        let system = MnaSystem::new(pattern).unwrap();

        let mut rhs = [1.0];

        assert_eq!(
            system.solve_in_place(&mut rhs),
            Err(MnaError::RhsLengthMismatch {
                expected: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn zero_dimension_system_is_trivial() {
        let pattern = PatternBuilder::new(0).unwrap().finish().unwrap();

        let mut system = MnaSystem::new(pattern).unwrap();

        assert!(system.is_factorized());

        system.factorize().unwrap();

        let mut rhs = [];

        system.solve_in_place(&mut rhs).unwrap();
    }
}
