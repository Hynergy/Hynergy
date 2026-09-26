use crate::compile::definition::{DefinitionStateId, DefinitionStateInitializer};
use crate::compile::island::DeviceState;
use hynergy_model::device::definition::{DeviceDefinition, DeviceId};
use hynergy_model::network::{
    DeviceInsertResult, DeviceLocation, DeviceRemoveResult, Network, PreparedDeviceInsert,
};
use thiserror::Error;

#[cfg(debug_assertions)]
use hynergy_model::device::registry::DefinitionRegistry;

const DEVICE_CHUNK_ROWS: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct StateChunk {
    state_count: usize,
    row_count: u8,
    values: Vec<f64>,
    initialized: u64,
}

impl StateChunk {
    #[inline]
    fn new(state_count: usize) -> Self {
        Self {
            state_count,
            row_count: 0,
            values: Vec::new(),
            initialized: 0,
        }
    }

    #[inline]
    fn row_count(&self) -> usize {
        usize::from(self.row_count)
    }

    #[inline]
    fn reserve_rows(&mut self, additional: usize) {
        self.values
            .reserve(additional.saturating_mul(self.state_count));
    }

    fn push_row(&mut self) {
        assert!(self.row_count() < DEVICE_CHUNK_ROWS);

        self.values
            .extend(std::iter::repeat_n(0.0, self.state_count));

        self.row_count += 1;
    }

    #[inline]
    fn row_range(&self, row: usize) -> std::ops::Range<usize> {
        assert!(row < self.row_count());

        let start = row * self.state_count;

        start..start + self.state_count
    }

    #[inline]
    fn row_values(&self, row: usize) -> &[f64] {
        let range = self.row_range(row);
        &self.values[range]
    }

    #[inline]
    fn row_values_mut(&mut self, row: usize) -> &mut [f64] {
        let range = self.row_range(row);
        &mut self.values[range]
    }

    #[inline]
    fn is_initialized(&self, row: usize) -> bool {
        debug_assert!(row < self.row_count());

        self.initialized & (1_u64 << row) != 0
    }

    #[inline]
    fn set_initialized(&mut self, row: usize) {
        debug_assert!(row < self.row_count());

        self.initialized |= 1_u64 << row;
    }

    fn remove_row(&mut self, row: usize) {
        let row_count = self.row_count();

        assert!(row < row_count);

        let last = row_count - 1;

        if row != last && self.state_count != 0 {
            let source = self.row_range(last);

            self.values.copy_within(source, row * self.state_count);
        }

        if row != last {
            let last_initialized = self.is_initialized(last);
            let row_mask = 1_u64 << row;

            if last_initialized {
                self.initialized |= row_mask;
            } else {
                self.initialized &= !row_mask;
            }
        }

        self.initialized &= !(1_u64 << last);

        self.values.truncate(last * self.state_count);
        self.row_count -= 1;
    }
}

#[derive(Debug)]
pub(crate) struct PreparedStateInsert {
    location: DeviceLocation,
    new_chunk: Option<StateChunk>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct PhysicalStateStore {
    chunks: Vec<StateChunk>,

    #[cfg(test)]
    semantic_read_count: std::cell::Cell<usize>,
    #[cfg(test)]
    physical_read_count: std::cell::Cell<usize>,
}

impl PhysicalStateStore {
    pub(crate) fn prepare_add_device(
        &mut self,
        definition: &DeviceDefinition,
        model: &PreparedDeviceInsert,
    ) -> PreparedStateInsert {
        let location = model.location();

        let chunk_index = location.chunk_index() as usize;
        let row = location.row() as usize;
        let state_count = definition.state_count();

        let new_chunk = if model.created_chunk() {
            assert_eq!(
                chunk_index,
                self.chunks.len(),
                "model/state chunk arrays must remain positionally aligned",
            );
            assert_eq!(row, 0);

            self.chunks.reserve(1);

            let mut chunk = StateChunk::new(state_count);
            chunk.reserve_rows(1);

            Some(chunk)
        } else {
            let chunk = self
                .chunks
                .get_mut(chunk_index)
                .expect("existing model chunk must have an aligned state chunk");

            assert_eq!(
                chunk.state_count, state_count,
                "homogeneous model/state chunks must have matching state stride",
            );

            assert_eq!(
                chunk.row_count(),
                row,
                "prepared model row must append at the aligned state row",
            );

            chunk.reserve_rows(1);

            None
        };

        PreparedStateInsert {
            location,
            new_chunk,
        }
    }

    #[inline]
    fn state_not_initialized(device: DeviceId, state_index: usize) -> PhysicalStateError {
        PhysicalStateError::StateNotInitialized {
            state: DeviceState::new(
                device,
                DefinitionStateId::new(
                    u32::try_from(state_index)
                        .expect("definition state index must fit DefinitionStateId"),
                ),
            ),
        }
    }

    pub(crate) fn commit_add_device(
        &mut self,
        prepared: PreparedStateInsert,
        insert: DeviceInsertResult,
    ) {
        debug_assert_eq!(insert.location(), prepared.location);

        let chunk_index = insert.chunk_index() as usize;
        let row = insert.row() as usize;

        if let Some(chunk) = prepared.new_chunk {
            debug_assert!(insert.created_chunk());
            debug_assert_eq!(chunk_index, self.chunks.len());

            self.chunks.push(chunk);
        } else {
            debug_assert!(!insert.created_chunk());
        }

        let chunk = self
            .chunks
            .get_mut(chunk_index)
            .expect("committed model chunk must have an aligned state chunk");

        debug_assert_eq!(chunk.row_count(), row);

        chunk.push_row();
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn get(&self, network: &Network, state: DeviceState) -> Option<f64> {
        #[cfg(test)]
        self.semantic_read_count
            .set(self.semantic_read_count.get() + 1);

        let location = network.device_location(state.device()).ok()?;

        let chunk = self.chunks.get(location.chunk_index() as usize)?;
        let row = location.row() as usize;

        if row >= chunk.row_count() {
            return None;
        }

        chunk.row_values(row).get(state.state().index()).copied()
    }

    #[inline]
    #[cfg(debug_assertions)]
    pub(crate) fn get_at(&self, address: PhysicalStateAddress) -> Option<f64> {
        #[cfg(test)]
        self.physical_read_count
            .set(self.physical_read_count.get() + 1);

        let chunk = self.chunks.get(address.location.chunk_index() as usize)?;
        let row = address.location.row() as usize;

        if row >= chunk.row_count() {
            return None;
        }

        chunk.row_values(row).get(address.state_index).copied()
    }

    pub(crate) fn remove_device(&mut self, removal: DeviceRemoveResult) {
        let chunk_index = removal.removed_chunk() as usize;
        let row = removal.removed_row() as usize;

        let chunk = self
            .chunks
            .get_mut(chunk_index)
            .expect("removed model chunk must have an aligned state chunk");

        chunk.remove_row(row);

        if chunk.row_count() != 0 {
            debug_assert!(
                removal.chunk_relocation().is_none(),
                "model chunk relocation is only valid when the removed chunk became empty",
            );

            return;
        }

        match removal.chunk_relocation() {
            Some(relocation) => {
                let from = relocation.from_chunk() as usize;
                let to = relocation.to_chunk() as usize;

                assert_eq!(to, chunk_index);
                assert_eq!(
                    from,
                    self.chunks.len() - 1,
                    "model chunk swap-remove must move the final chunk",
                );

                self.chunks.swap_remove(chunk_index);
            }

            None => {
                assert_eq!(
                    chunk_index,
                    self.chunks.len() - 1,
                    "empty non-final model chunk must report its relocation",
                );

                self.chunks.pop();
            }
        }
    }

    #[inline]
    pub(crate) fn read_logical_at(
        &self,
        _network: &Network,
        device: DeviceId,
        address: PhysicalStateAddress,
        initializer: DefinitionStateInitializer,
    ) -> Result<f64, PhysicalStateError> {
        let chunk = self
            .chunks
            .get(address.location().chunk_index() as usize)
            .ok_or_else(|| Self::state_not_initialized(device, address.state_index()))?;

        let row = address.location().row() as usize;
        let state_index = address.state_index();

        if row >= chunk.row_count() || state_index >= chunk.state_count {
            return Err(Self::state_not_initialized(device, state_index));
        }

        if chunk.is_initialized(row) {
            #[cfg(test)]
            self.physical_read_count
                .set(self.physical_read_count.get() + 1);

            return Ok(chunk.row_values(row)[state_index]);
        }

        match initializer {
            DefinitionStateInitializer::Literal(value) => Ok(value),
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub(crate) fn assert_aligned(&self, definitions: &DefinitionRegistry, network: &Network) {
        let mut expected_rows = Vec::<usize>::new();

        for device in network.iter_device_ids() {
            let location = network
                .device_location(device)
                .expect("iterated resident device must have a physical location");

            let chunk_index = location.chunk_index() as usize;
            let row = location.row() as usize;

            if expected_rows.len() <= chunk_index {
                expected_rows.resize(chunk_index + 1, 0);
            }

            expected_rows[chunk_index] += 1;

            let definition_id = network
                .device_definition_id(device)
                .expect("iterated resident device must have a definition");

            let definition = definitions
                .get(definition_id)
                .expect("resident device definition must remain registered");

            let chunk = self
                .chunks
                .get(chunk_index)
                .expect("resident model chunk must have a state sidecar chunk");

            assert_eq!(
                chunk.state_count,
                definition.state_count(),
                "state sidecar stride disagrees with model definition",
            );

            assert!(
                row < chunk.row_count(),
                "resident model row is outside state sidecar chunk",
            );

            assert_eq!(
                chunk.row_values(row).len(),
                definition.state_count(),
                "state row payload width disagrees with model definition",
            );
        }

        assert_eq!(
            self.chunks.len(),
            expected_rows.len(),
            "state sidecar chunk count disagrees with resident model chunks",
        );

        for (chunk_index, chunk) in self.chunks.iter().enumerate() {
            let row_count = chunk.row_count();

            assert!(
                (1..=DEVICE_CHUNK_ROWS).contains(&row_count),
                "resident state chunks must contain between 1 and 64 rows",
            );

            assert_eq!(
                row_count, expected_rows[chunk_index],
                "state sidecar row count disagrees with model chunk",
            );

            assert_eq!(
                chunk.values.len(),
                row_count * chunk.state_count,
                "state payload length disagrees with row count and stride",
            );

            assert!(
                chunk.values.iter().all(|value| value.is_finite()),
                "physical state must remain finite",
            );

            let live_mask = if row_count == DEVICE_CHUNK_ROWS {
                u64::MAX
            } else {
                (1_u64 << row_count) - 1
            };

            assert_eq!(
                chunk.initialized & !live_mask,
                0,
                "initialized bits must not exist outside live rows",
            );
        }
    }

    #[inline]
    pub(crate) fn validate_address(
        &self,
        state: DeviceState,
        address: PhysicalStateAddress,
    ) -> Result<(), PhysicalStateError> {
        if address.state_index() != state.state().index() {
            return Err(PhysicalStateError::StateNotInitialized { state });
        }

        let Some(chunk) = self.chunks.get(address.location().chunk_index() as usize) else {
            return Err(PhysicalStateError::StateNotInitialized { state });
        };

        let row = address.location().row() as usize;

        if row >= chunk.row_count() || address.state_index() >= chunk.state_count {
            return Err(PhysicalStateError::StateNotInitialized { state });
        }

        Ok(())
    }

    #[inline]
    pub(crate) fn write_prevalidated(&mut self, address: PhysicalStateAddress, value: f64) {
        debug_assert!(value.is_finite(), "committed physical state must be finite",);

        let chunk = self
            .chunks
            .get_mut(address.location().chunk_index() as usize)
            .expect("prevalidated state chunk must remain resident");

        let row = address.location().row() as usize;

        debug_assert!(
            row < chunk.row_count(),
            "prevalidated state row must remain resident",
        );

        debug_assert!(
            address.state_index() < chunk.state_count,
            "prevalidated state index must remain valid",
        );

        chunk.row_values_mut(row)[address.state_index()] = value;
    }

    #[inline]
    pub(crate) fn mark_initialized_prevalidated(&mut self, location: DeviceLocation) {
        let chunk = self
            .chunks
            .get_mut(location.chunk_index() as usize)
            .expect("prevalidated state chunk must remain resident");

        let row = location.row() as usize;

        debug_assert!(
            row < chunk.row_count(),
            "prevalidated state row must remain resident",
        );

        chunk.set_initialized(row);
    }
}

#[cfg(test)]
impl PhysicalStateStore {
    #[inline]
    pub(crate) fn is_initialized_at(&self, location: DeviceLocation) -> bool {
        let chunk = self
            .chunks
            .get(location.chunk_index() as usize)
            .expect("tested physical state chunk must exist");

        let row = location.row() as usize;

        assert!(
            row < chunk.row_count(),
            "tested physical state row must exist",
        );

        chunk.is_initialized(row)
    }

    #[inline]
    pub(crate) fn reset_read_counts(&self) {
        self.semantic_read_count.set(0);
        self.physical_read_count.set(0);
    }

    #[inline]
    pub(crate) fn semantic_read_count(&self) -> usize {
        self.semantic_read_count.get()
    }

    #[inline]
    pub(crate) fn physical_read_count(&self) -> usize {
        self.physical_read_count.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysicalStateAddress {
    location: DeviceLocation,
    state_index: usize,
}

impl PhysicalStateAddress {
    #[inline]
    pub(crate) const fn new(location: DeviceLocation, state_index: usize) -> Self {
        Self {
            location,
            state_index,
        }
    }

    #[inline]
    pub(crate) const fn location(self) -> DeviceLocation {
        self.location
    }

    #[inline]
    pub(crate) const fn state_index(self) -> usize {
        self.state_index
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalStateError {
    #[error("state {state:?} is not initialized")]
    StateNotInitialized { state: DeviceState },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::definition::DefinitionStateId;
    use hynergy_model::device::definition::{DefinitionId, DeviceId, PrimitiveElementKind};
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::Network;

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    fn state_key(device: DeviceId) -> DeviceState {
        DeviceState::new(device, DefinitionStateId::new(0))
    }

    fn add_stateful_device(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        state: &mut PhysicalStateStore,
        device: DeviceId,
        kind: PrimitiveElementKind,
    ) {
        let definition_id = DefinitionId::from(kind);
        let definition = definitions.get(definition_id).unwrap();

        let model_insert = network
            .prepare_add_device(definitions, device, definition_id)
            .unwrap();

        let state_insert = state.prepare_add_device(definition, &model_insert);
        let insert = network.commit_add_device(model_insert);

        state.commit_add_device(state_insert, insert);
    }

    fn commit_test_state(
        physical_state: &mut PhysicalStateStore,
        network: &Network,
        state: DeviceState,
        value: f64,
    ) {
        let location = network.device_location(state.device()).unwrap();

        let address = PhysicalStateAddress::new(location, state.state().index());

        physical_state.validate_address(state, address).unwrap();
        physical_state.write_prevalidated(address, value);
        physical_state.mark_initialized_prevalidated(location);
    }

    #[test]
    fn state_chunk_middle_row_removal_moves_values_and_initialized_bit() {
        let mut chunk = StateChunk::new(2);

        chunk.push_row();
        chunk.push_row();
        chunk.push_row();

        chunk.row_values_mut(0).copy_from_slice(&[10.0, 11.0]);
        chunk.row_values_mut(1).copy_from_slice(&[20.0, 21.0]);
        chunk.row_values_mut(2).copy_from_slice(&[30.0, 31.0]);

        chunk.set_initialized(0);
        chunk.set_initialized(2);

        chunk.remove_row(1);

        assert_eq!(chunk.row_count(), 2);
        assert_eq!(chunk.row_values(0), &[10.0, 11.0]);
        assert_eq!(chunk.row_values(1), &[30.0, 31.0]);

        assert!(chunk.is_initialized(0));
        assert!(chunk.is_initialized(1));
    }

    #[test]
    fn stateless_state_chunk_tracks_rows_without_value_payload() {
        let mut chunk = StateChunk::new(0);

        for _ in 0..3 {
            chunk.push_row();
        }

        assert_eq!(chunk.row_count(), 3);
        assert!(chunk.values.is_empty());

        chunk.remove_row(1);

        assert_eq!(chunk.row_count(), 2);
        assert!(chunk.values.is_empty());
    }

    #[test]
    fn physical_state_row_swap_preserves_moved_device_history() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        for raw in 1..=3 {
            add_stateful_device(
                &definitions,
                &mut network,
                &mut state,
                device(raw),
                PrimitiveElementKind::TickDelay,
            );
        }

        commit_test_state(&mut state, &network, state_key(device(1)), 10.0);
        commit_test_state(&mut state, &network, state_key(device(2)), 20.0);
        commit_test_state(&mut state, &network, state_key(device(3)), 30.0);

        let removal = network.remove_device(device(2)).unwrap();

        assert_eq!(removal.moved_device(), Some(device(3)));

        state.remove_device(removal);

        assert_eq!(state.get(&network, state_key(device(3))), Some(30.0),);

        let location = network.device_location(device(3)).unwrap();

        assert!(
            state.chunks[location.chunk_index() as usize].is_initialized(location.row() as usize),
        );
    }

    #[test]
    fn physical_state_chunk_swap_preserves_all_moved_rows() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        add_stateful_device(
            &definitions,
            &mut network,
            &mut state,
            device(1),
            PrimitiveElementKind::TickDelay,
        );

        for raw in 2..=3 {
            add_stateful_device(
                &definitions,
                &mut network,
                &mut state,
                device(raw),
                PrimitiveElementKind::SchmittBuffer,
            );
        }

        commit_test_state(&mut state, &network, state_key(device(1)), 10.0);
        commit_test_state(&mut state, &network, state_key(device(2)), 20.0);
        commit_test_state(&mut state, &network, state_key(device(3)), 30.0);

        let removal = network.remove_device(device(1)).unwrap();

        assert!(removal.chunk_relocation().is_some());

        state.remove_device(removal);

        assert_eq!(state.get(&network, state_key(device(2))), Some(20.0),);
        assert_eq!(state.get(&network, state_key(device(3))), Some(30.0),);
    }

    #[test]
    fn physical_state_address_reads_bound_chunk_row_directly() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        let device = device(1);
        let definition_id = DefinitionId::from(PrimitiveElementKind::TickDelay);
        let definition = definitions.get(definition_id).unwrap();

        let model_insert = network
            .prepare_add_device(&definitions, device, definition_id)
            .unwrap();

        let state_insert = state.prepare_add_device(definition, &model_insert);
        let insert = network.commit_add_device(model_insert);

        state.commit_add_device(state_insert, insert);

        let semantic = DeviceState::new(device, DefinitionStateId::new(0));

        commit_test_state(&mut state, &network, semantic, 12.5);

        let address = PhysicalStateAddress::new(network.device_location(device).unwrap(), 0);

        assert_eq!(state.get_at(address), Some(12.5),);
    }

    #[test]
    fn physical_state_scatter_does_not_initialize_until_finalization() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        let device = device(1);
        let definition_id = DefinitionId::from(PrimitiveElementKind::TickDelay);

        let definition = definitions.get(definition_id).unwrap();

        let model_insert = network
            .prepare_add_device(&definitions, device, definition_id)
            .unwrap();

        let state_insert = state.prepare_add_device(definition, &model_insert);
        let insert = network.commit_add_device(model_insert);

        state.commit_add_device(state_insert, insert);

        let semantic = DeviceState::new(device, DefinitionStateId::new(0));
        let location = network.device_location(device).unwrap();
        let address = PhysicalStateAddress::new(location, 0);

        state.validate_address(semantic, address).unwrap();
        state.write_prevalidated(address, 7.5);

        assert_eq!(state.get_at(address), Some(7.5),);

        assert!(
            !state.is_initialized_at(location),
            "scattering next-state values must not make them visible as committed state yet",
        );

        state.mark_initialized_prevalidated(location);

        assert!(
            state.is_initialized_at(location),
            "initialization is finalized only after every scalar write has completed",
        );
    }
}
