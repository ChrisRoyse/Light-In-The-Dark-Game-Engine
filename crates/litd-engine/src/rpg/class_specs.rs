use std::collections::BTreeSet;

use litd_hash::sha256_hex;

use crate::EngineError;

use super::class_specs_primitives::{invalid, validate_access, validate_text, validate_token};
use super::class_specs_validate_v2::{
    validate_ability_acquisition_shape, validate_ability_specialization_ids,
    validate_acquisition_graph, validate_alignment_exceptions, validate_fount,
};
use super::{attributes_types::AttributeScores, class_specs_types::*};

pub fn class_spec_readback(spec: &ClassSpec) -> Result<ClassSpecReadback, EngineError> {
    validate_class_spec(spec)?;
    let specialist_ability_count = spec
        .leveled_abilities
        .iter()
        .filter(|ability| !ability.specialization_ids.is_empty())
        .count();
    let dual_tagged_ability_count = spec
        .leveled_abilities
        .iter()
        .filter(|ability| ability.specialization_ids.len() > 1)
        .count();
    let costed_ability_count = spec
        .leveled_abilities
        .iter()
        .filter(|ability| ability.acquisition.is_some())
        .count();
    Ok(ClassSpecReadback {
        schema: spec.schema.clone(),
        class_id: spec.class_id.clone(),
        max_level: spec.max_level,
        display_name: spec.display_name.clone(),
        player_facing_canon_name: spec.player_facing_canon_name.clone(),
        source_issue: spec.source_issue,
        prime_stat: spec.prime_stat,
        stat_minimums: spec.stat_minimums,
        alignment: spec.alignment.clone(),
        ethos: spec.ethos.clone(),
        alignment_exception_count: spec.alignment_exceptions.len(),
        fount: spec.fount.clone(),
        empowerment: spec.empowerment,
        race_access: spec.race_access.clone(),
        allowed_weapon_count: spec.allowed_weapons.len(),
        allowed_armor_count: spec.allowed_armor.len(),
        leveled_ability_count: spec.leveled_abilities.len(),
        specialist_ability_count,
        dual_tagged_ability_count,
        costed_ability_count,
        specialization_count: spec.specializations.len(),
        specialization_choice_count: spec.specialization_choice_levels.len(),
        legacy_count: spec.legacies.len(),
        legacy_choice_count: spec.legacy_choice_levels.len(),
        visual_renderer: spec.visual.renderer.clone(),
        signature_vfx_count: spec.visual.signature_vfx.len(),
        class_spec_sha256: spec_hash(spec)?,
    })
}

pub fn validate_class_spec(spec: &ClassSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_CLASS_SPEC_SCHEMA {
        return invalid("schema", format!("expected {RPG_CLASS_SPEC_SCHEMA}"));
    }
    validate_token("class_id", &spec.class_id)?;
    if spec.max_level == 0 || spec.max_level > 100 {
        return invalid("max_level", "must be inside 1..=100");
    }
    validate_text("display_name", &spec.display_name)?;
    validate_text("player_facing_canon_name", &spec.player_facing_canon_name)?;
    if spec.source_issue == 0 {
        return invalid("source_issue", "must be non-zero");
    }
    if spec.canon_issues.is_empty() || !spec.canon_issues.contains(&spec.source_issue) {
        return invalid("canon_issues", "must include the source issue");
    }
    if !spec
        .mechanics_source_url
        .starts_with("https://wiki.carrionfields.net/")
    {
        return invalid(
            "mechanics_source_url",
            "must point to the approved factual mechanics source",
        );
    }
    validate_text("source_history_label", &spec.source_history_label)?;
    validate_text("concept_summary", &spec.concept_summary)?;
    validate_stat_minimums(spec.stat_minimums)?;
    validate_access("alignment", &spec.alignment)?;
    validate_access("ethos", &spec.ethos)?;
    validate_eligibility_policy("eligibility", &spec.eligibility, None)?;
    validate_alignment_exceptions(&spec.alignment_exceptions, &spec.race_access)?;
    validate_fount(&spec.fount)?;
    validate_race_access(&spec.race_access)?;
    validate_token_list("allowed_weapons", &spec.allowed_weapons, true)?;
    validate_token_list("allowed_armor", &spec.allowed_armor, false)?;
    let specialization_ids = validate_specializations(&spec.specializations)?;
    validate_leveled_abilities(&spec.leveled_abilities, &specialization_ids)?;
    if spec
        .leveled_abilities
        .iter()
        .any(|ability| ability.level > spec.max_level)
    {
        return invalid(
            "leveled_abilities.level",
            "authored ability level exceeds the class max_level",
        );
    }
    validate_choice_levels(
        "specialization_choice_levels",
        &spec.specialization_choice_levels,
    )?;
    validate_legacies(&spec.legacies)?;
    validate_choice_levels("legacy_choice_levels", &spec.legacy_choice_levels)?;
    if spec.legacies.is_empty() != spec.legacy_choice_levels.is_empty() {
        return invalid(
            "legacy_choice_levels",
            "legacies and legacy choice levels must be present together",
        );
    }
    if spec.specializations.is_empty() != spec.specialization_choice_levels.is_empty() {
        return invalid(
            "specialization_choice_levels",
            "specializations and specialization choice levels must be present together",
        );
    }
    validate_visual(&spec.visual)?;
    if spec.implementation_notes.is_empty() {
        return invalid("implementation_notes", "must include source notes");
    }
    Ok(())
}

/// Cross-checks that every race id named by a restricted `race_access` gate
/// actually exists in the authored race roster. Kept separate from
/// [`validate_class_spec`] because the roster is external knowledge (the set of
/// authored [`super::race_specs_types::RaceSpec`] ids), not a property of the
/// class row itself.
pub fn validate_class_race_access(
    spec: &ClassSpec,
    known_race_ids: &BTreeSet<String>,
) -> Result<(), EngineError> {
    if let ClassRaceAccess::Restricted { race_ids } = &spec.race_access {
        for race_id in race_ids {
            if !known_race_ids.contains(race_id) {
                return invalid(
                    "race_access.race_ids",
                    format!("references unknown race {race_id}"),
                );
            }
        }
    }
    for exception in &spec.alignment_exceptions {
        if !known_race_ids.contains(&exception.race_id) {
            return invalid(
                "alignment_exceptions.race_id",
                format!("references unknown race {}", exception.race_id),
            );
        }
    }
    validate_eligibility_policy(
        "eligibility",
        &spec.eligibility,
        Some(known_race_ids),
    )?;
    for specialization in &spec.specializations {
        if let Some(policy) = &specialization.eligibility {
            validate_eligibility_policy(
                &format!(
                    "specializations.{}.eligibility",
                    specialization.specialization_id
                ),
                policy,
                Some(known_race_ids),
            )?;
        }
    }
    Ok(())
}

/// Validates the typed OR-of-AND policy without inventing a race roster. The
/// optional roster is supplied by the caller when authoring has that external
/// source available.
pub fn validate_eligibility_policy(
    field: &str,
    policy: &ClassEligibilityPolicy,
    known_race_ids: Option<&BTreeSet<String>>,
) -> Result<(), EngineError> {
    if policy.clauses.is_empty() {
        return invalid(field, "must contain at least one OR clause");
    }
    for (index, clause) in policy.clauses.iter().enumerate() {
        let clause_field = format!("{field}.clauses[{index}]");
        if clause.match_all
            && (clause.race_ids.is_some()
                || clause.alignments.is_some()
                || clause.ethoses.is_some())
        {
            return invalid(clause_field, "match_all cannot carry predicates");
        }
        if !clause.match_all
            && clause.race_ids.is_none()
            && clause.alignments.is_none()
            && clause.ethoses.is_none()
        {
            return invalid(clause_field, "empty clause is not permitted");
        }
        validate_policy_values(&format!("{clause_field}.race_ids"), &clause.race_ids)?;
        validate_policy_values(&format!("{clause_field}.alignments"), &clause.alignments)?;
        validate_policy_values(&format!("{clause_field}.ethoses"), &clause.ethoses)?;
        if let (Some(known), Some(races)) = (known_race_ids, &clause.race_ids)
            && !races.iter().any(|race| known.contains(race))
        {
            return invalid(
                format!("{clause_field}.race_ids"),
                "clause matches no authored race",
            );
        }
    }
    Ok(())
}

fn validate_policy_values(
    field: &str,
    values: &Option<Vec<String>>,
) -> Result<(), EngineError> {
    let Some(values) = values else { return Ok(()) };
    if values.is_empty() {
        return invalid(field, "a present predicate must contain values");
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_token(field, value)?;
        if !seen.insert(value) {
            return invalid(field, format!("duplicate {value}"));
        }
    }
    Ok(())
}

fn validate_stat_minimums(minimums: AttributeScores) -> Result<(), EngineError> {
    for (name, value) in minimums.pairs() {
        if !(0..=100).contains(&value) {
            return invalid(
                format!("stat_minimums.{name}"),
                "must be inside 0..=100 (0 means no requirement)",
            );
        }
    }
    Ok(())
}

fn validate_race_access(access: &ClassRaceAccess) -> Result<(), EngineError> {
    if let ClassRaceAccess::Restricted { race_ids } = access {
        if race_ids.is_empty() {
            return invalid("race_access", "restricted access requires race ids");
        }
        let mut seen = BTreeSet::new();
        for race_id in race_ids {
            validate_token("race_access.race_ids", race_id)?;
            if !seen.insert(race_id.as_str()) {
                return invalid("race_access.race_ids", format!("duplicate {race_id}"));
            }
        }
    }
    Ok(())
}

fn validate_token_list(
    field: &str,
    values: &[String],
    require_non_empty: bool,
) -> Result<(), EngineError> {
    if require_non_empty && values.is_empty() {
        return invalid(field, "at least one entry is required");
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_token(field, value)?;
        if !seen.insert(value.as_str()) {
            return invalid(field, format!("duplicate {value}"));
        }
    }
    Ok(())
}

fn validate_specializations(
    specializations: &[ClassSpecialization],
) -> Result<BTreeSet<String>, EngineError> {
    let mut ids = BTreeSet::new();
    for specialization in specializations {
        validate_token(
            "specializations.specialization_id",
            &specialization.specialization_id,
        )?;
        validate_text("specializations.display_name", &specialization.display_name)?;
        validate_level("specializations.unlock_level", specialization.unlock_level)?;
        validate_text("specializations.description", &specialization.description)?;
        if !ids.insert(specialization.specialization_id.clone()) {
            return invalid(
                "specializations.specialization_id",
                format!("duplicate {}", specialization.specialization_id),
            );
        }
    }
    Ok(ids)
}

fn validate_leveled_abilities(
    abilities: &[ClassLeveledAbility],
    specialization_ids: &BTreeSet<String>,
) -> Result<(), EngineError> {
    if abilities.is_empty() {
        return invalid("leveled_abilities", "at least one ability is required");
    }
    let mut seen = BTreeSet::new();
    for ability in abilities {
        validate_level("leveled_abilities.level", ability.level)?;
        validate_token("leveled_abilities.ability_id", &ability.ability_id)?;
        validate_text("leveled_abilities.display_name", &ability.display_name)?;
        if !seen.insert((ability.level, ability.ability_id.clone())) {
            return invalid(
                "leveled_abilities",
                format!(
                    "duplicate ability {} at level {}",
                    ability.ability_id, ability.level
                ),
            );
        }
        validate_ability_specialization_ids(&ability.specialization_ids, specialization_ids)?;
        validate_ability_acquisition_shape(ability)?;
    }
    validate_acquisition_graph(abilities)?;
    Ok(())
}

fn validate_choice_levels(field: &str, levels: &[u16]) -> Result<(), EngineError> {
    let mut previous = 0u16;
    for level in levels {
        validate_level(field, *level)?;
        if *level <= previous {
            return invalid(field, "choice levels must be strictly ascending");
        }
        previous = *level;
    }
    Ok(())
}

fn validate_legacies(legacies: &[ClassLegacy]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for legacy in legacies {
        validate_token("legacies.legacy_id", &legacy.legacy_id)?;
        validate_text("legacies.display_name", &legacy.display_name)?;
        validate_text("legacies.description", &legacy.description)?;
        if !seen.insert(legacy.legacy_id.clone()) {
            return invalid(
                "legacies.legacy_id",
                format!("duplicate {}", legacy.legacy_id),
            );
        }
    }
    Ok(())
}

fn validate_visual(visual: &ClassVisualContract) -> Result<(), EngineError> {
    if visual.renderer != "Rust/wgpu" {
        return invalid("visual.renderer", "must target Rust/wgpu");
    }
    validate_token("visual.vfx_theme_id", &visual.vfx_theme_id)?;
    if visual.signature_vfx.len() < 3 {
        return invalid("visual.signature_vfx", "must include the core VFX set");
    }
    let mut seen = BTreeSet::new();
    for effect in &visual.signature_vfx {
        validate_token("visual.signature_vfx", effect)?;
        if !seen.insert(effect.as_str()) {
            return invalid("visual.signature_vfx", format!("duplicate {effect}"));
        }
    }
    validate_text("visual.palette_notes", &visual.palette_notes)?;
    validate_text("visual.animation_notes", &visual.animation_notes)?;
    Ok(())
}

fn validate_level(field: impl Into<String>, level: u16) -> Result<(), EngineError> {
    if !(1..=100).contains(&level) {
        return invalid(field, "must be inside 1..=100");
    }
    Ok(())
}

fn spec_hash(spec: &ClassSpec) -> Result<String, EngineError> {
    // Calyx lowers the two large collections as key-sorted objects so field paths remain
    // deterministic. Hash the same canonical order; otherwise a valid row can fail integrity
    // verification merely because the authored vector used a different order.
    let mut canonical = spec.clone();
    canonical
        .leveled_abilities
        .sort_by(|left, right| (left.level, &left.ability_id).cmp(&(right.level, &right.ability_id)));
    canonical
        .legacies
        .sort_by(|left, right| left.legacy_id.cmp(&right.legacy_id));
    serde_json::to_vec(&canonical)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize class spec: {error}"),
        })
}
