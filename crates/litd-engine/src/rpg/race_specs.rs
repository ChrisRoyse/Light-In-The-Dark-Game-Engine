use std::collections::BTreeSet;

use litd_hash::sha256_hex;

use crate::EngineError;

use super::{
    attributes_types::{AttributeScores, RaceAttributeCaps},
    race_specs_types::*,
};

pub fn race_attribute_caps(spec: &RaceSpec) -> RaceAttributeCaps {
    RaceAttributeCaps {
        race_id: spec.race_id.clone(),
        caps: spec.stat_caps,
    }
}

pub fn race_spec_readback(spec: &RaceSpec) -> Result<RaceSpecReadback, EngineError> {
    validate_race_spec(spec)?;
    Ok(RaceSpecReadback {
        schema: spec.schema.clone(),
        race_id: spec.race_id.clone(),
        display_name: spec.display_name.clone(),
        player_facing_canon_name: spec.player_facing_canon_name.clone(),
        source_issue: spec.source_issue,
        stat_caps: spec.stat_caps,
        alignment: spec.alignment.clone(),
        ethos: spec.ethos.clone(),
        size: spec.size,
        resistance_count: spec.resistances.len(),
        vulnerability_count: spec.vulnerabilities.len(),
        innate_ability_count: spec.innate_abilities.len(),
        infravision: spec.vision.infravision,
        class_access: spec.class_access.clone(),
        experience_penalty_percent: spec.experience_penalty_percent,
        hometown_count: spec.hometowns.len(),
        visual_renderer: spec.visual.renderer.clone(),
        animation_count: spec.visual.required_animations.len(),
        race_spec_sha256: spec_hash(spec)?,
    })
}

pub fn validate_race_spec(spec: &RaceSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_RACE_SPEC_SCHEMA {
        return invalid("schema", format!("expected {RPG_RACE_SPEC_SCHEMA}"));
    }
    validate_token("race_id", &spec.race_id)?;
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
    validate_text("culture_summary", &spec.culture_summary)?;
    validate_caps(spec.stat_caps)?;
    validate_access("alignment", &spec.alignment)?;
    validate_access("ethos", &spec.ethos)?;
    validate_profiles("resistances", &spec.resistances)?;
    validate_profiles("vulnerabilities", &spec.vulnerabilities)?;
    validate_innates(&spec.innate_abilities)?;
    validate_vision(&spec.vision)?;
    validate_class_access(&spec.class_access)?;
    if spec.experience_penalty_percent > 1000 {
        return invalid("experience_penalty_percent", "must be <= 1000");
    }
    if spec.hometowns.is_empty() {
        return invalid("hometowns", "at least one origin is required");
    }
    for hometown in &spec.hometowns {
        validate_token("hometown.origin_id", &hometown.origin_id)?;
        validate_text("hometown.display_name", &hometown.display_name)?;
        validate_text("hometown.canon_place", &hometown.canon_place)?;
    }
    validate_visual(&spec.visual)?;
    if spec.implementation_notes.is_empty() {
        return invalid("implementation_notes", "must include source notes");
    }
    Ok(())
}

fn validate_caps(caps: AttributeScores) -> Result<(), EngineError> {
    for (name, value) in caps.pairs() {
        if !(1..=100).contains(&value) {
            return invalid(format!("stat_caps.{name}"), "must be inside 1..=100");
        }
    }
    Ok(())
}

fn validate_access(field: &str, access: &RaceAccessRule) -> Result<(), EngineError> {
    if let RaceAccessRule::Restricted { values } = access {
        if values.is_empty() {
            return invalid(field, "restricted access requires values");
        }
        for value in values {
            validate_token(field, value)?;
        }
    }
    Ok(())
}

fn validate_profiles(field: &str, profiles: &[RaceResistanceProfile]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for profile in profiles {
        validate_token(field, &profile.resistance_id)?;
        // #1631: the allowed set is the decoupled RaceAffinity vocabulary (combat
        // channels + CF material/curse/meta affinities), NOT the nine combat
        // damage channels wired into the derived-stat table.
        if RaceAffinity::from_id(&profile.resistance_id).is_none() {
            return invalid(
                field,
                format!(
                    "unknown resistance {} (not in the race affinity vocabulary)",
                    profile.resistance_id
                ),
            );
        }
        if !seen.insert(profile.resistance_id.as_str()) {
            return invalid(
                field,
                format!("duplicate resistance {}", profile.resistance_id),
            );
        }
        if !(-100..=100).contains(&profile.amount) {
            return invalid(field, "amount must be inside -100..=100");
        }
    }
    Ok(())
}

fn validate_innates(abilities: &[RaceInnateAbility]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for ability in abilities {
        validate_token("innate_abilities.ability_id", &ability.ability_id)?;
        validate_text("innate_abilities.display_name", &ability.display_name)?;
        validate_text("innate_abilities.description", &ability.description)?;
        if !seen.insert(ability.ability_id.as_str()) {
            return invalid(
                "innate_abilities.ability_id",
                format!("duplicate {}", ability.ability_id),
            );
        }
    }
    Ok(())
}

fn validate_vision(vision: &RaceVision) -> Result<(), EngineError> {
    if !vision.infravision && vision.infravision_tiles != 0 {
        return invalid(
            "vision.infravision_tiles",
            "must be 0 when infravision is false",
        );
    }
    if vision.infravision && vision.infravision_tiles == 0 {
        return invalid(
            "vision.infravision_tiles",
            "must be non-zero when infravision is true",
        );
    }
    Ok(())
}

fn validate_class_access(access: &RaceClassAccess) -> Result<(), EngineError> {
    let class_ids = match access {
        RaceClassAccess::All => return Ok(()),
        RaceClassAccess::Restricted { class_ids } => {
            if class_ids.is_empty() {
                return invalid("class_access", "restricted access requires class ids");
            }
            class_ids
        }
        RaceClassAccess::AllExcept { class_ids } => {
            if class_ids.is_empty() {
                return invalid(
                    "class_access",
                    "all-except access requires excluded class ids",
                );
            }
            class_ids
        }
    };
    // NOTE (#1000 gap): only token FORM is checked here. There is no ClassSpec
    // registry yet (#1019-1037 unbuilt), so a well-formed but non-existent class id
    // is NOT rejected. Referential integrity must be added once the class roster lands.
    let mut seen = BTreeSet::new();
    for class_id in class_ids {
        validate_token("class_access.class_ids", class_id)?;
        if !seen.insert(class_id.as_str()) {
            return invalid(
                "class_access.class_ids",
                format!("duplicate class {class_id}"),
            );
        }
    }
    Ok(())
}

fn validate_visual(visual: &RaceVisualContract) -> Result<(), EngineError> {
    if visual.renderer != "Rust/wgpu" {
        return invalid("visual.renderer", "must target Rust/wgpu");
    }
    validate_token("visual.model_id", &visual.model_id)?;
    validate_token("visual.skeleton_id", &visual.skeleton_id)?;
    if visual.required_animations.len() < 6 {
        return invalid(
            "visual.required_animations",
            "must include core animation set",
        );
    }
    let mut seen = BTreeSet::new();
    for animation in &visual.required_animations {
        validate_token("visual.required_animations", animation)?;
        if !seen.insert(animation.as_str()) {
            return invalid(
                "visual.required_animations",
                format!("duplicate {animation}"),
            );
        }
    }
    validate_text("visual.silhouette", &visual.silhouette)?;
    validate_text("visual.material_notes", &visual.material_notes)?;
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    Ok(())
}

fn spec_hash(spec: &RaceSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize race spec: {error}"),
        })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
