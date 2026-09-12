use hynergy_ids::define_non_zero_id;
use hynergy_model::device::definition::{DeviceId, TerminalId};
use hynergy_model::network::WireId;
use std::num::NonZeroU32;

define_non_zero_id!(NetId, IslandId);

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct DerivedTopology {
    nets: Vec<Net>,
    wire_net_map: Vec<Option<NetId>>,
    islands: Vec<IslandTopology>,
    device_island_map: Vec<Option<IslandId>>,
}

impl DerivedTopology {
    #[inline]
    pub(crate) fn add_wire(&mut self, wire: WireId) {
        debug_assert!(wire.index() <= self.wire_net_map.len());

        let net_id = unsafe { NetId::try_from(self.nets.len() as u32 + 1).unwrap_unchecked() };

        if let Some(slot) = self
            .wire_net_map
            .get_mut(wire.index())
            .and_then(Option::as_mut)
        {
            *slot = net_id;
        } else {
            self.wire_net_map.push(Some(net_id));
        }

        self.nets.push(Net::default());
    }

    #[inline]
    pub(crate) fn add_device(&mut self, device: DeviceId) {
        debug_assert!(device.index() <= self.device_island_map.len());

        let mut island = IslandTopology::default();
        island.add_device(device);

        let island_id =
            unsafe { IslandId::try_from((self.islands.len() as u32) + 1).unwrap_unchecked() };

        if let Some(slot) = self
            .device_island_map
            .get_mut(device.index())
            .and_then(Option::as_mut)
        {
            *slot = island_id;
        } else {
            self.device_island_map.push(Some(island_id));
        }

        self.islands.push(island);
    }

    #[inline]
    pub(crate) fn connect_wires(&mut self, wire_a: WireId, wire_b: WireId) {
        debug_assert!(wire_a != wire_b);

        let net_id_a = self.wire_net_map[wire_a.index()].expect("wire_a should exist");
        let net_id_b = self.wire_net_map[wire_b.index()].expect("wire_a should exist");

        if net_id_a == net_id_b {
            return;
        }

        let net_a = &self.nets[net_id_a.index()];
        let net_b = &self.nets[net_id_b.index()];

        match (net_a.island, net_b.island) {
            (Some(island_id_a), Some(island_id_b)) if island_id_a != island_id_b => {
                let (dst, dst_net_id, src, src_net_id) =
                    if self.islands[island_id_a.index()].nets.len()
                        >= self.islands[island_id_b.index()].nets.len()
                    {
                        (island_id_a, net_id_a, island_id_b, net_id_b)
                    } else {
                        (island_id_b, net_id_b, island_id_a, net_id_a)
                    };

                let dst_index = dst.index();
                let src_index = src.index();

                let mut src_island = self.islands.swap_remove(src_index);
                let last_index = self.islands.len();

                let new_dst = if dst_index == last_index { src } else { dst };

                {
                    let mut pos = None;

                    for &device in &src_island.devices {
                        self.device_island_map[device.index()] = Some(new_dst);
                    }

                    for (index, &net_id) in src_island.nets.iter().enumerate() {
                        if net_id == src_net_id {
                            pos = Some(index);
                        }
                        self.nets[net_id.index()].island = Some(new_dst);
                    }

                    let src_net_index = pos.unwrap_or_else(|| {
                        unreachable!("logical bug: src_net_id not in src_island.nets");
                    });
                    src_island.nets.swap_remove(src_net_index);
                }

                if dst_index != last_index
                    && let Some(swapped) = self.islands.get(src_index)
                {
                    for &net_id in &swapped.nets {
                        self.nets[net_id.index()].island = Some(src);
                    }

                    for &device in &swapped.devices {
                        self.device_island_map[device.index()] = Some(src);
                    }
                }

                let dst_island = &mut self.islands[new_dst.index()];
                dst_island.merge(src_island);

                let dst_net_index = dst_net_id.index();
                let src_net_index = src_net_id.index();

                let src_net = self.nets.swap_remove(src_net_index);
                let last_net_index = self.nets.len();

                let new_dst_net_id = if src_net_index == last_net_index {
                    src_net_index
                } else {
                    dst_net_index
                };

                if dst_net_index != last_net_index
                    && let Some(swapped) = self.nets.get(src_net_index)
                    && let Some(island_id) = swapped.island
                {
                    let old_swapped_net_id =
                        NetId::from(unsafe { NonZeroU32::new_unchecked(last_net_index as u32) });
                    self.islands[island_id.index()]
                        .nets
                        .iter_mut()
                        .filter(|net_id| old_swapped_net_id == **net_id)
                        .for_each(|net_id| *net_id = dst_net_id);

                    swapped
                        .wires
                        .iter()
                        .for_each(|&wire_id| self.wire_net_map[wire_id.index()] = Some(dst_net_id));
                }

                self.nets[new_dst_net_id].merge(src_net);
            }
            (Some(island_id_a), Some(_)) => {}
            (None, Some(island_id_b)) => {}
            (Some(island_id_a), None) => {}
            (None, None) => {}
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
struct IslandTopology {
    nets: Vec<NetId>,
    devices: Vec<DeviceId>,
}

impl IslandTopology {
    #[inline]
    pub fn add_device(&mut self, device_id: DeviceId) -> usize {
        assert!(!self.devices.contains(&device_id));
        let index = self.devices.len();
        self.devices.push(device_id);
        index
    }

    #[inline]
    pub fn remove_device(&mut self, index: usize) -> Option<DeviceId> {
        self.devices.swap_remove(index);
        self.devices.get(index).copied()
    }

    #[inline]
    pub fn add_net(&mut self, device_id: NetId) -> usize {
        assert!(!self.nets.contains(&device_id));
        let index = self.nets.len();
        self.nets.push(device_id);
        index
    }

    #[inline]
    pub fn remove_net(&mut self, net_id: NetId) -> Option<NetId> {
        let index = self.nets.iter().position(|&id| id == net_id)?;
        self.nets.swap_remove(index);
        self.nets.get(index).copied()
    }

    #[inline]
    pub fn nets(&self) -> &[NetId] {
        &self.nets
    }

    #[inline]
    pub fn devices(&self) -> &[DeviceId] {
        &self.devices
    }

    #[inline]
    pub fn merge(&mut self, other: IslandTopology) {
        self.nets.extend_from_slice(&other.nets);
        self.devices.extend_from_slice(&other.devices);
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
struct Net {
    island: Option<IslandId>,
    wires: Vec<WireId>,
    terminals: Vec<(DeviceId, TerminalId)>,
}

impl Net {
    #[inline]
    pub fn add_wire(&mut self, wire: WireId) -> usize {
        let index = self.wires.len();
        self.wires.push(wire);
        index
    }

    #[inline]
    pub fn remove_wire(&mut self, index: usize) -> Option<WireId> {
        self.wires.swap_remove(index);
        self.wires.get(index).copied()
    }

    #[inline]
    pub fn add_terminal(&mut self, device: DeviceId, terminal: TerminalId) -> usize {
        let index = self.terminals.len();
        self.terminals.push((device, terminal));
        index
    }

    #[inline]
    pub fn remove_terminal(&mut self, index: usize) -> Option<(DeviceId, TerminalId)> {
        self.terminals.swap_remove(index);
        self.terminals.get(index).copied()
    }

    #[inline]
    pub fn island(&self) -> Option<IslandId> {
        self.island
    }

    #[inline]
    pub fn wires(&self) -> &[WireId] {
        &self.wires
    }

    #[inline]
    pub fn terminals(&self) -> &[(DeviceId, TerminalId)] {
        &self.terminals
    }

    #[inline]
    pub fn merge(&mut self, other: Net) {
        dbg!(self.island == other.island);
        self.wires.extend_from_slice(&other.wires);
        self.terminals.extend_from_slice(&other.terminals);
    }
}
