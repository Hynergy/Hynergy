use crate::device::definition::{DefinitionId, DeviceDefinition, DeviceId, TerminalId};
use crate::device::registry::DefinitionRegistry;
use crate::network::{ConnectionRef, ConnectionType, NetworkModelError};
use crate::parameter::ParameterId;
use std::fmt;

pub(super) const DEVICE_CHUNK_CAPACITY: usize = 64;

const DEVICE_ROW_BITS: u32 = 6;
const DEVICE_ROW_MASK: u32 = (1 << DEVICE_ROW_BITS) - 1;

pub(super) const MAX_DEVICE_CHUNKS: usize = 1 << 25;
pub(super) const UNASSIGNED_PARAMETER: f64 = f64::INFINITY;

const FIRST_UNLOADED_ENTRY: u32 = 0x8000_0001;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct DeviceLocator(u32);

impl DeviceLocator {
    #[inline]
    pub(super) fn new(chunk_index: usize, row: usize) -> Option<Self> {
        if chunk_index >= MAX_DEVICE_CHUNKS || row >= DEVICE_CHUNK_CAPACITY {
            return None;
        }

        Some(Self(((chunk_index as u32) << DEVICE_ROW_BITS) | row as u32))
    }

    #[inline]
    pub(super) const fn chunk_index(self) -> usize {
        (self.0 >> DEVICE_ROW_BITS) as usize
    }

    #[inline]
    pub(super) const fn row(self) -> usize {
        (self.0 & DEVICE_ROW_MASK) as usize
    }
}

impl fmt::Debug for DeviceLocator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceLocator")
            .field("chunk", &self.chunk_index())
            .field("row", &self.row())
            .finish()
    }
}

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct DeviceDirectoryEntry(u32);

impl DeviceDirectoryEntry {
    const UNASSIGNED: Self = Self(0);

    #[cfg(test)]
    #[inline]
    const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[cfg(test)]
    #[inline]
    const fn raw(self) -> u32 {
        self.0
    }

    #[inline]
    pub(super) const fn resident(locator: DeviceLocator) -> Self {
        Self(locator.0 + 1)
    }

    #[inline]
    pub(super) const fn is_unassigned(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub(super) const fn resident_locator(self) -> Option<DeviceLocator> {
        if self.0 == 0 || self.0 >= FIRST_UNLOADED_ENTRY {
            return None;
        }

        Some(DeviceLocator(self.0 - 1))
    }

    #[inline]
    const fn unloaded_record_index(self) -> Option<u32> {
        if self.0 < FIRST_UNLOADED_ENTRY {
            return None;
        }

        Some(self.0 - FIRST_UNLOADED_ENTRY)
    }
}

#[derive(Debug, Clone)]
pub(super) struct DeviceChunk {
    definition_id: DefinitionId,
    terminal_count: usize,
    parameter_count: usize,

    device_ids: Vec<DeviceId>,
    terminals: Vec<Option<ConnectionRef>>,
    parameters: Vec<f64>,
}

impl DeviceChunk {
    #[inline]
    pub(super) fn new(definition_id: DefinitionId, definition: &DeviceDefinition) -> Self {
        Self {
            definition_id,
            terminal_count: definition.terminals().len(),
            parameter_count: definition.parameters().len(),
            device_ids: Vec::new(),
            terminals: Vec::new(),
            parameters: Vec::new(),
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.device_ids.len()
    }

    #[inline]
    pub(super) fn is_full(&self) -> bool {
        self.len() == DEVICE_CHUNK_CAPACITY
    }

    #[cfg(test)]
    #[inline]
    pub(super) fn device_ids(&self) -> &[DeviceId] {
        &self.device_ids
    }

    #[inline]
    fn reserve_rows(&mut self, additional: usize) {
        self.device_ids.reserve(additional);
        self.terminals
            .reserve(additional.saturating_mul(self.terminal_count));
        self.parameters
            .reserve(additional.saturating_mul(self.parameter_count));
    }

    pub(super) fn push_empty_row(
        &mut self,
        device: DeviceId,
        definition: &DeviceDefinition,
    ) -> bool {
        if self.is_full() {
            return false;
        }

        debug_assert_eq!(self.terminal_count, definition.terminals().len());
        debug_assert_eq!(self.parameter_count, definition.parameters().len());

        self.device_ids.push(device);

        self.terminals
            .extend(std::iter::repeat_n(None, self.terminal_count));

        self.parameters.extend(std::iter::repeat_n(
            UNASSIGNED_PARAMETER,
            self.parameter_count,
        ));

        true
    }

    #[inline]
    fn terminal_index(&self, row: usize, terminal: TerminalId) -> Option<usize> {
        (terminal.index() < self.terminal_count)
            .then_some(row * self.terminal_count + terminal.index())
    }

    #[inline]
    fn parameter_index(&self, row: usize, parameter: ParameterId) -> Option<usize> {
        (parameter.index() < self.parameter_count)
            .then_some(row * self.parameter_count + parameter.index())
    }

    #[inline]
    pub(super) fn device_view(&self, row: usize) -> DeviceView<'_> {
        debug_assert!(row < self.len());

        DeviceView { chunk: self, row }
    }

    fn remove_row(&mut self, row: usize) -> Option<DeviceId> {
        let row_count = self.len();
        assert!(row < row_count);

        let last_row = row_count - 1;

        let moved_device = if row != last_row {
            let moved = self.device_ids[last_row];
            self.device_ids[row] = moved;
            Some(moved)
        } else {
            None
        };

        swap_remove_stride(&mut self.terminals, row, row_count, self.terminal_count);

        swap_remove_stride(&mut self.parameters, row, row_count, self.parameter_count);

        self.device_ids.pop();

        moved_device
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceView<'a> {
    chunk: &'a DeviceChunk,
    row: usize,
}

impl DeviceView<'_> {
    #[inline]
    pub const fn definition_id(&self) -> DefinitionId {
        self.chunk.definition_id
    }

    #[inline]
    pub fn terminals(&self) -> &[Option<ConnectionRef>] {
        let start = self.row * self.chunk.terminal_count;
        let end = start + self.chunk.terminal_count;

        &self.chunk.terminals[start..end]
    }

    #[inline]
    pub fn parameter(&self, parameter: ParameterId) -> Option<Option<f64>> {
        let index = self.chunk.parameter_index(self.row, parameter)?;

        Some(decode_parameter(self.chunk.parameters[index]))
    }

    #[inline]
    pub fn parameters(&self) -> impl ExactSizeIterator<Item = Option<f64>> + '_ {
        let start = self.row * self.chunk.parameter_count;
        let end = start + self.chunk.parameter_count;

        self.chunk.parameters[start..end]
            .iter()
            .copied()
            .map(decode_parameter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceInsertResult {
    chunk_index: u32,
    row: u8,
    created_chunk: bool,
}

impl DeviceInsertResult {
    #[inline]
    pub const fn chunk_index(self) -> u32 {
        self.chunk_index
    }

    #[inline]
    pub const fn row(self) -> u8 {
        self.row
    }

    #[inline]
    pub const fn created_chunk(self) -> bool {
        self.created_chunk
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceChunkRelocation {
    from_chunk: u32,
    to_chunk: u32,
}

impl DeviceChunkRelocation {
    #[inline]
    pub const fn from_chunk(self) -> u32 {
        self.from_chunk
    }

    #[inline]
    pub const fn to_chunk(self) -> u32 {
        self.to_chunk
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRemoveResult {
    removed_chunk: u32,
    removed_row: u8,
    moved_device: Option<DeviceId>,
    chunk_relocation: Option<DeviceChunkRelocation>,
}

impl DeviceRemoveResult {
    #[inline]
    pub const fn removed_chunk(self) -> u32 {
        self.removed_chunk
    }

    #[inline]
    pub const fn removed_row(self) -> u8 {
        self.removed_row
    }

    #[inline]
    pub const fn moved_device(self) -> Option<DeviceId> {
        self.moved_device
    }

    #[inline]
    pub const fn chunk_relocation(self) -> Option<DeviceChunkRelocation> {
        self.chunk_relocation
    }
}

#[derive(Debug, Default, Clone)]
pub(super) struct DeviceArena {
    directory: Vec<DeviceDirectoryEntry>,
    chunks: Vec<DeviceChunk>,
    active_chunks: Vec<Option<u32>>,
}

impl DeviceArena {
    #[inline]
    pub(super) fn with_directory_capacity(capacity: usize) -> Self {
        Self {
            directory: Vec::with_capacity(capacity),
            chunks: Vec::new(),
            active_chunks: Vec::new(),
        }
    }

    #[inline]
    pub(super) fn directory_len(&self) -> usize {
        self.directory.len()
    }

    #[inline]
    pub(super) fn is_assigned(&self, device: DeviceId) -> bool {
        self.directory
            .get(device.index())
            .is_some_and(|entry| !entry.is_unassigned())
    }

    #[inline]
    pub(super) fn resident_locator(
        &self,
        device: DeviceId,
    ) -> Result<DeviceLocator, NetworkModelError> {
        self.directory
            .get(device.index())
            .copied()
            .and_then(DeviceDirectoryEntry::resident_locator)
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device.id(),
            })
    }

    fn ensure_active_slot(&mut self, definition: DefinitionId) {
        if self.active_chunks.len() <= definition.index() {
            self.active_chunks.resize(definition.index() + 1, None);
        }
    }

    fn insertion_chunk(&mut self, definition: DefinitionId) -> Option<usize> {
        self.ensure_active_slot(definition);

        if let Some(index) = self.active_chunks[definition.index()].map(|index| index as usize) {
            if self
                .chunks
                .get(index)
                .is_some_and(|chunk| chunk.definition_id == definition && !chunk.is_full())
            {
                return Some(index);
            }

            self.active_chunks[definition.index()] = None;
        }

        let found = self
            .chunks
            .iter()
            .position(|chunk| chunk.definition_id == definition && !chunk.is_full());

        if let Some(index) = found {
            self.active_chunks[definition.index()] = Some(index as u32);
        }

        found
    }

    pub(super) fn insert(
        &mut self,
        device: DeviceId,
        definition_id: DefinitionId,
        definition: &DeviceDefinition,
    ) -> Result<DeviceInsertResult, NetworkModelError> {
        let device_index = device.index();

        debug_assert!(device_index <= self.directory.len());
        debug_assert!(
            device_index == self.directory.len() || self.directory[device_index].is_unassigned()
        );

        let mut created_chunk = false;

        let chunk_index = if let Some(index) = self.insertion_chunk(definition_id) {
            self.chunks[index].reserve_rows(1);
            index
        } else {
            if self.chunks.len() >= MAX_DEVICE_CHUNKS {
                return Err(NetworkModelError::DeviceArenaExhausted);
            }

            let mut chunk = DeviceChunk::new(definition_id, definition);
            chunk.reserve_rows(1);

            self.chunks.reserve(1);

            let index = self.chunks.len();
            self.chunks.push(chunk);

            created_chunk = true;
            index
        };

        if device_index == self.directory.len() {
            self.directory.reserve(1);
        }

        let row = self.chunks[chunk_index].len();

        let inserted = self.chunks[chunk_index].push_empty_row(device, definition);
        debug_assert!(inserted);

        let locator = DeviceLocator::new(chunk_index, row)
            .expect("selected chunk and row must fit packed device locator");

        let entry = DeviceDirectoryEntry::resident(locator);

        if device_index == self.directory.len() {
            self.directory.push(entry);
        } else {
            self.directory[device_index] = entry;
        }

        self.ensure_active_slot(definition_id);

        self.active_chunks[definition_id.index()] = if self.chunks[chunk_index].is_full() {
            None
        } else {
            Some(chunk_index as u32)
        };

        Ok(DeviceInsertResult {
            chunk_index: u32::try_from(chunk_index).expect("packed chunk index must fit u32"),
            row: u8::try_from(row).expect("device row must fit u8"),
            created_chunk,
        })
    }

    #[inline]
    pub(super) fn device(&self, device: DeviceId) -> Result<DeviceView<'_>, NetworkModelError> {
        let locator = self.resident_locator(device)?;

        Ok(self.chunks[locator.chunk_index()].device_view(locator.row()))
    }

    #[inline]
    pub(super) fn iter_devices(&self) -> impl Iterator<Item = (DeviceId, DeviceView<'_>)> + '_ {
        self.chunks.iter().flat_map(|chunk| {
            chunk
                .device_ids
                .iter()
                .copied()
                .enumerate()
                .map(move |(row, device)| (device, chunk.device_view(row)))
        })
    }

    pub(super) fn set_parameter(
        &mut self,
        device: DeviceId,
        parameter: ParameterId,
        value: f64,
    ) -> Result<(), NetworkModelError> {
        debug_assert!(value.is_finite());

        let locator = self.resident_locator(device)?;
        let chunk = &mut self.chunks[locator.chunk_index()];

        let index = chunk
            .parameter_index(locator.row(), parameter)
            .ok_or(NetworkModelError::InvalidParameter { parameter })?;

        chunk.parameters[index] = value;

        Ok(())
    }

    pub(super) fn terminal_connection(
        &self,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<Option<ConnectionRef>, NetworkModelError> {
        let locator = self.resident_locator(device)?;
        let chunk = &self.chunks[locator.chunk_index()];

        let index = chunk
            .terminal_index(locator.row(), terminal)
            .ok_or(NetworkModelError::InvalidTerminal)?;

        Ok(chunk.terminals[index])
    }

    pub(super) fn attach_terminal(
        &mut self,
        device: DeviceId,
        terminal: TerminalId,
        connection: ConnectionRef,
    ) -> Result<(), NetworkModelError> {
        let locator = self.resident_locator(device)?;
        let chunk = &mut self.chunks[locator.chunk_index()];

        let index = chunk
            .terminal_index(locator.row(), terminal)
            .ok_or(NetworkModelError::InvalidTerminal)?;

        if chunk.terminals[index].is_some() {
            return Err(NetworkModelError::TerminalAlreadyConnected);
        }

        chunk.terminals[index] = Some(connection);

        Ok(())
    }

    pub(super) fn detach_terminal(
        &mut self,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<Option<ConnectionRef>, NetworkModelError> {
        let locator = self.resident_locator(device)?;
        let chunk = &mut self.chunks[locator.chunk_index()];

        let index = chunk
            .terminal_index(locator.row(), terminal)
            .ok_or(NetworkModelError::InvalidTerminal)?;

        Ok(chunk.terminals[index].take())
    }

    pub(super) fn remove(
        &mut self,
        device: DeviceId,
    ) -> Result<DeviceRemoveResult, NetworkModelError> {
        let locator = self.resident_locator(device)?;
        let chunk_index = locator.chunk_index();
        let row = locator.row();

        let removed_chunk =
            u32::try_from(chunk_index).expect("packed device chunk index must fit u32");
        let removed_row = u8::try_from(row).expect("device chunk row must fit u8");

        let removed_definition = self.chunks[chunk_index].definition_id;

        let moved_device = self.chunks[chunk_index].remove_row(row);

        self.directory[device.index()] = DeviceDirectoryEntry::UNASSIGNED;

        if let Some(moved_device) = moved_device {
            self.directory[moved_device.index()] = DeviceDirectoryEntry::resident(locator);
        }

        if !self.chunks[chunk_index].device_ids.is_empty() {
            self.active_chunks[removed_definition.index()] = Some(removed_chunk);

            return Ok(DeviceRemoveResult {
                removed_chunk,
                removed_row,
                moved_device,
                chunk_relocation: None,
            });
        }

        if self.active_chunks[removed_definition.index()] == Some(removed_chunk) {
            self.active_chunks[removed_definition.index()] = None;
        }

        let last_chunk_index = self.chunks.len() - 1;

        if chunk_index == last_chunk_index {
            self.chunks.pop();

            return Ok(DeviceRemoveResult {
                removed_chunk,
                removed_row,
                moved_device,
                chunk_relocation: None,
            });
        }

        let moved_from_chunk =
            u32::try_from(last_chunk_index).expect("packed device chunk index must fit u32");

        let moved_definition = self.chunks[last_chunk_index].definition_id;

        self.chunks.swap_remove(chunk_index);

        for (moved_row, &moved_device) in self.chunks[chunk_index].device_ids.iter().enumerate() {
            let moved_locator = DeviceLocator::new(chunk_index, moved_row)
                .expect("moved chunk row must fit packed device locator");

            self.directory[moved_device.index()] = DeviceDirectoryEntry::resident(moved_locator);
        }

        if self.active_chunks[moved_definition.index()] == Some(moved_from_chunk) {
            self.active_chunks[moved_definition.index()] = Some(removed_chunk);
        }

        Ok(DeviceRemoveResult {
            removed_chunk,
            removed_row,
            moved_device,
            chunk_relocation: Some(DeviceChunkRelocation {
                from_chunk: moved_from_chunk,
                to_chunk: removed_chunk,
            }),
        })
    }

    #[cfg(any(test, debug_assertions))]
    pub(super) fn assert_consistent(&self, definitions: &DefinitionRegistry) {
        for (chunk_index, chunk) in self.chunks.iter().enumerate() {
            assert!(!chunk.device_ids.is_empty());
            assert!(chunk.len() <= DEVICE_CHUNK_CAPACITY);

            let definition = definitions
                .get(chunk.definition_id)
                .expect("chunk definition must remain registered");

            assert_eq!(chunk.terminal_count, definition.terminals().len());
            assert_eq!(chunk.parameter_count, definition.parameters().len());

            assert_eq!(chunk.device_ids.len(), chunk.len());
            assert_eq!(chunk.terminals.len(), chunk.len() * chunk.terminal_count);
            assert_eq!(chunk.parameters.len(), chunk.len() * chunk.parameter_count);

            assert!(
                chunk
                    .parameters
                    .iter()
                    .all(|&value| { value.is_finite() || value == UNASSIGNED_PARAMETER })
            );

            for (row, &device) in chunk.device_ids.iter().enumerate() {
                let expected = DeviceLocator::new(chunk_index, row).unwrap();

                assert_eq!(
                    self.directory
                        .get(device.index())
                        .and_then(|entry| entry.resident_locator()),
                    Some(expected),
                );
            }
        }

        for (device_index, entry) in self.directory.iter().copied().enumerate() {
            let Some(locator) = entry.resident_locator() else {
                if !entry.is_unassigned() {
                    assert!(entry.unloaded_record_index().is_some());
                }

                continue;
            };

            let chunk = self
                .chunks
                .get(locator.chunk_index())
                .expect("resident directory entry must reference a live chunk");

            let raw = u32::try_from(device_index + 1).expect("directory index must fit DeviceId");

            let device = DeviceId::try_from(raw).expect("DeviceId values are one-based");

            assert_eq!(chunk.device_ids.get(locator.row()).copied(), Some(device),);
        }

        for (definition_index, hint) in self.active_chunks.iter().copied().enumerate() {
            let Some(chunk_index) = hint else {
                continue;
            };

            let chunk = self
                .chunks
                .get(chunk_index as usize)
                .expect("active chunk hint must reference a live chunk");

            assert_eq!(chunk.definition_id.index(), definition_index);
            assert!(!chunk.is_full());
        }
    }

    #[inline]
    pub(super) fn iter_device_ids(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.device_ids.iter().copied())
    }

    #[cfg(test)]
    #[inline]
    pub(super) fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    #[cfg(test)]
    #[inline]
    pub(super) fn chunk_len(&self, chunk_index: usize) -> usize {
        self.chunks[chunk_index].len()
    }

    #[cfg(test)]
    #[inline]
    pub(super) fn chunk_definition(&self, chunk_index: usize) -> DefinitionId {
        self.chunks[chunk_index].definition_id
    }
}

#[inline]
fn swap_remove_stride<T: Copy>(values: &mut Vec<T>, row: usize, row_count: usize, stride: usize) {
    if stride == 0 {
        return;
    }

    let last_row = row_count - 1;

    if row != last_row {
        let source_start = last_row * stride;
        let source_end = source_start + stride;

        values.copy_within(source_start..source_end, row * stride);
    }

    values.truncate(last_row * stride);
}

#[inline]
fn decode_parameter(value: f64) -> Option<f64> {
    if value == UNASSIGNED_PARAMETER {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::definition::{DefinitionId, DeviceId, PrimitiveElementKind};
    use crate::device::registry::DefinitionRegistry;
    use crate::parameter::ParameterId;
    use std::mem::size_of;

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    #[test]
    fn device_directory_entry_is_exactly_four_bytes() {
        assert_eq!(size_of::<DeviceDirectoryEntry>(), 4);
    }

    #[test]
    fn resident_locator_encoding_uses_lower_half_inclusively() {
        let first = DeviceLocator::new(0, 0).unwrap();
        let last = DeviceLocator::new(MAX_DEVICE_CHUNKS - 1, DEVICE_CHUNK_CAPACITY - 1).unwrap();

        let first_entry = DeviceDirectoryEntry::resident(first);
        let last_entry = DeviceDirectoryEntry::resident(last);

        assert_eq!(first_entry.raw(), 1);
        assert_eq!(last_entry.raw(), 0x8000_0000);
        assert_eq!(first_entry.resident_locator(), Some(first));
        assert_eq!(last_entry.resident_locator(), Some(last));
    }

    #[test]
    fn locator_rejects_out_of_range_chunk_or_row() {
        assert!(DeviceLocator::new(MAX_DEVICE_CHUNKS, 0).is_none());
        assert!(DeviceLocator::new(0, DEVICE_CHUNK_CAPACITY).is_none());
    }

    #[test]
    fn upper_half_is_reserved_for_unloaded_markers() {
        let unloaded = DeviceDirectoryEntry::from_raw(0x8000_0001);

        assert!(!unloaded.is_unassigned());
        assert_eq!(unloaded.resident_locator(), None);
        assert_eq!(unloaded.unloaded_record_index(), Some(0));
    }

    #[test]
    fn chunk_appends_fixed_stride_rows_without_eager_capacity_sixty_four() {
        let definitions = DefinitionRegistry::new();
        let definition_id = DefinitionId::from(PrimitiveElementKind::VoltageControlledConductance);
        let definition = definitions.get(definition_id).unwrap();
        let mut chunk = DeviceChunk::new(definition_id, definition);

        chunk.push_empty_row(device(1), definition);
        chunk.push_empty_row(device(2), definition);

        assert_eq!(chunk.len(), 2);
        assert_eq!(chunk.device_ids(), &[device(1), device(2)]);
        assert_eq!(chunk.terminals.len(), 2 * definition.terminals().len(),);
        assert_eq!(chunk.parameters.len(), 2 * definition.parameters().len(),);
        assert!(
            chunk
                .parameters
                .iter()
                .all(|&value| value == UNASSIGNED_PARAMETER)
        );

        assert!(chunk.device_ids.capacity() < DEVICE_CHUNK_CAPACITY);
    }

    #[test]
    fn semantic_device_view_hides_unassigned_parameter_sentinel() {
        let definitions = DefinitionRegistry::new();
        let definition_id = DefinitionId::from(PrimitiveElementKind::Conductance);
        let definition = definitions.get(definition_id).unwrap();
        let mut chunk = DeviceChunk::new(definition_id, definition);

        chunk.push_empty_row(device(1), definition);

        let view = chunk.device_view(0);

        assert_eq!(view.definition_id(), definition_id);
        assert_eq!(view.parameter(ParameterId::new(0)), Some(None));
        assert_eq!(view.parameters().collect::<Vec<_>>(), vec![None]);
    }

    #[test]
    fn chunk_rejects_a_sixty_fifth_row() {
        let definitions = DefinitionRegistry::new();
        let definition_id = DefinitionId::from(PrimitiveElementKind::Conductance);
        let definition = definitions.get(definition_id).unwrap();
        let mut chunk = DeviceChunk::new(definition_id, definition);

        for raw in 1..=DEVICE_CHUNK_CAPACITY as u32 {
            assert!(chunk.push_empty_row(device(raw), definition));
        }

        assert!(!chunk.push_empty_row(device(65), definition));
    }
}
