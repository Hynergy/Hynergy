use super::connection::ConnectionRef;
use super::slot::WireSlot;
use super::{Network, NetworkModelError, WireId};

impl Network {
    pub fn add_wire(&mut self, id: WireId) -> Result<(), NetworkModelError> {
        let index = id.index();
        let len = self.wires.len();

        if index > len {
            return Err(NetworkModelError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        if index == len {
            self.wires.push(Some(WireSlot::new()));
            return Ok(());
        }

        let slot = &mut self.wires[index];
        if slot.is_some() {
            return Err(NetworkModelError::IdAlreadyAssigned { id: id.id() });
        }

        *slot = Some(WireSlot::new());
        Ok(())
    }

    pub fn remove_wire(&mut self, id: WireId) -> Result<(), NetworkModelError> {
        let index = id.index();
        let len = self.wires.len();

        if index >= len {
            return Err(NetworkModelError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        let slot = self.wires[index]
            .take()
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: super::ConnectionType::Wire,
                id: id.id(),
            })?;
        let removed_ref = ConnectionRef::from(id);

        for &connection in slot.connections() {
            self.unlink_one_way(connection, removed_ref);
        }

        Ok(())
    }

    pub fn connect_wires(
        &mut self,
        wire_a: WireId,
        wire_b: WireId,
    ) -> Result<(), NetworkModelError> {
        if wire_a == wire_b {
            return Err(NetworkModelError::WireConnectToSelf);
        }

        let a_index = wire_a.index();
        let b_index = wire_b.index();

        if a_index >= self.wires.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: super::ConnectionType::Wire,
                id: wire_a.id(),
            });
        }
        if b_index >= self.wires.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: super::ConnectionType::Wire,
                id: wire_b.id(),
            });
        }

        let [a_slot, b_slot] = unsafe { self.wires.get_disjoint_unchecked_mut([a_index, b_index]) };
        let a_slot = a_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: super::ConnectionType::Wire,
            id: wire_a.id(),
        })?;
        let b_slot = b_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: super::ConnectionType::Wire,
            id: wire_b.id(),
        })?;

        let a_to_b = ConnectionRef::from(wire_b);
        let b_to_a = ConnectionRef::from(wire_a);
        let a_connected = a_slot.contains_connection(a_to_b);
        let b_connected = b_slot.contains_connection(b_to_a);

        debug_assert_eq!(
            a_connected, b_connected,
            "bidirectional wire connection invariant violated"
        );
        if a_connected || b_connected {
            return Err(NetworkModelError::AlreadyConnected);
        }

        a_slot.add_connection(a_to_b);
        b_slot.add_connection(b_to_a);
        Ok(())
    }

    pub fn disconnect_wires(
        &mut self,
        wire_a: WireId,
        wire_b: WireId,
    ) -> Result<(), NetworkModelError> {
        if wire_a == wire_b {
            return Err(NetworkModelError::WireConnectToSelf);
        }

        let a_index = wire_a.index();
        let b_index = wire_b.index();

        if a_index >= self.wires.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: super::ConnectionType::Wire,
                id: wire_a.id(),
            });
        }
        if b_index >= self.wires.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: super::ConnectionType::Wire,
                id: wire_b.id(),
            });
        }

        let [a_slot, b_slot] = unsafe { self.wires.get_disjoint_unchecked_mut([a_index, b_index]) };
        let a_slot = a_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: super::ConnectionType::Wire,
            id: wire_a.id(),
        })?;
        let b_slot = b_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: super::ConnectionType::Wire,
            id: wire_b.id(),
        })?;

        let a_to_b = ConnectionRef::from(wire_b);
        let b_to_a = ConnectionRef::from(wire_a);
        let a_connected = a_slot.contains_connection(a_to_b);
        let b_connected = b_slot.contains_connection(b_to_a);

        debug_assert_eq!(
            a_connected, b_connected,
            "bidirectional wire connection invariant violated"
        );
        if !a_connected || !b_connected {
            return Err(NetworkModelError::NotConnected);
        }

        debug_assert!(a_slot.remove_connection(a_to_b));
        debug_assert!(b_slot.remove_connection(b_to_a));
        Ok(())
    }
}
