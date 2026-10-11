//! Runtime `MatchState`: the deterministic match-structure core that ties the
//! player roster (#836), unit groups / selection / order surface (#851), and the
//! victory/defeat framework (#838) together behind one deterministic state hash.
//!
//! Built only from a verified [`LoweredMatch`] (never raw author state). All
//! mutable state lives in dense `Vec`s / sorted vecs (no `HashMap`), so the
//! [`MatchState::state_hash`] is reproducible across runs and machines — an
//! identical order stream yields an identical hash (#85 lockstep).

use litd_hash::Fnv1a64;

use crate::{
    EngineError,
    game_runtime::{
        blueprint::LoweredMatch,
        groups::{
            AbilityId, PlayerGroups, SelectionModel, UnitCapabilities, UnitCapability, UnitId,
            UnitOrder,
        },
        players::{AllianceState, PlayerId, PlayerRoster},
        victory::{GameOverEvent, PlayerResult, StructureLedger, VictoryState},
    },
};

fn match_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::SimInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

use super::engagement::{DamageResult, EngagementDefaults};

/// A resolved runtime unit: owner, structure flag, HP, lowered capabilities, and its
/// shift-queue order book. Kept in a vec sorted by `unit` for stable iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UnitRecord {
    unit: UnitId,
    owner: PlayerId,
    is_structure: bool,
    alive: bool,
    /// Current hit points in milli-units (#1860 engagement model).
    hp_milli: u32,
    /// Max hit points in milli-units (seeded from engagement defaults at materialize).
    max_hp_milli: u32,
    capabilities: UnitCapabilities,
    order_book: Vec<UnitOrder>,
}

/// The deterministic runtime match state.
#[derive(Debug, Clone)]
pub struct MatchState {
    map_id: String,
    tick: u64,
    roster: PlayerRoster,
    groups: PlayerGroups,
    selection: SelectionModel,
    victory: VictoryState,
    structures: StructureLedger,
    units: Vec<UnitRecord>,
    game_over_log: Vec<GameOverEvent>,
}

impl MatchState {
    /// Materialize a runtime match from a verified lowered artifact using default
    /// engagement HP. Prefer [`Self::from_lowered_with_engagement`] when package
    /// constants carry authored HP defaults (#1860).
    pub fn from_lowered(lowered: &LoweredMatch) -> Result<Self, EngineError> {
        Self::from_lowered_with_engagement(lowered, EngagementDefaults::default())
    }

    /// Materialize a runtime match from a verified lowered artifact. Re-verifies
    /// the artifact (fingerprint + canonical round-trip) before executing. Every
    /// unit is seeded with max HP from `engagement` (structure vs field).
    pub fn from_lowered_with_engagement(
        lowered: &LoweredMatch,
        engagement: EngagementDefaults,
    ) -> Result<Self, EngineError> {
        engagement.validate()?;
        let blueprint = lowered.verify()?;
        let mut roster = PlayerRoster::new(blueprint.slots.clone(), blueprint.force_count)?;
        // Apply authored alliance overrides. `author_alliance` range-checks the
        // pair and rejects a self-pair, so a sabotaged override fails loud here.
        for over in &blueprint.alliance_overrides {
            roster.author_alliance(over.from, over.to, over.cell)?;
        }
        let player_count = roster.player_count();
        let victory = VictoryState::new(
            &roster,
            blueprint.conditions.clone(),
            blueprint.observers_after_defeat,
        )?;
        let mut structures = StructureLedger::new(player_count);
        let mut structure_counts = vec![0_u32; player_count];
        let mut units = Vec::with_capacity(blueprint.units.len());
        for authored in &blueprint.units {
            if authored.is_structure {
                structure_counts[usize::from(authored.owner)] += 1;
            }
            let max_hp = engagement.max_hp_for(authored.is_structure);
            units.push(UnitRecord {
                unit: authored.unit,
                owner: authored.owner,
                is_structure: authored.is_structure,
                alive: true,
                hp_milli: max_hp,
                max_hp_milli: max_hp,
                capabilities: UnitCapabilities::new(authored.capabilities.clone()),
                order_book: Vec::new(),
            });
        }
        units.sort_by_key(|u| u.unit);
        for (player, count) in structure_counts.into_iter().enumerate() {
            structures.set(player as PlayerId, count)?;
        }
        Ok(Self {
            map_id: lowered.map_id.clone(),
            tick: 0,
            roster,
            groups: PlayerGroups::new(player_count),
            selection: SelectionModel::new(player_count),
            victory,
            structures,
            units,
            game_over_log: Vec::new(),
        })
    }

    pub fn map_id(&self) -> &str {
        &self.map_id
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn roster(&self) -> &PlayerRoster {
        &self.roster
    }

    pub fn selection(&self) -> &SelectionModel {
        &self.selection
    }

    pub fn player_groups(&self) -> &PlayerGroups {
        &self.groups
    }

    pub fn result(&self, player: PlayerId) -> Result<PlayerResult, EngineError> {
        self.victory.result(player)
    }

    pub fn is_observer(&self, player: PlayerId) -> Result<bool, EngineError> {
        self.victory.is_observer(player)
    }

    pub fn game_over_log(&self) -> &[GameOverEvent] {
        &self.game_over_log
    }

    /// Advance the match clock. Orders execute against the resulting tick.
    pub fn advance_tick(&mut self) -> Result<u64, EngineError> {
        self.tick = self
            .tick
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?;
        Ok(self.tick)
    }

    fn unit_index(&self, unit: UnitId) -> Result<usize, EngineError> {
        self.units
            .binary_search_by_key(&unit, |u| u.unit)
            .map_err(|_| match_error("match.unit", format!("no unit {unit}")))
    }

    /// The runtime alliance-change order (#836). Fails loud on a locked pair.
    pub fn set_alliance(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        state: AllianceState,
        shared_vision: bool,
        shared_control: bool,
    ) -> Result<(), EngineError> {
        self.roster
            .set_alliance_state(from, to, state, shared_vision, shared_control)?;
        Ok(())
    }

    /// Replace a player's selection (#851). Every unit must exist and be alive.
    pub fn select(&mut self, player: PlayerId, units: &[UnitId]) -> Result<(), EngineError> {
        for unit in units {
            let index = self.unit_index(*unit)?;
            if !self.units[index].alive {
                return Err(match_error("match.select", format!("unit {unit} is dead")));
            }
        }
        self.selection
            .select(usize::from(player), units.iter().copied())
    }

    /// Issue an order to a set of units (#851). Validates the order against each
    /// unit's lowered capabilities and that the issuing player owns the unit,
    /// then applies shift-queue semantics: a non-queued order clears the book.
    pub fn issue_order(
        &mut self,
        player: PlayerId,
        units: &[UnitId],
        order: UnitOrder,
    ) -> Result<usize, EngineError> {
        // Validate every unit up front so a rejected order mutates nothing.
        let mut indices = Vec::with_capacity(units.len());
        for unit in units {
            let index = self.unit_index(*unit)?;
            let record = &self.units[index];
            if record.owner != player {
                return Err(match_error(
                    "match.issue.owner",
                    format!("player {player} does not own unit {unit}"),
                ));
            }
            if !record.alive {
                return Err(match_error(
                    "match.issue.dead",
                    format!("unit {unit} is dead"),
                ));
            }
            record.capabilities.validate(&order)?;
            indices.push(index);
        }
        for index in &indices {
            let book = &mut self.units[*index].order_book;
            if !order.queued {
                book.clear();
            }
            book.push(order);
        }
        Ok(indices.len())
    }

    /// The pending order book of a unit (shift-queue contents).
    pub fn order_book(&self, unit: UnitId) -> Result<&[UnitOrder], EngineError> {
        let index = self.unit_index(unit)?;
        Ok(&self.units[index].order_book)
    }

    /// Fire a custom-condition trigger, then re-evaluate victory (#838).
    pub fn fire_trigger(&mut self, trigger: u32) -> Result<Vec<GameOverEvent>, EngineError> {
        self.victory.fire_trigger(trigger);
        self.evaluate()
    }

    /// #1855/#838: force a player to Defeat, then re-evaluate victory so a melee
    /// win can cascade to the surviving participant. Fails loud on an unknown
    /// player. The direct defeat event and any cascade events are logged + returned.
    pub fn defeat_player(&mut self, player: PlayerId) -> Result<Vec<GameOverEvent>, EngineError> {
        let mut events = self.victory.force_defeat(player, self.tick)?;
        self.game_over_log.extend(events.iter().cloned());
        events.extend(self.evaluate()?);
        Ok(events)
    }

    /// #1859: register a mid-match spawned field unit. Fails loud on a duplicate
    /// unit id or unknown owner. Capabilities come from the caller (session seeds
    /// a point-move ability so scripted staging units accept `IssueMove`).
    /// `max_hp_milli` must be > 0 (#1860 engagement).
    pub fn spawn_unit(
        &mut self,
        unit: UnitId,
        owner: PlayerId,
        capabilities: Vec<UnitCapability>,
        max_hp_milli: u32,
    ) -> Result<(), EngineError> {
        if usize::from(owner) >= self.roster.player_count() {
            return Err(match_error(
                "match.spawn.owner",
                format!(
                    "owner {owner} is out of range for {} players",
                    self.roster.player_count()
                ),
            ));
        }
        if max_hp_milli == 0 {
            return Err(match_error(
                "match.spawn.hp",
                format!("unit {unit} max_hp_milli must be > 0"),
            ));
        }
        if self.units.binary_search_by_key(&unit, |u| u.unit).is_ok() {
            return Err(match_error(
                "match.spawn.unit",
                format!("unit {unit} already exists"),
            ));
        }
        let record = UnitRecord {
            unit,
            owner,
            is_structure: false,
            alive: true,
            hp_milli: max_hp_milli,
            max_hp_milli,
            capabilities: UnitCapabilities::new(capabilities),
            order_book: Vec::new(),
        };
        let pos = self
            .units
            .binary_search_by_key(&unit, |u| u.unit)
            .expect_err("duplicate checked above");
        self.units.insert(pos, record);
        Ok(())
    }

    /// #1860: apply damage to a live unit. Amount must be > 0. When HP reaches 0
    /// the unit is destroyed through the same victory path as [`Self::destroy_unit`].
    /// Overkill is clamped (amount_applied may be less than requested). Fail-loud
    /// on missing/dead targets or zero damage — never a silent no-op.
    pub fn apply_damage(
        &mut self,
        target: UnitId,
        amount_milli: u32,
    ) -> Result<DamageResult, EngineError> {
        if amount_milli == 0 {
            return Err(match_error(
                "match.damage.amount",
                format!("damage against unit {target} must be > 0 (zero-damage strike is invalid)"),
            ));
        }
        let index = self.unit_index(target)?;
        if !self.units[index].alive {
            return Err(match_error(
                "match.damage.dead",
                format!("unit {target} is already destroyed"),
            ));
        }
        let hp_before = self.units[index].hp_milli;
        let applied = amount_milli.min(hp_before);
        let hp_after = hp_before - applied;
        self.units[index].hp_milli = hp_after;
        if hp_after > 0 {
            return Ok(DamageResult {
                target,
                amount_applied: applied,
                hp_before,
                hp_after,
                destroyed: false,
                game_over: Vec::new(),
            });
        }
        // HP depleted — destroy through the shared path (structure ledger + victory).
        let game_over = self.destroy_unit(target)?;
        Ok(DamageResult {
            target,
            amount_applied: applied,
            hp_before,
            hp_after: 0,
            destroyed: true,
            game_over,
        })
    }

    /// Current HP in milli-units (#1860). Fails loud if the unit id is unknown.
    pub fn hp_milli(&self, unit: UnitId) -> Result<u32, EngineError> {
        let index = self.unit_index(unit)?;
        Ok(self.units[index].hp_milli)
    }

    /// Max HP in milli-units (#1860). Fails loud if the unit id is unknown.
    pub fn max_hp_milli(&self, unit: UnitId) -> Result<u32, EngineError> {
        let index = self.unit_index(unit)?;
        Ok(self.units[index].max_hp_milli)
    }

    /// Owning player for a runtime unit. Used by session-level presentation effects
    /// that must resolve the same deterministic player start cell as combat AI.
    pub fn unit_owner(&self, unit: UnitId) -> Result<PlayerId, EngineError> {
        let index = self.unit_index(unit)?;
        Ok(self.units[index].owner)
    }

    /// Destroy a structure/unit: mark dead, zero HP, decrement its owner's structure
    /// count, purge it from selections/groups/order books, then re-evaluate
    /// victory (#838). This is what flips defeat/victory in melee. Scripted /
    /// trigger kills still use this path; combat reduction uses [`Self::apply_damage`].
    pub fn destroy_unit(&mut self, unit: UnitId) -> Result<Vec<GameOverEvent>, EngineError> {
        let index = self.unit_index(unit)?;
        if !self.units[index].alive {
            return Err(match_error(
                "match.destroy",
                format!("unit {unit} already destroyed"),
            ));
        }
        self.units[index].alive = false;
        self.units[index].hp_milli = 0;
        self.units[index].order_book.clear();
        let (owner, is_structure) = (self.units[index].owner, self.units[index].is_structure);
        if is_structure {
            self.structures.destroy_one(owner)?;
        }
        self.selection.remove_unit_everywhere(unit);
        for player in 0..self.roster.player_count() {
            self.groups.group_mut(player)?.remove(unit);
        }
        self.evaluate()
    }

    /// Re-evaluate victory/defeat and append any transitions to the game-over log.
    pub fn evaluate(&mut self) -> Result<Vec<GameOverEvent>, EngineError> {
        let events = self
            .victory
            .evaluate(self.tick, &self.roster, &self.structures)?;
        self.game_over_log.extend(events.iter().cloned());
        Ok(events)
    }

    /// Add a unit to a player's persistent named group (#851).
    pub fn add_to_group(&mut self, player: PlayerId, unit: UnitId) -> Result<bool, EngineError> {
        self.unit_index(unit)?;
        Ok(self.groups.group_mut(usize::from(player))?.add(unit))
    }

    /// A unit's lowered capability set (for host/AI order planning).
    pub fn capabilities(&self, unit: UnitId) -> Result<&[UnitCapability], EngineError> {
        let index = self.unit_index(unit)?;
        Ok(self.units[index].capabilities.capabilities())
    }

    /// Is a unit currently alive? Read surface for the runtime macro AI (#1786) to decide
    /// whether an enemy structure still needs striking / its own losses.
    pub fn is_alive(&self, unit: UnitId) -> Result<bool, EngineError> {
        let index = self.unit_index(unit)?;
        Ok(self.units[index].alive)
    }

    /// A player's current alive structure count (the retreat trigger + victory ledger,
    /// #1786 / #838). Reads the same ledger victory evaluates.
    pub fn structure_count(&self, player: PlayerId) -> Result<u32, EngineError> {
        self.structures.count(player)
    }

    pub fn has_ability(&self, unit: UnitId, ability: AbilityId) -> Result<bool, EngineError> {
        Ok(self
            .capabilities(unit)?
            .iter()
            .any(|c| c.ability == ability))
    }

    /// Alive unit ids owned by `player`, sorted (stable for ability auto-select #1894).
    pub fn alive_owned_units(&self, player: PlayerId) -> Vec<UnitId> {
        self.units
            .iter()
            .filter(|u| u.alive && u.owner == player)
            .map(|u| u.unit)
            .collect()
    }

    /// Deterministic digest of the entire match state. Folds tick, roster +
    /// alliance matrix, groups, selections, victory results/triggers, structure
    /// ledger, and every unit's alive flag + order book, all in stable order.
    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update_u64_le(self.tick);
        hasher.update(self.map_id.as_bytes());
        self.roster.fold_hash(&mut hasher);
        self.groups.fold_hash(&mut hasher);
        self.selection.fold_hash(&mut hasher);
        self.victory.fold_hash(&mut hasher);
        self.structures.fold_hash(&mut hasher);
        hasher.update_u64_le(self.units.len() as u64);
        for record in &self.units {
            hasher.update_u64_le(u64::from(record.unit));
            hasher.update_u8(record.owner);
            hasher.update_u8(u8::from(record.is_structure));
            hasher.update_u8(u8::from(record.alive));
            hasher.update_u64_le(u64::from(record.hp_milli));
            hasher.update_u64_le(u64::from(record.max_hp_milli));
            hasher.update_u64_le(record.order_book.len() as u64);
            for order in &record.order_book {
                order.fold_hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}
