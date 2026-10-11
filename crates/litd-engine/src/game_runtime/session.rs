//! #1843: the composed match-runtime SESSION the `litd-game` host instantiates from
//! packaged runtime datasets and drives with the deterministic tick.
//!
//! [`MatchRuntimeSession`] wires the self-contained Wave-4 systems together behind ONE
//! deterministic hash: the economy world ([`EconomyWorld`], #1785/#837 -- harvest, build,
//! train), the match-structure core ([`MatchState`], #804/#836/#838 -- players/alliances/
//! victory), sim-side fog of war ([`FogState`], #847), day/night ([`TimeOfDayState`],
//! #848), the HUD world ([`GameHudWorld`], #839-#843), and the campaign KV store (#850).
//! It is built ONLY from a validated [`RuntimeDatasetsManifest`] -- the same lowered data a
//! `.litdgame` carries -- never from in-Rust authoring builders (#1312/#1783).
//!
//! It binds the two documented Wave-4 seams to the real modules:
//! - fog <-> alliance shared-vision (#836): [`Self::recompute_fog`] derives a
//!   [`SharedVisionTable`] from the live [`PlayerRoster`] alliance `shared_vision` flags,
//!   so an alliance order changes who sees whose units.
//! - day/night <-> sight (#848): the per-unit fog sight radius is
//!   [`TimeOfDayState::sight_radius_input_mm`] converted to cells, so nightfall shrinks
//!   vision.
//!
//! Every mutation flows through [`SessionOrder`], and the applied order stream is retained,
//! so a save is a deterministic replay log ([`SessionSave`]): reload = rebuild from the
//! manifest and replay. Identical order streams reproduce an identical
//! [`Self::combined_state_hash`] across independent executions (#85 lockstep).

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    nav::{
        BakedNavMesh, NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput,
        bake_navmesh,
    },
};

use super::datasets::{
    LoweredRuntimeConstants, RuntimeDatasetsManifest, validate_runtime_datasets,
};
use super::error::GameRuntimeError;
use super::fog::{FogState, ModifierKind, SharedVisionTable, VisibilityModifier, VisionSource};
use super::hud::GameHudWorld;
use super::kv_store::CampaignKvStore;
use super::match_state::MatchState;
use super::time_of_day::TimeOfDayState;
use super::world::EconomyWorld;

#[path = "session_orders.rs"]
mod session_orders;
pub use session_orders::{SessionOrder, SessionStepReport};

#[path = "session_save.rs"]
mod session_save;
pub use session_save::{SESSION_SAVE_SCHEMA, SessionSave};

#[path = "session_replay.rs"]
mod session_replay;
pub use session_replay::{
    SESSION_REPLAY_ENCODING_BYTE_INDEX, SESSION_REPLAY_ENCODING_RON, SESSION_REPLAY_FORMAT_VERSION,
    SESSION_REPLAY_MAGIC, SESSION_REPLAY_VERSION_BYTE_INDEX, SessionReplay,
};

#[path = "session_transaction.rs"]
mod session_transaction;

#[path = "session_ai.rs"]
mod session_ai;
pub use session_ai::{AiDirector, AiTimelineEvent};

#[path = "session_triggers.rs"]
mod session_triggers;
pub use session_triggers::{TriggerFiring, TriggerRuntime};

// #1855: dispatch helpers for the runtime-effect orders (quests/timers/dialogs/
// floating text/victory-defeat) that need small typed construction.
#[path = "session_effects.rs"]
mod session_effects;

// #1857: dispatch for the fog reveal/conceal orders (session-held modifier list).
#[path = "session_fog_orders.rs"]
mod session_fog_orders;

// #1854/#1859/#1861: spawn-unit, cinematic SimGating, camera-follow intent.
#[path = "session_extra_orders.rs"]
mod session_extra_orders;
use session_extra_orders::CinematicGate;

/// A field unit that contributes vision to the fog map. Owner + integer cell; moving it
/// (via [`SessionOrder::MoveFieldUnit`]) is what makes fog update as units move. Seeded
/// from the match blueprint's non-structure units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldUnit {
    pub id: u32,
    pub owner: u32,
    pub cell_x: i32,
    pub cell_z: i32,
}

/// A read-only view of a field unit's live position (for the host/FSV to observe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldUnitView {
    pub id: u32,
    pub owner: u32,
    pub cell_x: i32,
    pub cell_z: i32,
}

/// The composed deterministic match runtime.
pub struct MatchRuntimeSession {
    source_manifest: RuntimeDatasetsManifest,
    datasets_fingerprint: String,
    constants: LoweredRuntimeConstants,
    economy: EconomyWorld,
    match_state: MatchState,
    fog: FogState,
    time_of_day: TimeOfDayState,
    hud: GameHudWorld,
    kv: CampaignKvStore,
    field_units: Vec<FieldUnit>,
    ai: AiDirector,
    triggers: TriggerRuntime,
    /// #1857: session-held fog reveal/conceal modifiers, kept sorted + unique so the
    /// list is canonical regardless of insertion order. Passed to every fog
    /// recompute and folded into `combined_state_hash`.
    fog_modifiers: Vec<VisibilityModifier>,
    /// #1854: active cinematic gate (sim-gating side only; presentation host owns
    /// the float `CinematicPlayer`). Folded into `combined_state_hash`.
    cinematic_gate: Option<CinematicGate>,
    /// #1859: next deterministic unit id for `SpawnFieldUnit` (starts past the
    /// highest blueprint-seeded field unit id).
    next_spawned_unit_id: u32,
    /// #1861: presentation camera-follow binding (NOT hashed -- presentation tier).
    camera_follow_unit: Option<u32>,
    /// #1856: lowered spatial region set (optional package section).
    regions: Option<super::regions::LoweredRegionSet>,
    order_log: Vec<SessionOrder>,
    applied_tick: u64,
}

impl MatchRuntimeSession {
    /// Instantiate the runtime from validated package datasets. Fails loud (typed) on any
    /// corrupt/mismatched dataset before a single system is constructed.
    pub fn from_manifest(manifest: &RuntimeDatasetsManifest) -> Result<Self, GameRuntimeError> {
        validate_runtime_datasets(manifest)?;
        let constants = manifest.constants.clone();
        let navmesh = bake_grid(constants.fog_grid_width, constants.fog_grid_height)?;
        let depot = NavCellCoord::new(0, 0);
        let build_origin = NavCellCoord::new(0, 0);
        let economy = EconomyWorld::new(
            manifest.economy.clone(),
            navmesh,
            depot,
            build_origin,
            constants.match_seed,
        )?;
        let match_state = MatchState::from_lowered_with_engagement(
            &manifest.lowered_match,
            constants.engagement_defaults(),
        )?;
        let player_count = match_state.roster().player_count();
        let player_ids: Vec<u32> = (0..player_count as u32).collect();
        let fog = FogState::new(
            constants.fog_grid_width,
            constants.fog_grid_height,
            &player_ids,
        )?;
        let time_of_day = TimeOfDayState::new(constants.time_of_day_config(), 0, 0)?;
        let hud = GameHudWorld::new();
        let kv = CampaignKvStore::new("campaign.session.1843", 1)?;
        // Seed field units (vision emitters) from the blueprint's mobile units.
        let blueprint = manifest.lowered_match.verify()?;
        let mut field_units = Vec::new();
        for authored in &blueprint.units {
            if !authored.is_structure {
                let (cx, cz) = start_cell(authored.owner, &constants);
                field_units.push(FieldUnit {
                    id: authored.unit,
                    owner: u32::from(authored.owner),
                    cell_x: cx,
                    cell_z: cz,
                });
            }
        }
        field_units.sort_by_key(|u| u.id);
        // #1786: compose the macro-AI order-source from the authored profiles + blueprint.
        let blueprint_units: Vec<(u32, u8, bool)> = blueprint
            .units
            .iter()
            .map(|u| (u.unit, u.owner, u.is_structure))
            .collect();
        // Force (team) per dense slot so the AI never targets allies (#836/#1788 2v2).
        let owner_forces: Vec<u8> = match_state
            .roster()
            .slots()
            .iter()
            .map(|slot| slot.force)
            .collect();
        let ai = AiDirector::new(
            &manifest.ai_profiles,
            &blueprint_units,
            &owner_forces,
            &manifest.economy,
            &constants,
        )?;
        // #1851/#1856: ECA trigger evaluator, seeded with the graph + region set.
        let triggers = TriggerRuntime::new(manifest.triggers.as_ref(), manifest.regions.as_ref());
        let next_spawned_unit_id = field_units
            .iter()
            .map(|u| u.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut session = Self {
            source_manifest: manifest.clone(),
            datasets_fingerprint: manifest.datasets_fingerprint.clone(),
            constants,
            economy,
            match_state,
            fog,
            time_of_day,
            hud,
            kv,
            field_units,
            ai,
            triggers,
            fog_modifiers: Vec::new(),
            cinematic_gate: None,
            next_spawned_unit_id,
            camera_follow_unit: None,
            regions: manifest.regions.clone(),
            order_log: Vec::new(),
            applied_tick: 0,
        };
        session.recompute_fog()?;
        Ok(session)
    }

    pub fn datasets_fingerprint(&self) -> &str {
        &self.datasets_fingerprint
    }

    pub fn economy(&self) -> &EconomyWorld {
        &self.economy
    }

    pub fn match_state(&self) -> &MatchState {
        &self.match_state
    }

    pub fn fog(&self) -> &FogState {
        &self.fog
    }

    pub fn time_of_day(&self) -> &TimeOfDayState {
        &self.time_of_day
    }

    pub fn hud(&self) -> &GameHudWorld {
        &self.hud
    }

    pub fn kv(&self) -> &CampaignKvStore {
        &self.kv
    }

    pub fn applied_tick(&self) -> u64 {
        self.applied_tick
    }

    /// Authored package tick rate used by presentation-only HUD animation.
    pub fn ticks_per_second(&self) -> u32 {
        self.constants.tick_rate
    }

    /// The macro-AI order-source (#1786): its executors' timeline of key decisions.
    pub fn ai(&self) -> &AiDirector {
        &self.ai
    }

    pub fn order_log(&self) -> &[SessionOrder] {
        &self.order_log
    }

    /// Live position of a seeded field unit, if present.
    pub fn field_unit(&self, id: u32) -> Option<FieldUnitView> {
        self.field_units
            .iter()
            .find(|u| u.id == id)
            .map(|u| FieldUnitView {
                id: u.id,
                owner: u.owner,
                cell_x: u.cell_x,
                cell_z: u.cell_z,
            })
    }

    /// #1856: the package region set, if present.
    pub fn regions(&self) -> Option<&super::regions::LoweredRegionSet> {
        self.regions.as_ref()
    }

    /// #1856: whether a field unit currently sits inside a region cell rect.
    pub fn unit_in_region(&self, unit: u32, region: u32) -> Result<bool, GameRuntimeError> {
        let Some(set) = self.regions.as_ref() else {
            return Err(GameRuntimeError::Invalid {
                field: "session.regions".to_string(),
                reason: "package embeds no region set (#1856)".to_string(),
            });
        };
        let region = set
            .region(region)
            .ok_or_else(|| GameRuntimeError::Invalid {
                field: "session.regions".to_string(),
                reason: format!("region_id {region} is absent from the package region set"),
            })?;
        let unit = self
            .field_unit(unit)
            .ok_or_else(|| GameRuntimeError::Invalid {
                field: "session.regions".to_string(),
                reason: format!("field unit {unit} is not live in the session"),
            })?;
        Ok(region.contains_cell(unit.cell_x, unit.cell_z))
    }

    /// The single deterministic value the replay + save/load acid tests assert on. Mixes
    /// every sub-system hash plus the field-unit positions, in fixed order.
    pub fn combined_state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update(self.datasets_fingerprint.as_bytes());
        hasher.update_u64_le(self.applied_tick);
        hasher.update_u64_le(self.economy.state_hash());
        hasher.update_u64_le(self.match_state.state_hash());
        hasher.update_u64_le(self.fog.state_hash());
        hasher.update_u64_le(self.time_of_day.state_hash());
        hasher.update_u64_le(self.hud.state_hash());
        hasher.update_u64_le(self.kv.state_hash());
        hasher.update_u64_le(self.field_units.len() as u64);
        for unit in &self.field_units {
            hasher.update_u64_le(u64::from(unit.id));
            hasher.update_u64_le(u64::from(unit.owner));
            hasher.update_u64_le(unit.cell_x as u64);
            hasher.update_u64_le(unit.cell_z as u64);
        }
        // #1786: fold the macro-AI executor cursors -- severing this is a determinism hole
        // the mutation-falsifiable test detects.
        self.ai.fold_hash(&mut hasher);
        // #1851: fold the trigger variable store / latches / fire counts -- same rule.
        self.triggers.fold_hash(&mut hasher);
        // #1857: fold the fog modifier list -- severing this hides a reveal/conceal
        // divergence the mutation-falsifiable test detects.
        hasher.update_u64_le(self.fog_modifiers.len() as u64);
        for modifier in &self.fog_modifiers {
            hasher.update_u64_le(u64::from(modifier.player));
            hasher.update_u64_le(fog_modifier_kind_code(modifier.kind));
            hasher.update_u64_le(modifier.min_x as u64);
            hasher.update_u64_le(modifier.min_y as u64);
            hasher.update_u64_le(modifier.max_x as u64);
            hasher.update_u64_le(modifier.max_y as u64);
        }
        // #1854: fold the cinematic gate -- severing this hides a SimGating
        // divergence the mutation-falsifiable test detects.
        hasher.update_u64_le(u64::from(self.cinematic_gate.is_some()));
        if let Some(gate) = &self.cinematic_gate {
            hasher.update_u64_le(gate.timeline_id.len() as u64);
            hasher.update(gate.timeline_id.as_bytes());
            hasher.update_u64_le(u64::from(gate.pauses_sim));
            hasher.update_u64_le(u64::from(gate.skipped));
        }
        hasher.update_u64_le(u64::from(self.next_spawned_unit_id));
        hasher.finish()
    }

    /// Rebuild every player's fog grid from the live field-unit positions. Binds BOTH
    /// Wave-4 seams: the per-unit sight radius comes from day/night (#848), and shared
    /// vision comes from the live roster alliance flags (#836).
    pub(super) fn recompute_fog(&mut self) -> Result<(), GameRuntimeError> {
        let cell_mm = self.constants.fog_cell_size_mm.max(1);
        let radius_cells = (self.time_of_day.sight_radius_input_mm() / cell_mm).max(1);
        let mut sources = Vec::with_capacity(self.field_units.len());
        for unit in &self.field_units {
            sources.push(VisionSource {
                owner: unit.owner,
                cell_x: unit.cell_x,
                cell_y: unit.cell_z,
                sight_radius_cells: radius_cells,
            });
        }
        let table = self.shared_vision_table()?;
        // #1857: the previously-hardcoded empty modifier list is now the session-held
        // reveal/conceal set (deterministic, deduped).
        self.fog.recompute(&sources, &self.fog_modifiers, &table);
        Ok(())
    }

    /// Snapshot the live roster's alliance `shared_vision` flags into the fog module's
    /// [`SharedVisionTable`] -- the concrete #836 <-> #847 binding.
    fn shared_vision_table(&self) -> Result<SharedVisionTable, GameRuntimeError> {
        let roster = self.match_state.roster();
        let count = roster.player_count();
        let mut table = SharedVisionTable::new();
        for a in 0..count {
            for b in (a + 1)..count {
                let cell = roster.alliance(a as u8, b as u8)?;
                if cell.shared_vision {
                    table.ally(a as u32, b as u32);
                }
            }
        }
        Ok(table)
    }
}

/// #1857: stable hash code for a fog modifier kind, folded into the session hash so a
/// dropped reveal/conceal is a determinism hole the mutation-falsifiable test detects.
fn fog_modifier_kind_code(kind: ModifierKind) -> u64 {
    match kind {
        ModifierKind::Reveal => 1,
        ModifierKind::Conceal => 2,
    }
}

/// Deterministic start cell for a player's seeded field units: player 0 near the depot
/// corner, others tucked toward the far corner, all clamped inside the grid.
pub(super) fn start_cell(owner: u8, constants: &LoweredRuntimeConstants) -> (i32, i32) {
    let w = constants.fog_grid_width as i32;
    let h = constants.fog_grid_height as i32;
    // Preserve 1v1 corners for slots 0/1 (Ashford Crossing FSV). Slots 2/3 fill the
    // remaining corners for 2v2 (Ridgefork Crossing / #1788).
    match owner {
        0 => (2.min(w - 1), 2.min(h - 1)),
        1 => {
            let back = 1i32;
            ((w - 3 - back).max(0), (h - 3).max(0))
        }
        2 => (2.min(w - 1), (h - 3).max(0)),
        3 => ((w - 3).max(0), 2.min(h - 1)),
        other => {
            let back = i32::from(other);
            ((w - 3 - back).max(0), (h - 3).max(0))
        }
    }
}

/// Bake a fully-buildable rectangular navmesh of the given cell dimensions. The economy
/// world places structures and paths workers on this grid.
fn bake_grid(width: u32, height: u32) -> Result<BakedNavMesh, EngineError> {
    let mut cells = Vec::with_capacity((width * height) as usize);
    for z in 0..height as i32 {
        for x in 0..width as i32 {
            cells.push(NavTerrainCellInput {
                id: format!("c_{x}_{z}"),
                x,
                z,
                height_fp: 0,
                flags: NavPathingFlags {
                    walkable: true,
                    buildable: true,
                    flyable: true,
                    blocked: false,
                },
            });
        }
    }
    bake_navmesh(&NavBakeInput::new(65536, cells, Vec::new()))
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;

// Shared builder + per-issue coverage for the #1855/#1857/#1854/#1859/#1861
// session-order cluster.
#[cfg(test)]
#[path = "session_order_test_support.rs"]
mod session_order_test_support;

#[cfg(test)]
#[path = "session_effects_tests.rs"]
mod session_effects_tests;

#[cfg(test)]
#[path = "session_extra_orders_tests.rs"]
mod session_extra_orders_tests;

#[cfg(test)]
#[path = "session_duration_tests.rs"]
mod session_duration_tests;

#[cfg(test)]
#[path = "session_timer_transaction_tests.rs"]
mod session_timer_transaction_tests;
