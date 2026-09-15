use faer::sparse::SymbolicSparseColMatRef;
use thiserror::Error;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownIndex(u32);

impl UnknownIndex {
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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatrixSlot(u32);

impl MatrixSlot {
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    fn from_index(index: usize) -> Self {
        debug_assert!(index <= u32::MAX as usize);

        Self(index as u32)
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PatternError {
    #[error("MNA dimension {dimension} exceeds maximum supported dimension {max}")]
    DimensionTooLarge { dimension: usize, max: usize },

    #[error("unknown index {index:?} is outside matrix dimension {dimension}")]
    IndexOutOfBounds {
        index: UnknownIndex,
        dimension: usize,
    },

    #[error("MNA pattern contains {nnz} non zeros, exceeding maximum {max}")]
    TooManyNonZeros { nnz: usize, max: usize },
}

#[derive(Debug)]
pub struct PatternBuilder {
    dimension: u32,
    coordinates: Vec<(u32, u32)>,
}

impl PatternBuilder {
    pub const MAX_DIMENSION: usize = i32::MAX as usize;

    #[inline]
    pub fn new(dimension: usize) -> Result<Self, PatternError> {
        Self::with_capacity(dimension, 0)
    }

    pub fn with_capacity(dimension: usize, requested_entries: usize) -> Result<Self, PatternError> {
        if dimension > Self::MAX_DIMENSION {
            return Err(PatternError::DimensionTooLarge {
                dimension,
                max: Self::MAX_DIMENSION,
            });
        }

        Ok(Self {
            dimension: dimension as u32,
            coordinates: Vec::with_capacity(requested_entries),
        })
    }

    #[inline]
    pub const fn dimension(&self) -> usize {
        self.dimension as usize
    }

    #[inline]
    pub fn requested_entry_count(&self) -> usize {
        self.coordinates.len()
    }

    #[inline]
    pub fn request(&mut self, row: UnknownIndex, column: UnknownIndex) -> Result<(), PatternError> {
        self.validate_index(row)?;
        self.validate_index(column)?;

        self.coordinates.push((column.get(), row.get()));

        Ok(())
    }

    pub fn finish(mut self) -> Result<MnaPattern, PatternError> {
        self.coordinates.sort_unstable();
        self.coordinates.dedup();

        let nnz = self.coordinates.len();

        if nnz > u32::MAX as usize {
            return Err(PatternError::TooManyNonZeros {
                nnz,
                max: u32::MAX as usize,
            });
        }

        let dimension = self.dimension();

        let mut column_ptrs = Vec::with_capacity(dimension + 1);
        let mut row_indices = Vec::with_capacity(nnz);

        let mut position = 0usize;

        for column in 0..dimension {
            column_ptrs.push(position as u32);

            while position < nnz && self.coordinates[position].0 as usize == column {
                row_indices.push(self.coordinates[position].1);
                position += 1;
            }
        }

        debug_assert_eq!(position, nnz);

        column_ptrs.push(nnz as u32);

        let pattern = MnaPattern {
            column_ptrs: column_ptrs.into_boxed_slice(),
            row_indices: row_indices.into_boxed_slice(),
        };

        #[cfg(debug_assertions)]
        pattern.assert_consistent();

        Ok(pattern)
    }

    #[inline]
    fn validate_index(&self, index: UnknownIndex) -> Result<(), PatternError> {
        if index.get() >= self.dimension {
            return Err(PatternError::IndexOutOfBounds {
                index,
                dimension: self.dimension(),
            });
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct MnaPattern {
    column_ptrs: Box<[u32]>,
    row_indices: Box<[u32]>,
}

impl MnaPattern {
    #[inline]
    pub fn dimension(&self) -> usize {
        self.column_ptrs.len() - 1
    }

    #[inline]
    pub fn nnz(&self) -> usize {
        self.row_indices.len()
    }

    pub fn slot(&self, row: UnknownIndex, column: UnknownIndex) -> Option<MatrixSlot> {
        if row.index() >= self.dimension() || column.index() >= self.dimension() {
            return None;
        }

        let column = column.index();

        let start = self.column_ptrs[column] as usize;
        let end = self.column_ptrs[column + 1] as usize;

        let relative = self.row_indices[start..end]
            .binary_search(&row.get())
            .ok()?;

        Some(MatrixSlot::from_index(start + relative))
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn column_ptrs(&self) -> &[u32] {
        &self.column_ptrs
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn row_indices(&self) -> &[u32] {
        &self.row_indices
    }

    #[cfg(debug_assertions)]
    fn assert_consistent(&self) {
        let dimension = self.dimension();

        assert_eq!(self.column_ptrs.len(), dimension + 1);
        assert_eq!(self.column_ptrs.first().copied(), Some(0));
        assert_eq!(
            self.column_ptrs.last().copied().map(|value| value as usize),
            Some(self.row_indices.len())
        );

        for pointers in self.column_ptrs.windows(2) {
            assert!(
                pointers[0] <= pointers[1],
                "CSC column pointers must be monotonically increasing"
            );
        }

        for column in 0..dimension {
            let start = self.column_ptrs[column] as usize;
            let end = self.column_ptrs[column + 1] as usize;
            let rows = &self.row_indices[start..end];

            assert!(
                rows.iter().all(|&row| (row as usize) < dimension),
                "CSC row index outside matrix dimension"
            );

            assert!(
                rows.windows(2).all(|rows| rows[0] < rows[1]),
                "CSC row indices must be strictly increasing"
            );
        }
    }

    #[inline]
    pub(crate) fn symbolic(&self) -> SymbolicSparseColMatRef<'_, u32> {
        SymbolicSparseColMatRef::new_checked(
            self.dimension(),
            self.dimension(),
            &self.column_ptrs,
            None,
            &self.row_indices,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicates_coordinates_and_assigns_csc_slots() {
        let mut builder = PatternBuilder::new(3).unwrap();

        builder
            .request(UnknownIndex::new(2), UnknownIndex::new(0))
            .unwrap();
        builder
            .request(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();
        builder
            .request(UnknownIndex::new(2), UnknownIndex::new(0))
            .unwrap();
        builder
            .request(UnknownIndex::new(1), UnknownIndex::new(2))
            .unwrap();

        let pattern = builder.finish().unwrap();

        assert_eq!(pattern.dimension(), 3);
        assert_eq!(pattern.nnz(), 3);

        assert_eq!(
            pattern
                .slot(UnknownIndex::new(0), UnknownIndex::new(0))
                .unwrap()
                .index(),
            0
        );

        assert_eq!(
            pattern
                .slot(UnknownIndex::new(2), UnknownIndex::new(0))
                .unwrap()
                .index(),
            1
        );

        assert_eq!(
            pattern
                .slot(UnknownIndex::new(1), UnknownIndex::new(2))
                .unwrap()
                .index(),
            2
        );

        assert_eq!(
            pattern.slot(UnknownIndex::new(1), UnknownIndex::new(1)),
            None
        );
    }

    #[test]
    fn sorts_rows_within_columns() {
        let mut builder = PatternBuilder::new(4).unwrap();

        builder
            .request(UnknownIndex::new(3), UnknownIndex::new(1))
            .unwrap();
        builder
            .request(UnknownIndex::new(0), UnknownIndex::new(1))
            .unwrap();
        builder
            .request(UnknownIndex::new(2), UnknownIndex::new(1))
            .unwrap();

        let pattern = builder.finish().unwrap();

        assert_eq!(pattern.row_indices(), &[0, 2, 3]);
        assert_eq!(pattern.column_ptrs(), &[0, 0, 3, 3, 3]);
    }

    #[test]
    fn preserves_empty_columns() {
        let mut builder = PatternBuilder::new(4).unwrap();

        builder
            .request(UnknownIndex::new(1), UnknownIndex::new(0))
            .unwrap();
        builder
            .request(UnknownIndex::new(2), UnknownIndex::new(3))
            .unwrap();

        let pattern = builder.finish().unwrap();

        assert_eq!(pattern.column_ptrs(), &[0, 1, 1, 1, 2]);
        assert_eq!(pattern.row_indices(), &[1, 2]);
    }

    #[test]
    fn rejects_row_outside_dimension() {
        let mut builder = PatternBuilder::new(3).unwrap();

        assert_eq!(
            builder.request(UnknownIndex::new(3), UnknownIndex::new(0)),
            Err(PatternError::IndexOutOfBounds {
                index: UnknownIndex::new(3),
                dimension: 3,
            })
        );
    }

    #[test]
    fn rejects_column_outside_dimension() {
        let mut builder = PatternBuilder::new(3).unwrap();

        assert_eq!(
            builder.request(UnknownIndex::new(0), UnknownIndex::new(3)),
            Err(PatternError::IndexOutOfBounds {
                index: UnknownIndex::new(3),
                dimension: 3,
            })
        );
    }

    #[test]
    fn rejects_dimension_unsupported_by_faer_u32_indices() {
        let dimension = i32::MAX as usize + 1;

        assert_eq!(
            PatternBuilder::new(dimension).unwrap_err(),
            PatternError::DimensionTooLarge {
                dimension,
                max: PatternBuilder::MAX_DIMENSION,
            }
        );
    }

    #[test]
    fn zero_dimension_is_valid() {
        let pattern = PatternBuilder::new(0).unwrap().finish().unwrap();

        assert_eq!(pattern.dimension(), 0);
        assert_eq!(pattern.nnz(), 0);
        assert_eq!(pattern.column_ptrs(), &[0]);
        assert!(pattern.row_indices().is_empty());
    }

    #[test]
    fn preallocated_builder_has_identical_semantics() {
        let mut builder = PatternBuilder::with_capacity(2, 32).unwrap();

        builder
            .request(UnknownIndex::new(0), UnknownIndex::new(0))
            .unwrap();
        builder
            .request(UnknownIndex::new(1), UnknownIndex::new(1))
            .unwrap();

        let pattern = builder.finish().unwrap();

        assert_eq!(pattern.column_ptrs(), &[0, 1, 2]);
        assert_eq!(pattern.row_indices(), &[0, 1]);
    }
}
