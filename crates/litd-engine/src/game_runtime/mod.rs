//! Base-building & economy runtime (#1785 / #837).
//!
//! The Rust-era rebuild of the Go-era harvest/production sim (#300/#302), which was dropped
//! with g3n (#1071). Everything here is generic host machinery (#1312/#1783): resource costs,
//! harvest yields, upkeep tiers, and build times are all editor-authored data lowered into a
//! fingerprinted ruleset ([`lowered::LoweredEconomyRuleset`]) -- the engine executes the data
//! and hardcodes no game rule. All sim-affecting state is integer/fixed-point and folded into
//! a single deterministic [`world::EconomyWorld::state_hash`], so identical order streams
//! reproduce identical hashes for LAN lockstep (#85).
//!
//! This module also hosts the match-structure core (#804) and the save/load + day/night
//! runtime (#850 / #848):
//!
//! A [`GameRuntime`] composes the deterministic sim world (`FixedTickSim` +
//! `SimOrderQueue`, `sim.rs`/`sim_order.rs`) with three new deterministic pieces:
//!
//! - [`kv_store::CampaignKvStore`] -- the WC3 game-cache/hashtable equivalent: a
//!   typed key-value store scoped per-campaign and per-save-slot (#850a).
//! - [`time_of_day::TimeOfDayState`] -- sim-tick-driven day/night + weather that is
//!   itself SIM state and folds into the combined hash (#848).
//! - [`hero_carryover`] -- persist a hero's level/XP/items into the campaign store
//!   at chapter end and rehydrate in the next chapter (#853 runtime follow-up), plus
//!   #1853 permanent-death tombstones under `hero_roster`.
//!
//! [`save::GameSaveFile`] serializes the ENTIRE runtime (sim snapshot + both order
//! queues' pending positions + day/night state + KV store) to a fingerprinted save
//! file, validated on load: a corrupt or tampered save fails loud with a typed
//! [`error::GameRuntimeError`], never a silent fresh start.
//!
//! Determinism is load-bearing (#85): `combined_state_hash` mixes every sub-hash,
//! and replaying an identical order stream reproduces it exactly -- the save/load
//! acid test and the replay check in the FSV both assert on it.

use litd_hash::Fnv1a64;

use crate::{
    sim::{FixedTickSim, SimConfig},
    sim_order::{SimOrder, SimOrderQueue},
    sim_replay::SimSnapshot,
};

pub mod economy;
pub mod harvest;
pub mod lowered;
pub mod orders;
pub mod structures;
pub mod world;

// #1843: the lowered runtime datasets a packaged `.litdgame` carries + the composed
// match-runtime session the litd-game host instantiates from that package data.
// #1786: the editor-authored macro-AI opponent profile carried in those datasets.
pub mod ai_profile;
pub mod datasets;
pub mod session;
// #1851: the lowered ECA trigger graph those datasets carry + its session evaluator.
pub mod trigger_graph;
// #1856: spatial regions (integer cell rects) + enter/leave evaluation inputs.
pub mod regions;
// #1852: the single-player campaign flow runner (chapter advance + hero carry-over + save).
pub mod campaign_flow;
mod campaign_flow_topology;
// #1852: a real 2-chapter lowered campaign, shared by the flow unit tests + the FSV example.
#[cfg(any(test, feature = "fsv"))]
pub mod campaign_flow_fixture;

// Match-structure core (#804): players/alliances (#836), groups/orders (#851),
// victory/defeat (#838). Its order queue lives in `match_orders` to avoid a name
// collision with the economy `orders` module above.
pub mod blueprint;
pub mod engagement;
pub mod groups;
pub mod match_orders;
pub mod match_state;
pub mod players;
pub mod victory;

// Save/load persistence (#850) + runtime day/night & weather (#848).
pub mod error;
pub mod hero_carryover;
pub mod kv_store;
pub mod save;
pub mod time_of_day;

// WC3-parity runtime UI/HUD systems (#839-#843): quests, leaderboards/multiboards,
// timers, dialogs, floating text. Kept in its own `hud` module so its root-level
// `PlayerId`/`MAX_PLAYERS` (64-slot HUD mask) do not collide with the match-structure
// player model above.
pub mod hud;

// Runtime presentation & perception systems (#844-#847): sim-side deterministic fog,
// plus per-client camera / cinematic player / effects (presentation tier).
pub mod camera;
pub mod cinematic_player;
pub mod effects;
pub mod fog;

pub use economy::{PlayerAccounts, apply_upkeep_multiplier, upkeep_tier_for};
pub use harvest::{Worker, WorkerEvent, WorkerPhase};
pub use lowered::{
    ECONOMY_LOWERING_SCHEMA_VERSION, ECONOMY_LOWERING_VERSION, EconomyRulesetAuthor,
    LoweredEconomyRuleset, LoweredResourceKind, LoweredResourceNode, LoweredStructureType,
    LoweredUnitEconomy, LoweredUpkeepTier, validate_ruleset, verify_lowered_economy_ron,
};
pub use orders::{EconomyOrder, OrderOutcome};
pub use structures::{ProductionItem, RallyPoint, Structure, validate_footprint};
pub use world::{ECONOMY_MAX_UNITS, EconomyWorld, TickReport};

pub use ai_profile::{
    AI_HANDICAP_UNITY_PERMILLE, AiBuildStep, AiWaveStep, AiWaveTargetKind, LITD_AI_PROFILE_SCHEMA,
    LoweredAiProfile, lower_ai_profile, lower_ai_profile_with_waves,
};
pub use blueprint::{
    AuthoredAlliance, AuthoredUnit, LITD_LOWERED_MATCH_SCHEMA, LITD_MATCH_BLUEPRINT_SCHEMA,
    LoweredMatch, MatchBlueprint,
};
pub use datasets::{
    LITD_RUNTIME_CONSTANTS_SCHEMA, LITD_RUNTIME_DATASETS_SCHEMA, LoweredRuntimeConstants,
    RuntimeDatasetsManifest, lower_runtime_constants, lower_runtime_constants_with_engagement,
    validate_runtime_datasets,
};
pub use engagement::{
    DEFAULT_FIELD_UNIT_MAX_HP_MILLI, DEFAULT_STRIKE_DAMAGE_MILLI, DEFAULT_STRUCTURE_MAX_HP_MILLI,
    DamageResult, EngagementDefaults,
};
pub use groups::{
    AbilityId, OrderTarget, OrderTargetKind, PlayerGroups, SelectionModel, UnitCapabilities,
    UnitCapability, UnitGroup, UnitId, UnitOrder,
};
pub use match_orders::{MatchOrder, MatchOrderKind, MatchOrderQueue, MatchStepReport};
pub use match_state::MatchState;
pub use players::{
    AllianceCell, AllianceState, ForceId, MAX_PLAYERS, PlayerController, PlayerId, PlayerRoster,
    PlayerSlot,
};
pub use regions::{LITD_REGION_SET_SCHEMA, LoweredRegion, LoweredRegionSet, lower_region_set};
pub use session::{
    FieldUnitView, MatchRuntimeSession, SESSION_REPLAY_ENCODING_BYTE_INDEX,
    SESSION_REPLAY_ENCODING_RON, SESSION_REPLAY_FORMAT_VERSION, SESSION_REPLAY_MAGIC,
    SESSION_REPLAY_VERSION_BYTE_INDEX, SessionOrder, SessionReplay, SessionSave, SessionStepReport,
    TriggerFiring, TriggerRuntime,
};
pub use trigger_graph::{
    CompareOp, FlagOperand, IntOperand, LITD_TRIGGER_GRAPH_SCHEMA, LoweredTrigger,
    LoweredTriggerGraph, TriggerAction, TriggerCondition, TriggerEvent, TriggerValue,
    TriggerVarInit, lower_trigger_graph, validate_trigger_graph_against_blueprint,
};
pub use victory::{GameOverEvent, PlayerResult, StructureLedger, VictoryCondition, VictoryState};

pub use campaign_flow::{
    CAMPAIGN_PROGRESS_CATEGORY, CHAPTER_UNLOCK_CATEGORY, CampaignFlow, ChapterAdvance,
};
pub use error::GameRuntimeError;
pub use hero_carryover::{
    HERO_CARRYOVER_CATEGORY, HERO_ROSTER_CATEGORY, HERO_ROSTER_STATE_ACTIVE,
    HERO_ROSTER_STATE_FALLEN, HeroProgress, assert_revival_allowed, is_hero_fallen,
    persist_hero_carryover, rehydrate_hero_carryover, tombstone_hero,
};
pub use kv_store::{CAMPAIGN_KV_SCHEMA, CampaignKvStore, CampaignUnitSnapshot, CampaignValue};
pub use save::{GAME_SAVE_SCHEMA, GameSaveFile};
pub use time_of_day::{
    RuntimeEnvOrder, RuntimeEnvOrderKind, TIME_OF_DAY_SCHEMA, TimeOfDayConfig, TimeOfDayState,
    presentation_sight_radius_meters,
};

// Aggregate HUD types (the per-subsystem items stay under `hud::{boards,dialogs,..}`).
pub use hud::{GameHudWorld, HudColor, HudOrder, HudOrderKind, HudStepReport, PlayerMask};

/// One-step outcome of the composed runtime (analogous to `SimOrderStepReport`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameRuntimeStepReport {
    pub tick: u64,
    pub sim_orders_applied: usize,
    pub env_orders_applied: usize,
    pub combined_state_hash: u64,
    pub sight_radius_input_mm: u32,
    pub is_night: bool,
}

/// The full deterministic game runtime: sim world + order queue + day/night +
/// campaign KV store, hashed together.
pub struct GameRuntime<const MAX_BODIES: usize, const MAX_ORDERS: usize> {
    sim: FixedTickSim<MAX_BODIES>,
    queue: SimOrderQueue<MAX_ORDERS>,
    env_orders: Vec<RuntimeEnvOrder>,
    time_of_day: TimeOfDayState,
    kv: CampaignKvStore,
}

impl<const MAX_BODIES: usize, const MAX_ORDERS: usize> GameRuntime<MAX_BODIES, MAX_ORDERS> {
    pub fn new(
        config: SimConfig,
        seed: u64,
        time_of_day: TimeOfDayState,
        kv: CampaignKvStore,
    ) -> Result<Self, GameRuntimeError> {
        Ok(Self {
            sim: FixedTickSim::new(config, seed)?,
            queue: SimOrderQueue::default(),
            env_orders: Vec::new(),
            time_of_day,
            kv,
        })
    }

    pub fn sim(&self) -> &FixedTickSim<MAX_BODIES> {
        &self.sim
    }

    pub fn sim_mut(&mut self) -> &mut FixedTickSim<MAX_BODIES> {
        &mut self.sim
    }

    pub fn time_of_day(&self) -> &TimeOfDayState {
        &self.time_of_day
    }

    pub fn kv(&self) -> &CampaignKvStore {
        &self.kv
    }

    pub fn kv_mut(&mut self) -> &mut CampaignKvStore {
        &mut self.kv
    }

    pub fn pending_env_orders(&self) -> &[RuntimeEnvOrder] {
        &self.env_orders
    }

    /// Enqueue a deterministic sim order (movement/attack) into the sim queue.
    pub fn enqueue_order(&mut self, order: SimOrder) -> Result<u64, GameRuntimeError> {
        Ok(self.queue.enqueue(order)?)
    }

    /// Enqueue a trigger-set environment order (set time-of-day / set weather).
    pub fn enqueue_env_order(&mut self, order: RuntimeEnvOrder) -> Result<(), GameRuntimeError> {
        let execute_tick = self.sim.tick().saturating_add(1);
        if order.execute_tick < execute_tick {
            return error::invalid(
                "env_order.execute_tick",
                "environment order is already past due for the next tick",
            );
        }
        self.env_orders.push(order);
        Ok(())
    }

    /// The single deterministic value the acid test + replay check assert on.
    pub fn combined_state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update_u64_le(self.sim.state_hash());
        hasher.update_u64_le(self.time_of_day.state_hash());
        hasher.update_u64_le(self.kv.state_hash());
        hasher.finish()
    }

    /// Advance exactly one tick: apply due environment orders, then step the sim
    /// (which applies due sim orders), then advance day/night by one tick.
    pub fn step(&mut self) -> Result<GameRuntimeStepReport, GameRuntimeError> {
        let execute_tick = self.sim.tick().saturating_add(1);
        let env_orders_applied = self.apply_env_orders_for_tick(execute_tick)?;
        let sim_report = self.queue.step(&mut self.sim)?;
        self.time_of_day.advance_tick()?;
        Ok(GameRuntimeStepReport {
            tick: sim_report.tick,
            sim_orders_applied: sim_report.orders_applied,
            env_orders_applied,
            combined_state_hash: self.combined_state_hash(),
            sight_radius_input_mm: self.time_of_day.sight_radius_input_mm(),
            is_night: self.time_of_day.is_night(),
        })
    }

    /// Advance `ticks` ticks, returning the final combined state hash.
    pub fn run_ticks(&mut self, ticks: u64) -> Result<u64, GameRuntimeError> {
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(self.combined_state_hash())
    }

    fn apply_env_orders_for_tick(&mut self, execute_tick: u64) -> Result<usize, GameRuntimeError> {
        if self
            .env_orders
            .iter()
            .any(|o| o.execute_tick < execute_tick)
        {
            return error::invalid(
                "env_order.execute_tick",
                "an environment order elapsed without being applied (scheduler bug)",
            );
        }
        let mut due: Vec<RuntimeEnvOrder> = self
            .env_orders
            .iter()
            .copied()
            .filter(|o| o.execute_tick == execute_tick)
            .collect();
        due.sort_by_key(RuntimeEnvOrder::sort_key);
        for order in &due {
            self.time_of_day.apply(order.kind)?;
        }
        self.env_orders.retain(|o| o.execute_tick != execute_tick);
        Ok(due.len())
    }

    /// Capture the entire runtime as a fingerprinted save (in memory).
    pub fn capture_save(&self) -> Result<GameSaveFile, GameRuntimeError> {
        GameSaveFile::capture(
            SimSnapshot::capture(&self.sim),
            self.queue.pending_order_list(),
            self.env_orders.clone(),
            self.time_of_day.clone(),
            self.kv.clone(),
        )
    }

    /// Rebuild a runtime from a validated save, resuming at the saved tick with the
    /// saved queue position. Deterministic continuation is guaranteed because the
    /// sim snapshot re-verifies its own `state_hash` on restore.
    pub fn restore_from_save(save: &GameSaveFile) -> Result<Self, GameRuntimeError> {
        save.validate()?;
        let sim = save.sim_snapshot.restore::<MAX_BODIES>()?;
        let mut queue = SimOrderQueue::<MAX_ORDERS>::default();
        for order in &save.pending_sim_orders {
            queue.enqueue(*order)?;
        }
        Ok(Self {
            sim,
            queue,
            env_orders: save.pending_env_orders.clone(),
            time_of_day: save.time_of_day.clone(),
            kv: save.kv.clone(),
        })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "save_load_tests.rs"]
mod save_load_tests;

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod presentation_tests;
