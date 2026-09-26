use super::IslandId;

const DEVICE_CHUNK_ROWS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeviceTopologyChunk {
    partition_count: usize,
    row_count: u8,
    component_islands: Vec<IslandId>,
}

impl DeviceTopologyChunk {
    #[inline]
    pub(super) fn new(partition_count: usize) -> Self {
        Self {
            partition_count,
            row_count: 0,
            component_islands: Vec::new(),
        }
    }

    #[inline]
    pub(super) fn partition_count(&self) -> usize {
        self.partition_count
    }

    #[inline]
    pub(super) fn row_count(&self) -> usize {
        usize::from(self.row_count)
    }

    #[inline]
    pub(super) fn reserve_rows(&mut self, additional: usize) {
        self.component_islands
            .reserve(additional.saturating_mul(self.partition_count));
    }

    #[cfg(test)]
    pub(super) fn push_row(&mut self, islands: &[IslandId]) {
        self.reserve_rows(1);
        self.push_row_iter(islands.iter().copied());
    }

    pub(super) fn push_row_iter(&mut self, islands: impl ExactSizeIterator<Item = IslandId>) {
        assert!(self.row_count() < DEVICE_CHUNK_ROWS);
        assert_eq!(islands.len(), self.partition_count);

        debug_assert!(
            self.component_islands.capacity()
                >= self.component_islands.len() + self.partition_count,
            "topology row capacity must be reserved before commit",
        );

        self.component_islands.extend(islands);
        self.row_count += 1;
    }

    #[inline]
    fn row_range(&self, row: usize) -> std::ops::Range<usize> {
        assert!(row < self.row_count());

        let start = row * self.partition_count;

        start..start + self.partition_count
    }

    #[inline]
    pub(super) fn row_islands(&self, row: usize) -> &[IslandId] {
        let range = self.row_range(row);
        &self.component_islands[range]
    }

    #[inline]
    pub(super) fn component_island(&self, row: usize, partition: usize) -> IslandId {
        assert!(row < self.row_count());
        assert!(partition < self.partition_count);

        self.component_islands[row * self.partition_count + partition]
    }

    #[inline]
    pub(super) fn set_component_island(&mut self, row: usize, partition: usize, island: IslandId) {
        assert!(row < self.row_count());
        assert!(partition < self.partition_count);

        self.component_islands[row * self.partition_count + partition] = island;
    }

    pub(super) fn remove_row(&mut self, row: usize) {
        let row_count = self.row_count();

        assert!(row < row_count);

        let last = row_count - 1;

        if self.partition_count != 0 && row != last {
            let source_start = last * self.partition_count;
            let source_end = source_start + self.partition_count;

            self.component_islands
                .copy_within(source_start..source_end, row * self.partition_count);
        }

        self.component_islands.truncate(last * self.partition_count);

        self.row_count -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceTopologyChunk;
    use crate::topology::IslandId;

    #[test]
    fn topology_chunk_middle_row_removal_moves_entire_partition_stride() {
        let a = IslandId::try_from(1).unwrap();
        let b = IslandId::try_from(2).unwrap();
        let c = IslandId::try_from(3).unwrap();
        let d = IslandId::try_from(4).unwrap();
        let e = IslandId::try_from(5).unwrap();
        let f = IslandId::try_from(6).unwrap();

        let mut chunk = DeviceTopologyChunk::new(2);

        chunk.push_row(&[a, b]);
        chunk.push_row(&[c, d]);
        chunk.push_row(&[e, f]);

        chunk.remove_row(1);

        assert_eq!(chunk.row_count(), 2);
        assert_eq!(chunk.row_islands(0), &[a, b]);
        assert_eq!(chunk.row_islands(1), &[e, f]);
    }

    #[test]
    fn zero_partition_topology_chunk_still_tracks_rows() {
        let mut chunk = DeviceTopologyChunk::new(0);

        chunk.push_row(&[]);
        chunk.push_row(&[]);

        assert_eq!(chunk.row_count(), 2);

        chunk.remove_row(0);

        assert_eq!(chunk.row_count(), 1);
    }
}
