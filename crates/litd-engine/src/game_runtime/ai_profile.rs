//! #1786: the lowered COMPUTER-OPPONENT PROFILE a packaged `.litdgame` carries so the
//! `litd-game` host can run an authored macro AI as a deterministic order-source.
//!
//! A [`LoweredAiProfile`] is pure DATA (editor-authored, lowered through the #1843
//! runtime-datasets valve, never hardcoded in Rust per #1312/#1783): it names the player
//! slot the AI drives, the worker/economy targets, the ordered build order, the army unit
//! and wave size, the retreat rule, and an optional authorized resource handicap. The
//! runtime executor (`session_ai.rs`) reads session state and emits the SAME typed
//! [`super::SessionOrder`]s a human would; it invents no game knowledge -- it executes
//! these numbers.
//!
//! Self-fingerprinted with the same idiom as [`super::datasets::LoweredRuntimeConstants`]:
//! a mutated field changes the canonical RON, changes this fingerprint, changes the
//! binding manifest fingerprint, and is rejected FAIL-LOUD at package load. Cross-dataset
//! reference checks (a build step or army unit that names a structure/unit absent from the
//! economy ruleset -- the "unsatisfiable build order / missing tech" case) are enforced in
//! [`super::datasets::validate_runtime_datasets`], which has the economy in hand.

use serde::{Deserialize, Serialize};

use crate::EngineError;

pub const LITD_AI_PROFILE_SCHEMA: &str = "litd.game.ai_profile.v1";

/// The permille denominator for the authorized handicap (1000 = no bonus).
pub const AI_HANDICAP_UNITY_PERMILLE: u16 = 1000;

fn profile_error<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("litd_game.package.ai_profile.{field}"),
        reason: reason.into(),
    })
}

/// One ordered build-order step: place `struct_id` in the economy once gold reaches
/// `at_gold` (and the cost is affordable). `cell_x/z` is the deterministic placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiBuildStep {
    pub struct_id: String,
    pub at_gold: u64,
    pub cell_x: i32,
    pub cell_z: i32,
}

/// How the AI selects the structure its attack wave strikes. Kept an enum so future
/// authored target rules extend without breaking the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiWaveTargetKind {
    /// Any fog-visible enemy structure (the AI must SEE it first -- no omniscient cheat).
    VisibleEnemyStructure,
}

/// One scheduled assault wave (#1858). Empty [`LoweredAiProfile::waves`] means legacy
/// single-wave behavior via `wave_size` alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiWaveStep {
    /// Absolute match tick at or after which this wave may mass and launch.
    pub at_tick: u64,
    /// Army bodies to have trained before this wave commits (>=1).
    pub size: u32,
    /// Ticks after WAVE_SENT before the next scheduled wave may mass (>=0).
    pub regroup_ticks: u32,
}

/// The lowered macro-AI profile. Every field is integer/string data -- no behaviour code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredAiProfile {
    pub schema: String,
    /// Dense player slot this profile drives (must exist in the match roster).
    pub controlled_player: u8,
    /// Display name for the FSV timeline / logs.
    pub display_name: String,
    /// Act every `decision_interval_ticks` ticks (>0): the AI's fixed decision cadence.
    pub decision_interval_ticks: u32,
    /// Resource node the AI sends harvesters to (must exist in the economy ruleset).
    pub worker_node_id: String,
    /// Desired harvester count the AI grows to before it stops adding workers.
    pub worker_target: u32,
    /// Ordered structures the AI places, gated by `at_gold` + affordability.
    pub build_order: Vec<AiBuildStep>,
    /// Unit the AI masses for its attack wave (must exist in the economy ruleset).
    pub army_unit_id: String,
    /// Legacy single-wave army size (>=1). Used when [`Self::waves`] is empty.
    pub wave_size: u32,
    /// How the wave picks its target.
    pub wave_target_kind: AiWaveTargetKind,
    /// #1858: ordered multi-wave schedule. Empty = one wave of `wave_size`.
    /// `serde(default)` keeps older packages without this field valid.
    #[serde(default)]
    pub waves: Vec<AiWaveStep>,
    /// If the AI's own alive structure count drops BELOW this, it retreats (0 = never).
    pub retreat_when_structures_below: u32,
    /// Ticks the AI holds its retreat/regroup before resuming the assault (>=1 when the
    /// retreat rule is armed).
    pub retreat_cooldown_ticks: u32,
    /// Authorized per-mille resource handicap applied to harvest deposits (1000 = none,
    /// >1000 = an authored bonus). The ONLY sanctioned "cheat"; default is unity.
    pub handicap_permille: u16,
    pub profile_fingerprint: String,
}

impl LoweredAiProfile {
    /// fnv1a64 hex over the canonical RON with the fingerprint field cleared -- the same
    /// idiom the economy ruleset + runtime constants use.
    pub fn compute_fingerprint(&self) -> Result<String, EngineError> {
        let mut preimage = self.clone();
        preimage.profile_fingerprint.clear();
        let ron = litd_hash::to_stable_ron(&preimage).map_err(|error| {
            EngineError::SerializationFailed {
                format: "ron",
                reason: format!("encode ai profile RON failed: {error}"),
            }
        })?;
        Ok(litd_hash::fnv1a64_hex(ron.as_bytes()))
    }

    /// Structural + fingerprint validation. Reference checks against the economy/match
    /// live in `validate_runtime_datasets`, which has those datasets in scope.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema != LITD_AI_PROFILE_SCHEMA {
            return profile_error(
                "schema",
                format!("expected {LITD_AI_PROFILE_SCHEMA}, got {}", self.schema),
            );
        }
        if self.decision_interval_ticks == 0 {
            return profile_error("decision_interval_ticks", "must be > 0");
        }
        if self.wave_size == 0 {
            return profile_error("wave_size", "must be >= 1");
        }
        if self.worker_node_id.trim().is_empty() {
            return profile_error("worker_node_id", "must be a non-empty node id");
        }
        if self.army_unit_id.trim().is_empty() {
            return profile_error("army_unit_id", "must be a non-empty unit id");
        }
        if self.retreat_when_structures_below > 0 && self.retreat_cooldown_ticks == 0 {
            return profile_error(
                "retreat_cooldown_ticks",
                "an armed retreat rule needs a non-zero cooldown",
            );
        }
        if self.handicap_permille == 0 {
            return profile_error("handicap_permille", "must be > 0 (1000 = no bonus)");
        }
        for (index, step) in self.build_order.iter().enumerate() {
            if step.struct_id.trim().is_empty() {
                return profile_error(
                    &format!("build_order.{index}.struct_id"),
                    "must be a non-empty structure id",
                );
            }
        }
        let mut prev_at = 0_u64;
        for (index, step) in self.waves.iter().enumerate() {
            if step.size == 0 {
                return profile_error(&format!("waves.{index}.size"), "must be >= 1");
            }
            if index > 0 && step.at_tick < prev_at {
                return profile_error(
                    &format!("waves.{index}.at_tick"),
                    format!(
                        "must be non-decreasing across the schedule (got {} < previous {prev_at})",
                        step.at_tick
                    ),
                );
            }
            prev_at = step.at_tick;
        }
        let expected = self.compute_fingerprint()?;
        if self.profile_fingerprint != expected {
            return Err(EngineError::SerializedFingerprintMismatch {
                expected,
                actual: self.profile_fingerprint.clone(),
            });
        }
        Ok(())
    }

    /// #1858: whether this profile uses the multi-wave schedule (vs legacy single wave).
    pub fn has_wave_schedule(&self) -> bool {
        !self.waves.is_empty()
    }

    /// Army size for wave index `cursor` (legacy uses `wave_size` when schedule empty).
    pub fn wave_size_at(&self, cursor: usize) -> u32 {
        if self.waves.is_empty() {
            self.wave_size
        } else {
            self.waves.get(cursor).map(|w| w.size).unwrap_or(0)
        }
    }

    /// Cumulative army bodies required through wave `cursor` inclusive.
    pub fn cumulative_army_target(&self, cursor: usize) -> u32 {
        if self.waves.is_empty() {
            return self.wave_size;
        }
        self.waves
            .iter()
            .take(cursor.saturating_add(1))
            .map(|w| w.size)
            .sum()
    }
}

/// Author + lower a legacy single-wave [`LoweredAiProfile`] (`waves` empty).
#[allow(clippy::too_many_arguments)]
pub fn lower_ai_profile(
    controlled_player: u8,
    display_name: impl Into<String>,
    decision_interval_ticks: u32,
    worker_node_id: impl Into<String>,
    worker_target: u32,
    build_order: Vec<AiBuildStep>,
    army_unit_id: impl Into<String>,
    wave_size: u32,
    retreat_when_structures_below: u32,
    retreat_cooldown_ticks: u32,
    handicap_permille: u16,
) -> Result<LoweredAiProfile, EngineError> {
    lower_ai_profile_with_waves(
        controlled_player,
        display_name,
        decision_interval_ticks,
        worker_node_id,
        worker_target,
        build_order,
        army_unit_id,
        wave_size,
        Vec::new(),
        retreat_when_structures_below,
        retreat_cooldown_ticks,
        handicap_permille,
    )
}

/// Author + lower a profile with an explicit multi-wave schedule (#1858).
/// Empty `waves` = legacy single wave of `wave_size`.
#[allow(clippy::too_many_arguments)]
pub fn lower_ai_profile_with_waves(
    controlled_player: u8,
    display_name: impl Into<String>,
    decision_interval_ticks: u32,
    worker_node_id: impl Into<String>,
    worker_target: u32,
    build_order: Vec<AiBuildStep>,
    army_unit_id: impl Into<String>,
    wave_size: u32,
    waves: Vec<AiWaveStep>,
    retreat_when_structures_below: u32,
    retreat_cooldown_ticks: u32,
    handicap_permille: u16,
) -> Result<LoweredAiProfile, EngineError> {
    let mut profile = LoweredAiProfile {
        schema: LITD_AI_PROFILE_SCHEMA.to_string(),
        controlled_player,
        display_name: display_name.into(),
        decision_interval_ticks,
        worker_node_id: worker_node_id.into(),
        worker_target,
        build_order,
        army_unit_id: army_unit_id.into(),
        wave_size,
        wave_target_kind: AiWaveTargetKind::VisibleEnemyStructure,
        waves,
        retreat_when_structures_below,
        retreat_cooldown_ticks,
        handicap_permille,
        profile_fingerprint: String::new(),
    };
    profile.profile_fingerprint = profile.compute_fingerprint()?;
    profile.validate()?;
    Ok(profile)
}
