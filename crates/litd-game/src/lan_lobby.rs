//! #1787: the host-authoritative lobby state machine.
//!
//! This is the deliverable's core alongside the protocol: a pure, testable state machine that
//! owns the 2..=8 slots, faction/slot selection, occupancy, and ready-up, and gates the
//! transition into the match (all occupied slots ready). It carries NO game-specific rules --
//! it only tracks who is in which slot with which faction and whether everyone is ready -- so
//! it is generic host machinery (#1783). The transport layer (lan_run) drives it in response to
//! wire messages; the machine itself never touches a socket.

use crate::lan_error::LanError;
use crate::lan_protocol::{LobbySlot, LobbySnapshot};

/// Minimum and maximum player slots a LAN match supports (M1 exercises 1v1 = two slots).
pub const MIN_SLOTS: u8 = 2;
pub const MAX_SLOTS: u8 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot {
    faction: u8,
    occupant: Option<Occupant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Occupant {
    name: String,
    is_host: bool,
    ready: bool,
}

/// The authoritative lobby. Slot 0 is always the host; joiners take the lowest free slot they
/// request (or the lowest free slot if their request is taken).
#[derive(Debug, Clone)]
pub struct LanLobby {
    slots: Vec<Slot>,
}

impl LanLobby {
    /// Create a lobby with `capacity` slots and seat the host in slot 0 with `faction`.
    pub fn new(capacity: u8, host_name: &str, host_faction: u8) -> Result<Self, LanError> {
        if !(MIN_SLOTS..=MAX_SLOTS).contains(&capacity) {
            return Err(LanError::Lobby {
                field: "capacity",
                reason: format!("capacity {capacity} must be within {MIN_SLOTS}..={MAX_SLOTS}"),
            });
        }
        let mut slots: Vec<Slot> = (0..capacity)
            .map(|index| Slot {
                faction: index,
                occupant: None,
            })
            .collect();
        slots[0].faction = host_faction;
        slots[0].occupant = Some(Occupant {
            name: host_name.to_string(),
            is_host: true,
            ready: false,
        });
        Ok(Self { slots })
    }

    pub fn host_slot(&self) -> u8 {
        0
    }

    pub fn capacity(&self) -> u8 {
        self.slots.len() as u8
    }

    /// Seat a joiner. Honours the requested slot if free, else assigns the lowest free slot.
    /// Fails typed if the lobby is full or the requested faction is out of range.
    pub fn seat_joiner(
        &mut self,
        name: &str,
        requested_slot: u8,
        faction: u8,
    ) -> Result<u8, LanError> {
        if faction >= self.capacity() {
            return Err(LanError::Lobby {
                field: "faction",
                reason: format!(
                    "faction {faction} out of range for {} slots",
                    self.capacity()
                ),
            });
        }
        let target = if self.is_free(requested_slot) {
            requested_slot
        } else {
            self.lowest_free().ok_or(LanError::Lobby {
                field: "slot",
                reason: "no free slot: lobby is full".to_string(),
            })?
        };
        let slot = &mut self.slots[target as usize];
        slot.faction = faction;
        slot.occupant = Some(Occupant {
            name: name.to_string(),
            is_host: false,
            ready: false,
        });
        Ok(target)
    }

    /// Mark a seated slot ready. Fails typed if the slot is empty.
    pub fn set_ready(&mut self, slot: u8) -> Result<(), LanError> {
        let entry = self
            .slots
            .get_mut(slot as usize)
            .and_then(|s| s.occupant.as_mut())
            .ok_or(LanError::Lobby {
                field: "ready",
                reason: format!("slot {slot} is not occupied"),
            })?;
        entry.ready = true;
        Ok(())
    }

    /// True once at least two slots are occupied and every occupied slot is ready -- the gate
    /// into the match.
    pub fn all_ready(&self) -> bool {
        let occupied = self.occupied_count();
        occupied >= usize::from(MIN_SLOTS)
            && self
                .slots
                .iter()
                .filter_map(|s| s.occupant.as_ref())
                .all(|o| o.ready)
    }

    pub fn occupied_count(&self) -> usize {
        self.slots.iter().filter(|s| s.occupant.is_some()).count()
    }

    /// The slots that are occupied, in slot order -- the participants the match runs with.
    #[cfg(test)]
    pub fn occupied_slots(&self) -> Vec<u8> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.occupant.is_some())
            .map(|(index, _)| index as u8)
            .collect()
    }

    /// The immutable snapshot broadcast to peers.
    pub fn snapshot(&self) -> LobbySnapshot {
        let slots = self
            .slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                let occupant = slot.occupant.as_ref();
                LobbySlot {
                    slot: index as u8,
                    faction: slot.faction,
                    occupied: occupant.is_some(),
                    is_host: occupant.map(|o| o.is_host).unwrap_or(false),
                    ready: occupant.map(|o| o.ready).unwrap_or(false),
                    player_name: occupant.map(|o| o.name.clone()).unwrap_or_default(),
                }
            })
            .collect();
        LobbySnapshot {
            slots,
            all_ready: self.all_ready(),
        }
    }

    fn is_free(&self, slot: u8) -> bool {
        self.slots
            .get(slot as usize)
            .map(|s| s.occupant.is_none())
            .unwrap_or(false)
    }

    fn lowest_free(&self) -> Option<u8> {
        self.slots
            .iter()
            .position(|s| s.occupant.is_none())
            .map(|index| index as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_seats_slot_zero_and_gate_needs_everyone_ready() {
        let mut lobby = LanLobby::new(2, "host", 0).unwrap();
        let joiner = lobby.seat_joiner("client", 1, 1).unwrap();
        assert_eq!(joiner, 1);
        assert!(!lobby.all_ready());
        lobby.set_ready(0).unwrap();
        assert!(!lobby.all_ready());
        lobby.set_ready(1).unwrap();
        assert!(lobby.all_ready());
        assert_eq!(lobby.occupied_slots(), vec![0, 1]);
    }

    #[test]
    fn requested_slot_collision_falls_back_to_lowest_free() {
        let mut lobby = LanLobby::new(4, "host", 0).unwrap();
        // Slot 0 is the host; requesting it must fall back to the next free slot.
        let seat = lobby.seat_joiner("client", 0, 2).unwrap();
        assert_eq!(seat, 1);
    }

    #[test]
    fn out_of_range_capacity_and_faction_are_typed() {
        assert_eq!(
            LanLobby::new(9, "h", 0).unwrap_err().code(),
            "LITD_LAN_LOBBY"
        );
        let mut lobby = LanLobby::new(2, "host", 0).unwrap();
        assert_eq!(
            lobby.seat_joiner("c", 1, 9).unwrap_err().code(),
            "LITD_LAN_LOBBY"
        );
    }

    #[test]
    fn full_lobby_refuses_further_joiners_typed() {
        let mut lobby = LanLobby::new(2, "host", 0).unwrap();
        lobby.seat_joiner("c1", 1, 1).unwrap();
        let error = lobby.seat_joiner("c2", 1, 1).unwrap_err();
        assert_eq!(error.code(), "LITD_LAN_LOBBY");
    }
}
