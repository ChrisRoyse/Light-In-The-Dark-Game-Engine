//! #1843: the lowered runtime datasets a packaged `.litdgame` carries so the
//! `litd-game` host can instantiate the Wave-4 runtime systems from PACKAGE data.
//!
//! A [`RuntimeDatasetsManifest`] bundles the three fingerprinted lowered artifacts the
//! runtime consumes -- the economy ruleset ([`LoweredEconomyRuleset`], #1785/#837), the
//! match blueprint ([`LoweredMatch`], #804/#836/#838), and the runtime tuning constants
//! ([`LoweredRuntimeConstants`], lowered from #827 gameplay constants + #848 day/night).
//! It is embedded in the runtime package envelope (`game_package.rs`) exactly like the
//! #853 campaign manifest, and is validated FAIL-LOUD at package load: a corrupt or
//! sabotaged sub-dataset changes its own fingerprint, which changes the manifest
//! fingerprint, so [`validate_runtime_datasets`] rejects it with a typed error rather
//! than booting a tampered match.
//!
//! The datasets are LOWERED (produced by the editor object-data lowering valve from the
//! real authored records), never authored in Rust: the runtime executes the numbers and
//! hardcodes no rule (#1312/#1783).

use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::ai_profile::LoweredAiProfile;
use super::blueprint::LoweredMatch;
use super::lowered::{LoweredEconomyRuleset, validate_ruleset, verify_lowered_economy_ron};
use super::regions::LoweredRegionSet;
use super::time_of_day::TimeOfDayConfig;
use super::trigger_graph::{
    LoweredTriggerGraph, TriggerCondition, TriggerEvent, validate_trigger_graph_against_blueprint,
    validate_trigger_graph_against_economy,
};

pub const LITD_RUNTIME_DATASETS_SCHEMA: &str = "litd.game.runtime_datasets.v1";
pub const LITD_RUNTIME_CONSTANTS_SCHEMA: &str = "litd.game.runtime_constants.v1";

fn datasets_error<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("litd_game.package.runtime_datasets.{field}"),
        reason: reason.into(),
    })
}

/// Runtime tuning: day/night, fog, match seed, and #1860 engagement defaults.
/// Integer fields only; self-fingerprinted so a mutated field fails load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredRuntimeConstants {
    pub schema: String,
    /// Ticks per second (must match the economy ruleset).
    pub tick_rate: u32,
    /// Ticks in one full day/night cycle (>0).
    pub day_length_ticks: u64,
    /// Full-daylight per-unit sight radius, milli-metres.
    pub base_sight_radius_mm: u32,
    /// Fraction of base sight removed at midnight, per-mille (0..=1000).
    pub night_sight_penalty_permille: u16,
    /// World milli-metres per fog grid cell (>0).
    pub fog_cell_size_mm: u32,
    pub fog_grid_width: u32,
    pub fog_grid_height: u32,
    pub match_seed: u64,
    /// #1860 structure max HP (milli, >0).
    pub structure_max_hp_milli: u32,
    /// #1860 field unit max HP (milli, >0).
    pub field_unit_max_hp_milli: u32,
    /// #1860 strike damage (milli, >0).
    pub strike_damage_milli: u32,
    pub constants_fingerprint: String,
}

impl LoweredRuntimeConstants {
    /// The day/night sim configuration these constants describe (#848 seam input).
    pub fn time_of_day_config(&self) -> TimeOfDayConfig {
        TimeOfDayConfig {
            day_length_ticks: self.day_length_ticks,
            base_sight_radius_mm: self.base_sight_radius_mm,
            night_sight_penalty_permille: self.night_sight_penalty_permille,
        }
    }

    /// fnv1a64 over the canonical RON with the fingerprint field cleared -- the same
    /// idiom the economy ruleset uses. Any mutated field changes this value.
    pub fn compute_fingerprint(&self) -> Result<String, EngineError> {
        let mut preimage = self.clone();
        preimage.constants_fingerprint.clear();
        let ron = litd_hash::to_stable_ron(&preimage).map_err(|error| {
            EngineError::SerializationFailed {
                format: "ron",
                reason: format!("encode runtime constants RON failed: {error}"),
            }
        })?;
        Ok(litd_hash::fnv1a64_hex(ron.as_bytes()))
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema != LITD_RUNTIME_CONSTANTS_SCHEMA {
            return datasets_error(
                "constants.schema",
                format!(
                    "expected {LITD_RUNTIME_CONSTANTS_SCHEMA}, got {}",
                    self.schema
                ),
            );
        }
        if self.tick_rate == 0 {
            return datasets_error("constants.tick_rate", "tick_rate must be non-zero");
        }
        if self.day_length_ticks == 0 {
            return datasets_error("constants.day_length_ticks", "day_length_ticks must be > 0");
        }
        if self.night_sight_penalty_permille > 1000 {
            return datasets_error(
                "constants.night_sight_penalty_permille",
                "must be within 0..=1000",
            );
        }
        if self.fog_cell_size_mm == 0 {
            return datasets_error("constants.fog_cell_size_mm", "must be non-zero");
        }
        if self.fog_grid_width == 0 || self.fog_grid_height == 0 {
            return datasets_error("constants.fog_grid", "fog grid dims must be non-zero");
        }
        if u64::from(self.fog_grid_width) * u64::from(self.fog_grid_height) > u64::from(u32::MAX) {
            return datasets_error("constants.fog_grid", "fog grid cell count exceeds u32");
        }
        if self.structure_max_hp_milli == 0
            || self.field_unit_max_hp_milli == 0
            || self.strike_damage_milli == 0
        {
            return datasets_error(
                "constants.engagement",
                "structure_max_hp_milli, field_unit_max_hp_milli, and strike_damage_milli must each be > 0 (#1860)",
            );
        }
        let expected = self.compute_fingerprint()?;
        if self.constants_fingerprint != expected {
            return Err(EngineError::SerializedFingerprintMismatch {
                expected,
                actual: self.constants_fingerprint.clone(),
            });
        }
        Ok(())
    }

    pub fn engagement_defaults(&self) -> super::engagement::EngagementDefaults {
        super::engagement::EngagementDefaults {
            structure_max_hp_milli: self.structure_max_hp_milli,
            field_unit_max_hp_milli: self.field_unit_max_hp_milli,
        }
    }
}

/// Lower constants with default #1860 engagement (structure 3000 / field 1000 / strike 1000).
#[allow(clippy::too_many_arguments)]
pub fn lower_runtime_constants(
    tick_rate: u32,
    day_length_ticks: u64,
    base_sight_radius_mm: u32,
    night_sight_penalty_permille: u16,
    fog_cell_size_mm: u32,
    fog_grid_width: u32,
    fog_grid_height: u32,
    match_seed: u64,
) -> Result<LoweredRuntimeConstants, EngineError> {
    use super::engagement::{
        DEFAULT_FIELD_UNIT_MAX_HP_MILLI, DEFAULT_STRIKE_DAMAGE_MILLI,
        DEFAULT_STRUCTURE_MAX_HP_MILLI,
    };
    lower_runtime_constants_with_engagement(
        tick_rate,
        day_length_ticks,
        base_sight_radius_mm,
        night_sight_penalty_permille,
        fog_cell_size_mm,
        fog_grid_width,
        fog_grid_height,
        match_seed,
        DEFAULT_STRUCTURE_MAX_HP_MILLI,
        DEFAULT_FIELD_UNIT_MAX_HP_MILLI,
        DEFAULT_STRIKE_DAMAGE_MILLI,
    )
}

/// Lower constants with explicit engagement numbers (#1860). Zero HP/damage fails loud.
#[allow(clippy::too_many_arguments)]
pub fn lower_runtime_constants_with_engagement(
    tick_rate: u32,
    day_length_ticks: u64,
    base_sight_radius_mm: u32,
    night_sight_penalty_permille: u16,
    fog_cell_size_mm: u32,
    fog_grid_width: u32,
    fog_grid_height: u32,
    match_seed: u64,
    structure_max_hp_milli: u32,
    field_unit_max_hp_milli: u32,
    strike_damage_milli: u32,
) -> Result<LoweredRuntimeConstants, EngineError> {
    let mut constants = LoweredRuntimeConstants {
        schema: LITD_RUNTIME_CONSTANTS_SCHEMA.to_string(),
        tick_rate,
        day_length_ticks,
        base_sight_radius_mm,
        night_sight_penalty_permille,
        fog_cell_size_mm,
        fog_grid_width,
        fog_grid_height,
        match_seed,
        structure_max_hp_milli,
        field_unit_max_hp_milli,
        strike_damage_milli,
        constants_fingerprint: String::new(),
    };
    constants.constants_fingerprint = constants.compute_fingerprint()?;
    constants.validate()?;
    Ok(constants)
}

/// The full lowered runtime datasets embedded in a runtime package. All three
/// sub-artifacts are already fingerprinted; the manifest fingerprint binds them
/// together so swapping one dataset for a differently-fingerprinted one is detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDatasetsManifest {
    pub schema: String,
    pub economy: LoweredEconomyRuleset,
    pub lowered_match: LoweredMatch,
    pub constants: LoweredRuntimeConstants,
    /// #1786: editor-authored macro-AI opponent profiles. Additive (`serde(default)` so a
    /// pre-#1786 package with no profiles still deserializes); folded into the binding
    /// fingerprint so a tampered/substituted profile is rejected at load.
    #[serde(default)]
    pub ai_profiles: Vec<LoweredAiProfile>,
    /// #1851: the lowered ECA trigger graph the session evaluator executes. Additive
    /// (`serde(default)` so a pre-#1851 package still deserializes); folded into the
    /// binding fingerprint so a tampered/substituted trigger section is rejected at load.
    #[serde(default)]
    pub triggers: Option<LoweredTriggerGraph>,
    /// #1856: fingerprinted spatial region set (integer cell rects). Additive.
    #[serde(default)]
    pub regions: Option<LoweredRegionSet>,
    pub datasets_fingerprint: String,
}

impl RuntimeDatasetsManifest {
    /// Assemble a manifest from the three lowered artifacts (no AI profiles) and stamp the
    /// binding fingerprint. Each artifact must already carry its own valid fingerprint.
    pub fn assemble(
        economy: LoweredEconomyRuleset,
        lowered_match: LoweredMatch,
        constants: LoweredRuntimeConstants,
    ) -> Result<Self, EngineError> {
        Self::assemble_with_ai(economy, lowered_match, constants, Vec::new())
    }

    /// Assemble a manifest with #1786 macro-AI opponent profiles.
    pub fn assemble_with_ai(
        economy: LoweredEconomyRuleset,
        lowered_match: LoweredMatch,
        constants: LoweredRuntimeConstants,
        ai_profiles: Vec<LoweredAiProfile>,
    ) -> Result<Self, EngineError> {
        Self::assemble_with_triggers(economy, lowered_match, constants, ai_profiles, None)
    }

    /// Assemble a manifest carrying the #1851 lowered ECA trigger graph.
    pub fn assemble_with_triggers(
        economy: LoweredEconomyRuleset,
        lowered_match: LoweredMatch,
        constants: LoweredRuntimeConstants,
        ai_profiles: Vec<LoweredAiProfile>,
        triggers: Option<LoweredTriggerGraph>,
    ) -> Result<Self, EngineError> {
        Self::assemble_with_regions(
            economy,
            lowered_match,
            constants,
            ai_profiles,
            triggers,
            None,
        )
    }

    /// Assemble a manifest carrying triggers + #1856 spatial regions.
    pub fn assemble_with_regions(
        economy: LoweredEconomyRuleset,
        lowered_match: LoweredMatch,
        constants: LoweredRuntimeConstants,
        ai_profiles: Vec<LoweredAiProfile>,
        triggers: Option<LoweredTriggerGraph>,
        regions: Option<LoweredRegionSet>,
    ) -> Result<Self, EngineError> {
        let mut manifest = Self {
            schema: LITD_RUNTIME_DATASETS_SCHEMA.to_string(),
            economy,
            lowered_match,
            constants,
            ai_profiles,
            triggers,
            regions,
            datasets_fingerprint: String::new(),
        };
        manifest.datasets_fingerprint = manifest.compute_fingerprint();
        Ok(manifest)
    }

    /// fnv1a64 over the sub-fingerprints in fixed order. Deterministic and path-free; the
    /// single value the negative control recomputes.
    pub fn compute_fingerprint(&self) -> String {
        let mut hasher = litd_hash::Fnv1a64::new();
        hasher.update(self.schema.as_bytes());
        hasher.update(self.economy.lowering_fingerprint.as_bytes());
        hasher.update(self.lowered_match.fingerprint.as_bytes());
        hasher.update(self.constants.constants_fingerprint.as_bytes());
        hasher.update_u64_le(self.ai_profiles.len() as u64);
        for profile in &self.ai_profiles {
            hasher.update(profile.profile_fingerprint.as_bytes());
        }
        // #1851: fold the trigger dataset (presence tag + sub-fingerprint) so a
        // substituted/stripped trigger section changes the binding fingerprint.
        match &self.triggers {
            None => {
                hasher.update_u64_le(0);
            }
            Some(graph) => {
                hasher.update_u64_le(1);
                hasher.update(graph.graph_fingerprint.as_bytes());
            }
        }
        // #1856: fold the region set.
        match &self.regions {
            None => {
                hasher.update_u64_le(0);
            }
            Some(set) => {
                hasher.update_u64_le(1);
                hasher.update(set.region_set_fingerprint.as_bytes());
            }
        }
        format!("{:016x}", hasher.finish())
    }
}

/// Fail-loud validation of the embedded runtime datasets, run at package validation
/// time in BOTH the editor (before write) and `litd-game` (before play). Re-derives
/// every fingerprint from the canonical bytes, so a sabotaged cost/handicap/sight
/// value is rejected with a typed error, never silently executed.
pub fn validate_runtime_datasets(manifest: &RuntimeDatasetsManifest) -> Result<(), EngineError> {
    if manifest.schema != LITD_RUNTIME_DATASETS_SCHEMA {
        return datasets_error(
            "schema",
            format!(
                "expected {LITD_RUNTIME_DATASETS_SCHEMA}, got {}",
                manifest.schema
            ),
        );
    }
    // Economy: structural validity + fingerprint re-derivation (the #1785 negative
    // control -- a tampered field changes the canonical RON and thus the fingerprint).
    validate_ruleset(&manifest.economy)?;
    let economy_ron = manifest.economy.to_canonical_ron()?;
    verify_lowered_economy_ron(&economy_ron, &manifest.economy.lowering_fingerprint)?;
    // Match blueprint: schema + sha256 + canonical round-trip (the #804 verifier).
    manifest.lowered_match.verify()?;
    // Runtime constants: structural + fingerprint.
    manifest.constants.validate()?;
    // Cross-consistency: the runtime advances all sims at one tick rate.
    if manifest.constants.tick_rate != manifest.economy.tick_rate {
        return datasets_error(
            "tick_rate",
            format!(
                "runtime constants tick_rate {} does not match economy ruleset tick_rate {}",
                manifest.constants.tick_rate, manifest.economy.tick_rate
            ),
        );
    }
    // #1786 macro-AI profiles: structural + fingerprint + CROSS-DATASET reference checks.
    // The "unsatisfiable build order / missing tech" case fails LOUD here, never silently.
    for profile in &manifest.ai_profiles {
        validate_ai_profile_against_datasets(manifest, profile)?;
    }
    // #1856 spatial regions: structural + fingerprint.
    if let Some(regions) = &manifest.regions {
        regions.validate()?;
    }
    // #1851 ECA trigger graph: structural + fingerprint + CROSS-DATASET unit-reference
    // checks (a trigger naming a unit absent from the blueprint fails LOUD here).
    if let Some(graph) = &manifest.triggers {
        graph.validate()?;
        let blueprint = manifest.lowered_match.verify()?;
        validate_trigger_graph_against_blueprint(graph, &blueprint)?;
        validate_trigger_graph_against_economy(graph, &manifest.economy)?;
        // #1856: every region id a trigger names must exist in the region set.
        validate_trigger_regions(graph, manifest.regions.as_ref())?;
    }
    // Binding fingerprint: any swapped sub-dataset diverges here.
    let expected = manifest.compute_fingerprint();
    if manifest.datasets_fingerprint != expected {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected,
            actual: manifest.datasets_fingerprint.clone(),
        });
    }
    Ok(())
}

/// #1856: region ids referenced by trigger events/conditions must exist in the set.
fn validate_trigger_regions(
    graph: &LoweredTriggerGraph,
    regions: Option<&LoweredRegionSet>,
) -> Result<(), EngineError> {
    let mut needed: Vec<u32> = Vec::new();
    for trigger in &graph.triggers {
        for event in &trigger.events {
            match event {
                TriggerEvent::UnitEntersRegion { region }
                | TriggerEvent::UnitLeavesRegion { region } => needed.push(*region),
                _ => {}
            }
        }
        for condition in &trigger.conditions {
            if let TriggerCondition::UnitInRegion { region, .. } = condition {
                needed.push(*region);
            }
        }
    }
    if needed.is_empty() {
        return Ok(());
    }
    let Some(set) = regions else {
        return datasets_error(
            "regions",
            "triggers reference regions but the package embeds no region set (#1856)",
        );
    };
    for region_id in needed {
        if set.region(region_id).is_none() {
            return datasets_error(
                "regions",
                format!("trigger references region_id {region_id} absent from the region set"),
            );
        }
    }
    Ok(())
}

/// Validate one AI profile structurally, then cross-check every id it references against
/// the economy ruleset + match roster. A build step / army unit / worker node that names
/// something the datasets do not define is rejected typed (the missing-tech edge case).
fn validate_ai_profile_against_datasets(
    manifest: &RuntimeDatasetsManifest,
    profile: &LoweredAiProfile,
) -> Result<(), EngineError> {
    profile.validate()?;
    let blueprint = manifest.lowered_match.verify()?;
    let player_count = blueprint.slots.len();
    if usize::from(profile.controlled_player) >= player_count {
        return datasets_error(
            "ai_profile.controlled_player",
            format!(
                "controlled_player {} is out of range for {player_count} players",
                profile.controlled_player
            ),
        );
    }
    if manifest
        .economy
        .nodes
        .iter()
        .all(|node| node.node_id != profile.worker_node_id)
    {
        return datasets_error(
            "ai_profile.worker_node_id",
            format!(
                "worker node {} is absent from the economy",
                profile.worker_node_id
            ),
        );
    }
    if manifest.economy.unit(&profile.army_unit_id).is_none() {
        return datasets_error(
            "ai_profile.army_unit_id",
            format!(
                "army unit {} is absent from the economy (missing tech)",
                profile.army_unit_id
            ),
        );
    }
    for (index, step) in profile.build_order.iter().enumerate() {
        if manifest.economy.structure(&step.struct_id).is_none() {
            return datasets_error(
                &format!("ai_profile.build_order.{index}.struct_id"),
                format!(
                    "build step {index} names structure {} absent from the economy (missing tech)",
                    step.struct_id
                ),
            );
        }
    }
    Ok(())
}
