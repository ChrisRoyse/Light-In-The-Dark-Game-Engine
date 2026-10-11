//! #851 unit groups / player groups / selection + order-issuing surface.
//!
//! Deterministic containers: a [`UnitGroup`] holds a `Vec<UnitId>` kept sorted
//! and deduped, so membership iteration order is stable regardless of insertion
//! order (no `HashMap`, #85). Group ops add/remove and filter-by-predicate over
//! caller-supplied unit state. [`PlayerGroups`] and [`SelectionModel`] are dense
//! per-player `UnitGroup`s. The order surface types cover point / unit-target /
//! no-target orders with shift-queue semantics, validated against a unit's
//! lowered capability set before they are accepted.

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::{EngineError, math::FixedVec3};

/// A unit handle. Mirrors a `SimBodyId` index but is decoupled from the physics
/// sim so match logic can reference units the sim has already despawned.
pub type UnitId = u32;
/// An ability / order id (a unit capability, from lowered unit data).
pub type AbilityId = u16;

fn groups_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::SimInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

/// A deterministic, sorted, deduped set of units.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitGroup {
    members: Vec<UnitId>,
}

impl UnitGroup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from an arbitrary list; sorts + dedups so equal sets hash equal.
    pub fn from_units(units: impl IntoIterator<Item = UnitId>) -> Self {
        let mut members: Vec<UnitId> = units.into_iter().collect();
        members.sort_unstable();
        members.dedup();
        Self { members }
    }

    pub fn members(&self) -> &[UnitId] {
        &self.members
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn contains(&self, unit: UnitId) -> bool {
        self.members.binary_search(&unit).is_ok()
    }

    /// Add a unit; returns whether it was newly inserted. Keeps the vec sorted.
    pub fn add(&mut self, unit: UnitId) -> bool {
        match self.members.binary_search(&unit) {
            Ok(_) => false,
            Err(pos) => {
                self.members.insert(pos, unit);
                true
            }
        }
    }

    /// Remove a unit; returns whether it was present.
    pub fn remove(&mut self, unit: UnitId) -> bool {
        match self.members.binary_search(&unit) {
            Ok(pos) => {
                self.members.remove(pos);
                true
            }
            Err(_) => false,
        }
    }

    pub fn clear(&mut self) {
        self.members.clear();
    }

    /// Retain only units for which `keep` (a predicate over unit state the caller
    /// resolves) returns true. Deterministic: visits members in sorted order.
    pub fn retain(&mut self, mut keep: impl FnMut(UnitId) -> bool) {
        self.members.retain(|unit| keep(*unit));
    }

    /// A new group of the members for which `keep` returns true.
    pub fn filter(&self, mut keep: impl FnMut(UnitId) -> bool) -> Self {
        Self {
            members: self.members.iter().copied().filter(|u| keep(*u)).collect(),
        }
    }

    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.members.len() as u64);
        for unit in &self.members {
            hasher.update_u64_le(u64::from(*unit));
        }
    }
}

/// Dense per-player named-group storage (one persistent [`UnitGroup`] per player).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGroups {
    groups: Vec<UnitGroup>,
}

impl PlayerGroups {
    pub fn new(player_count: usize) -> Self {
        Self {
            groups: vec![UnitGroup::new(); player_count],
        }
    }

    pub fn group(&self, player: usize) -> Result<&UnitGroup, EngineError> {
        self.groups
            .get(player)
            .ok_or_else(|| groups_error("player_groups.player", format!("no player {player}")))
    }

    pub fn group_mut(&mut self, player: usize) -> Result<&mut UnitGroup, EngineError> {
        self.groups
            .get_mut(player)
            .ok_or_else(|| groups_error("player_groups.player", format!("no player {player}")))
    }

    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.groups.len() as u64);
        for group in &self.groups {
            group.fold_hash(hasher);
        }
    }
}

/// Per-player current selection (sim-adjacent state: it affects which orders a
/// player can issue, so it is folded into the deterministic match hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionModel {
    selected: Vec<UnitGroup>,
}

impl SelectionModel {
    pub fn new(player_count: usize) -> Self {
        Self {
            selected: vec![UnitGroup::new(); player_count],
        }
    }

    pub fn selection(&self, player: usize) -> Result<&UnitGroup, EngineError> {
        self.selected
            .get(player)
            .ok_or_else(|| groups_error("selection.player", format!("no player {player}")))
    }

    /// Replace a player's selection with a fresh set of units.
    pub fn select(
        &mut self,
        player: usize,
        units: impl IntoIterator<Item = UnitId>,
    ) -> Result<(), EngineError> {
        let slot = self
            .selected
            .get_mut(player)
            .ok_or_else(|| groups_error("selection.player", format!("no player {player}")))?;
        *slot = UnitGroup::from_units(units);
        Ok(())
    }

    /// Drop a unit from every player's selection (e.g. on death).
    pub fn remove_unit_everywhere(&mut self, unit: UnitId) {
        for group in &mut self.selected {
            group.remove(unit);
        }
    }

    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.selected.len() as u64);
        for group in &self.selected {
            group.fold_hash(hasher);
        }
    }
}

/// What an order points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderTarget {
    None,
    Point(FixedVec3),
    Unit(UnitId),
}

impl OrderTarget {
    const fn kind_code(self) -> u64 {
        match self {
            Self::None => 0,
            Self::Point(_) => 1,
            Self::Unit(_) => 2,
        }
    }
}

/// The target shape an ability requires, part of lowered unit capability data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderTargetKind {
    None,
    Point,
    Unit,
}

/// A single issued order. `queued` carries shift-queue semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitOrder {
    pub ability: AbilityId,
    pub target: OrderTarget,
    pub queued: bool,
}

impl UnitOrder {
    pub const fn point(ability: AbilityId, at: FixedVec3, queued: bool) -> Self {
        Self {
            ability,
            target: OrderTarget::Point(at),
            queued,
        }
    }

    pub const fn unit_target(ability: AbilityId, unit: UnitId, queued: bool) -> Self {
        Self {
            ability,
            target: OrderTarget::Unit(unit),
            queued,
        }
    }

    pub const fn no_target(ability: AbilityId, queued: bool) -> Self {
        Self {
            ability,
            target: OrderTarget::None,
            queued,
        }
    }

    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.ability));
        hasher.update_u64_le(self.target.kind_code());
        match self.target {
            OrderTarget::None => {}
            OrderTarget::Point(at) => {
                hasher.update_i64_le(at.x.raw());
                hasher.update_i64_le(at.y.raw());
                hasher.update_i64_le(at.z.raw());
            }
            OrderTarget::Unit(unit) => {
                hasher.update_u64_le(u64::from(unit));
            }
        }
        hasher.update_u8(u8::from(self.queued));
    }
}

/// One capability a unit possesses (from lowered unit data): an ability id and
/// the target shape it accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitCapability {
    pub ability: AbilityId,
    pub target_kind: OrderTargetKind,
}

/// A unit's lowered capability set, used to validate incoming orders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitCapabilities {
    caps: Vec<UnitCapability>,
}

impl UnitCapabilities {
    pub fn new(mut caps: Vec<UnitCapability>) -> Self {
        caps.sort_unstable_by_key(|c| c.ability);
        caps.dedup_by_key(|c| c.ability);
        Self { caps }
    }

    pub fn capabilities(&self) -> &[UnitCapability] {
        &self.caps
    }

    fn capability(&self, ability: AbilityId) -> Option<UnitCapability> {
        self.caps
            .binary_search_by_key(&ability, |c| c.ability)
            .ok()
            .map(|i| self.caps[i])
    }

    /// Validate an order against this unit's capabilities. Fails loud when the
    /// unit lacks the ability or the order's target shape does not match the
    /// ability's required target kind.
    pub fn validate(&self, order: &UnitOrder) -> Result<(), EngineError> {
        let cap = self.capability(order.ability).ok_or_else(|| {
            groups_error(
                "order.ability",
                format!("unit lacks ability {}", order.ability),
            )
        })?;
        let matches = matches!(
            (cap.target_kind, order.target),
            (OrderTargetKind::None, OrderTarget::None)
                | (OrderTargetKind::Point, OrderTarget::Point(_))
                | (OrderTargetKind::Unit, OrderTarget::Unit(_))
        );
        if !matches {
            return Err(groups_error(
                "order.target",
                format!(
                    "ability {} requires target kind {:?}, got a different target",
                    order.ability, cap.target_kind
                ),
            ));
        }
        Ok(())
    }
}
