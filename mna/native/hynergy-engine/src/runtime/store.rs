use crate::runtime::island::IslandRuntime;
use crate::topology::IslandId;
use std::num::NonZeroU32;

#[derive(Debug, Default)]
pub(crate) struct IslandRuntimeStore {
    positions: Vec<Option<NonZeroU32>>,
    entries: Vec<(IslandId, IslandRuntime)>,
}

impl IslandRuntimeStore {
    #[inline]
    pub(crate) fn get(&self, id: IslandId) -> Option<&IslandRuntime> {
        let dense_index = self.dense_index(id)?;
        let (stored_id, runtime) = self.entries.get(dense_index)?;
        debug_assert_eq!(*stored_id, id);
        Some(runtime)
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, id: IslandId) -> Option<&mut IslandRuntime> {
        let dense_index = self.dense_index(id)?;
        let (stored_id, runtime) = self.entries.get_mut(dense_index)?;
        debug_assert_eq!(*stored_id, id);
        Some(runtime)
    }

    pub(crate) fn insert(&mut self, id: IslandId, runtime: IslandRuntime) -> Option<IslandRuntime> {
        if let Some(dense_index) = self.dense_index(id) {
            let (stored_id, stored_runtime) = self
                .entries
                .get_mut(dense_index)
                .expect("island runtime position must reference a dense entry");
            debug_assert_eq!(*stored_id, id);
            return Some(std::mem::replace(stored_runtime, runtime));
        }

        let slot = id.index();
        let required_len = slot
            .checked_add(1)
            .expect("island ID index must fit in the position table");
        if self.positions.len() < required_len {
            self.positions.resize_with(required_len, || None);
        }

        let dense_position = Self::encode_dense_position(self.entries.len());
        self.entries.push((id, runtime));
        self.positions[slot] = Some(dense_position);

        None
    }

    pub(crate) fn remove(&mut self, id: IslandId) -> Option<IslandRuntime> {
        let dense_index = self.dense_index(id)?;
        let (removed_id, runtime) = self.entries.swap_remove(dense_index);
        debug_assert_eq!(removed_id, id);
        self.positions[id.index()] = None;

        if dense_index < self.entries.len() {
            let moved_id = self.entries[dense_index].0;
            self.positions[moved_id.index()] = Some(Self::encode_dense_position(dense_index));
        }

        Some(runtime)
    }

    #[inline]
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut IslandRuntime> {
        self.entries.iter_mut().map(|(_, runtime)| runtime)
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    fn dense_index(&self, id: IslandId) -> Option<usize> {
        self.positions
            .get(id.index())
            .copied()
            .flatten()
            .map(|position| position.get() as usize - 1)
    }

    #[inline]
    fn encode_dense_position(dense_index: usize) -> NonZeroU32 {
        dense_index
            .checked_add(1)
            .and_then(|position| u32::try_from(position).ok())
            .and_then(NonZeroU32::new)
            .expect("island runtime store exceeded u32 dense position range")
    }
}

#[cfg(test)]
mod tests {
    use super::IslandRuntimeStore;
    use crate::runtime::island::{IslandRuntime, IslandTickStatus};
    use crate::topology::{DeviceComponent, IslandId};
    use crate::{World, WorldConfig};
    use hynergy_model::device::definition::{
        DeviceId, DevicePartitionId, PrimitiveElementKind, TerminalId,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::WireId;
    use hynergy_model::parameter::ParameterId;
    use std::num::NonZeroU32;

    fn live_runtimes() -> ([IslandId; 4], [IslandRuntime; 4]) {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(WorldConfig::new(NonZeroU32::new(30).unwrap()));
        let wires = [1, 2, 3, 4, 5, 6, 7, 8].map(|raw| WireId::try_from(raw).unwrap());
        let devices = [1, 2, 3, 4].map(|raw| DeviceId::try_from(raw).unwrap());

        for wire in wires {
            world.add_wire(wire).unwrap();
        }

        for (index, &device) in devices.iter().enumerate() {
            world
                .add_device(
                    &definitions,
                    device,
                    PrimitiveElementKind::Conductance.into(),
                )
                .unwrap();
            world
                .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0)
                .unwrap();
            world
                .attach_terminal(&definitions, wires[index * 2], device, TerminalId::new(0))
                .unwrap();
            world
                .attach_terminal(
                    &definitions,
                    wires[index * 2 + 1],
                    device,
                    TerminalId::new(1),
                )
                .unwrap();
        }

        world.tick(&definitions).unwrap();
        let islands = devices.map(|device| {
            world.derived_topology.component_island(
                &world.network,
                DeviceComponent::new(device, DevicePartitionId::new(0)),
            )
        });
        let runtimes = islands.map(|island| {
            world
                .island_runtimes
                .remove(island)
                .expect("each live test island must have a runtime")
        });
        (islands, runtimes)
    }

    #[test]
    fn removing_non_tail_runtime_repairs_moved_key() {
        let (islands, [first, middle, mut last, _]) = live_runtimes();
        let mut store = IslandRuntimeStore::default();
        last.mark_unavailable();
        store.insert(islands[0], first);
        store.insert(islands[1], middle);
        store.insert(islands[2], last);

        let removed = store.remove(islands[1]).unwrap();

        assert_eq!(removed.tick_status(), IslandTickStatus::Available);
        assert!(store.get(islands[1]).is_none());
        assert_eq!(
            store.get(islands[2]).unwrap().tick_status(),
            IslandTickStatus::Unavailable
        );
        assert_eq!(
            store.get(islands[0]).unwrap().tick_status(),
            IslandTickStatus::Available
        );
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn replacing_runtime_keeps_its_dense_position() {
        let (islands, [first, middle, last, mut replacement]) = live_runtimes();
        let mut store = IslandRuntimeStore::default();
        store.insert(islands[0], first);
        store.insert(islands[1], middle);
        store.insert(islands[2], last);

        let last_runtime = store.get(islands[2]).unwrap() as *const IslandRuntime;
        replacement.mark_unavailable();
        let replaced = store.insert(islands[1], replacement).unwrap();

        assert_eq!(replaced.tick_status(), IslandTickStatus::Available);
        assert_eq!(
            store.get(islands[1]).unwrap().tick_status(),
            IslandTickStatus::Unavailable
        );
        assert!(std::ptr::eq(
            store.get(islands[2]).unwrap() as *const IslandRuntime,
            last_runtime,
        ));
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn absent_removal_and_high_id_insertion_keep_live_entries_resolvable() {
        let (islands, [first, high_runtime, _, _]) = live_runtimes();
        let mut store = IslandRuntimeStore::default();
        store.insert(islands[0], first);

        let absent = IslandId::try_from(128).unwrap();
        assert!(store.remove(absent).is_none());
        assert_eq!(store.len(), 1);

        assert!(store.insert(absent, high_runtime).is_none());
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.get(islands[0]).unwrap().tick_status(),
            IslandTickStatus::Available
        );
        assert_eq!(
            store.get(absent).unwrap().tick_status(),
            IslandTickStatus::Available
        );
    }
}
