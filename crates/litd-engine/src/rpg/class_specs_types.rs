use serde::{Deserialize, Serialize};

use super::abilities_types::AbilityKind;
use super::attributes_types::AttributeScores;
use super::race_specs_types::RaceAccessRule;

/// Schema v2 (#1718). v2 extends v1 with four typed fields that v1 could only
/// record as prose in `implementation_notes`:
///   * `ClassSpec.fount` - the in-fiction power source (gap 7).
///   * `ClassSpec.alignment_exceptions` - per-race carve-outs on the class
///     alignment gate (gap 2).
///   * `ClassSpec.max_level` - the authoritative per-class advancement ceiling
///     (gap 6).
///   * `ClassLeveledAbility.specialization_ids` - a membership *set*, replacing
///     v1's single `specialization_id: Option<String>` (gap 4).
///   * `ClassLeveledAbility.acquisition` - the point-buy cost / prerequisite
///     economy (gap 1).
///
/// Migration is source-level, not on-disk: authored `ClassSpec` rows are
/// `#[cfg(test)]` Rust fixtures (#1630, no runtime loader / no persisted v1
/// corpus), so every fixture is rewritten to v2 in the same change and there is
/// deliberately NO `serde(default)` / permissive-default path - a v1 payload
/// missing the new fields fails to deserialize loudly rather than defaulting.
pub const RPG_CLASS_SPEC_SCHEMA: &str = "litd.rpg.class_spec.v3";
pub const WARRIOR_CLASS_ID: &str = "class.ashen_warrior";

/// A full class build spec: prime stat, alignment/ethos/race gates, leveled
/// abilities, specializations, legacies, gear access, and a Rust/wgpu visual
/// contract. Mirrors [`super::race_specs_types::RaceSpec`] so classes are
/// authorable through the same editor -> Calyx persist -> lower pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassSpec {
    pub schema: String,
    pub class_id: String,
    /// Authoritative class advancement ceiling. This is distinct from the world
    /// maximum and is never inferred from the last ability row.
    pub max_level: u16,
    pub display_name: String,
    pub player_facing_canon_name: String,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub source_history_label: String,
    pub concept_summary: String,
    pub prime_stat: PrimeStat,
    pub stat_minimums: AttributeScores,
    pub alignment: RaceAccessRule,
    pub ethos: RaceAccessRule,
    /// Canonical OR-of-AND eligibility policy. The legacy alignment and ethos
    /// fields remain as source readbacks, while admission decisions use this
    /// explicit policy so cross-dimensional alternatives cannot be flattened
    /// into a Cartesian product.
    pub eligibility: ClassEligibilityPolicy,
    /// Per-race carve-outs on the class-level `alignment` gate (schema v2, gap
    /// 2). Empty for classes whose alignment rule is uniform across races. Each
    /// entry overrides `alignment` for exactly one race id.
    pub alignment_exceptions: Vec<RaceAlignmentException>,
    /// The in-fiction power source the class draws on (schema v2, gap 7).
    pub fount: ClassFount,
    pub empowerment: bool,
    pub race_access: ClassRaceAccess,
    pub allowed_weapons: Vec<String>,
    pub allowed_armor: Vec<String>,
    pub leveled_abilities: Vec<ClassLeveledAbility>,
    pub specializations: Vec<ClassSpecialization>,
    pub specialization_choice_levels: Vec<u16>,
    pub legacies: Vec<ClassLegacy>,
    pub legacy_choice_levels: Vec<u16>,
    pub visual: ClassVisualContract,
    pub implementation_notes: Vec<String>,
}

/// A reusable eligibility expression. Each clause is an AND of the predicates
/// that are present; the clauses are ORed. `None` means that predicate is not
/// constrained by that clause, never "allow everything" by fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassEligibilityPolicy {
    pub clauses: Vec<ClassEligibilityClause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassEligibilityClause {
    /// Explicit unrestricted clause for a class whose authored gates are all
    /// open. This is not an omitted-policy fallback and cannot carry another
    /// predicate at the same time.
    pub match_all: bool,
    pub race_ids: Option<Vec<String>>,
    pub alignments: Option<Vec<String>>,
    pub ethoses: Option<Vec<String>>,
}

impl ClassEligibilityPolicy {
    /// Explicit source-level migration for legacy class rows. New authored
    /// rows should construct clauses directly when the rule is not a simple
    /// one-clause conjunction.
    pub fn from_legacy(alignment: &RaceAccessRule, ethos: &RaceAccessRule) -> Self {
        Self {
            clauses: vec![ClassEligibilityClause {
                match_all: matches!(alignment, RaceAccessRule::Any)
                    && matches!(ethos, RaceAccessRule::Any),
                race_ids: None,
                alignments: restricted_values(alignment),
                ethoses: restricted_values(ethos),
            }],
        }
    }
}

fn restricted_values(rule: &RaceAccessRule) -> Option<Vec<String>> {
    match rule {
        RaceAccessRule::Any => None,
        RaceAccessRule::Restricted { values } => Some(values.clone()),
    }
}

/// The primary attribute a class trains toward. Reuses the same six-attribute
/// axis as [`AttributeScores`] so authoring stays consistent with the stats
/// pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimeStat {
    Strength,
    Intelligence,
    Wisdom,
    Dexterity,
    Constitution,
    Charisma,
}

impl PrimeStat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Strength => "strength",
            Self::Intelligence => "intelligence",
            Self::Wisdom => "wisdom",
            Self::Dexterity => "dexterity",
            Self::Constitution => "constitution",
            Self::Charisma => "charisma",
        }
    }
}

/// The inverse of [`super::race_specs_types::RaceClassAccess`]: which races may
/// take this class. `All` means no race gate; `Restricted` lists the race ids
/// that may pick the class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassRaceAccess {
    All,
    Restricted { race_ids: Vec<String> },
}

/// One ability the class learns at a given level. References an ability by id
/// (the ability mechanics live in the #961 ability framework / `AbilityCatalog`);
/// this row only says *when* the class gains access and, if the ability is
/// gated behind a specialization, *which* specialization unlocks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassLeveledAbility {
    pub level: u16,
    pub ability_id: String,
    pub display_name: String,
    pub kind: AbilityKind,
    /// The specialization(s) this ability belongs to (schema v2, gap 4). Empty
    /// means a general ability with no specialization gate. CF sometimes
    /// dual-tags one ability to two specializations (e.g. a Ranger skill shared
    /// by two terrains); v1's single `Option<String>` could only keep the first
    /// tag, so this is a validated, de-duplicated *set*. v1 migration maps
    /// `None -> []` and `Some(x) -> [x]`.
    pub specialization_ids: Vec<String>,
    /// The point-buy acquisition economy for this ability (schema v2, gap 1):
    /// its point cost, the cumulative cost along its prerequisite chain, and an
    /// optional prerequisite ability. `None` for abilities granted purely by
    /// level (CF publishes costs only for path/specialist skills).
    pub acquisition: Option<AbilityAcquisitionCost>,
}

/// The point-buy cost of acquiring a specialist ability (schema v2, gap 1).
/// Transcribed from CF's per-path skill tables (`Cost` / `Cumulative Cost` /
/// `Prerequisite` columns). `cumulative_cost` is the running total of `cost`
/// along the prerequisite chain and MUST equal `cost` when there is no
/// prerequisite, or `prerequisite.cumulative_cost + cost` when there is one -
/// the validator enforces this so a transcription slip fails loud.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityAcquisitionCost {
    pub cost: u16,
    pub cumulative_cost: u16,
    pub prerequisite_ability_id: Option<String>,
}

/// A per-race override of the class-level alignment gate (schema v2, gap 2).
/// CF's Healer, for example, is "good or neutral, except humans, dark-elves,
/// and saurians who may be evil" - a carve-out that v1's single class-wide
/// `alignment` rule could not express. `race_id` names the race the override
/// applies to; `allowed` is the alignment rule for members of that race.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceAlignmentException {
    pub race_id: String,
    pub allowed: RaceAccessRule,
}

/// The in-fiction power source a class draws on (schema v2, gap 7). A closed
/// enum: an unknown fount is unrepresentable rather than defaulted. `Unresolved`
/// is a first-class, honest value for classes whose fount is a documented open
/// operator/lore decision (e.g. the Conjurer and Necromancer capture passes
/// both flagged this) - it is NOT a silent fallback and carries the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassFount {
    /// No magical fount; the class draws purely on martial skill.
    Martial,
    /// Arcane magic drawn from the mortal Weave (invokers, transmuters).
    Invocation,
    /// Divine / spirit power granted through supplication (paladins, healers).
    Supplication,
    /// Power bargained from the Hollow through a pact.
    Pact,
    /// A documented, unresolved fount decision. `reason` records the open
    /// question verbatim so nothing is fabricated.
    Unresolved { reason: String },
}

impl ClassFount {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Martial => "martial",
            Self::Invocation => "invocation",
            Self::Supplication => "supplication",
            Self::Pact => "pact",
            Self::Unresolved { .. } => "unresolved",
        }
    }

    /// Whether the class draws on a magical fount (everything but `Martial`).
    /// `Unresolved` counts as magic-using: it is only ever used for classes
    /// whose magic exists but whose *source* is undecided.
    pub const fn is_magical(&self) -> bool {
        !matches!(self, Self::Martial)
    }
}

/// A specialization / path the class can choose (for Warriors: a weapon type).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassSpecialization {
    pub specialization_id: String,
    pub display_name: String,
    pub unlock_level: u16,
    pub description: String,
    /// Optional specialization-level OR-of-AND gate. Class eligibility is
    /// always checked first; this policy narrows the class admission set.
    pub eligibility: Option<ClassEligibilityPolicy>,
}

/// A legacy / capstone technique the class may adopt at a legacy choice level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassLegacy {
    pub legacy_id: String,
    pub display_name: String,
    pub description: String,
}

/// The Rust/wgpu visual contract for a class: the VFX theme and signature
/// ability effects the renderer must supply. Analogous to
/// [`super::race_specs_types::RaceVisualContract`] but VFX-oriented since a
/// class shares a race's body model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassVisualContract {
    pub renderer: String,
    pub vfx_theme_id: String,
    pub signature_vfx: Vec<String>,
    pub palette_notes: String,
    pub animation_notes: String,
}

/// Flattened, count-oriented view returned after validation. Mirrors
/// [`super::race_specs_types::RaceSpecReadback`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassSpecReadback {
    pub schema: String,
    pub class_id: String,
    pub max_level: u16,
    pub display_name: String,
    pub player_facing_canon_name: String,
    pub source_issue: u32,
    pub prime_stat: PrimeStat,
    pub stat_minimums: AttributeScores,
    pub alignment: RaceAccessRule,
    pub ethos: RaceAccessRule,
    pub alignment_exception_count: usize,
    pub fount: ClassFount,
    pub empowerment: bool,
    pub race_access: ClassRaceAccess,
    pub allowed_weapon_count: usize,
    pub allowed_armor_count: usize,
    pub leveled_ability_count: usize,
    pub specialist_ability_count: usize,
    /// Abilities tagged to more than one specialization (schema v2, gap 4).
    pub dual_tagged_ability_count: usize,
    /// Abilities carrying a point-buy acquisition cost (schema v2, gap 1).
    pub costed_ability_count: usize,
    pub specialization_count: usize,
    pub specialization_choice_count: usize,
    pub legacy_count: usize,
    pub legacy_choice_count: usize,
    pub visual_renderer: String,
    pub signature_vfx_count: usize,
    pub class_spec_sha256: String,
}
