use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::{
    ImmortalCatalog, ImmortalDefinition, ImmortalEvaluationInput, ImmortalRules, ImmortalSignal,
    ImmortalSignalKind, RPG_IMMORTAL_SCHEMA,
};

pub(super) struct ImmortalIndexes<'a> {
    pub immortals: BTreeMap<&'a str, &'a ImmortalDefinition>,
}

pub(super) fn validate_immortal_rules(rules: &ImmortalRules) -> Result<(), EngineError> {
    if rules.schema != RPG_IMMORTAL_SCHEMA {
        return invalid("rules.schema", format!("expected {RPG_IMMORTAL_SCHEMA}"));
    }
    if rules.min_signal_strength_milli == 0 || rules.min_signal_strength_milli > 1000 {
        return invalid("rules.min_signal_strength_milli", "must be inside 1..=1000");
    }
    if rules.min_rp_quality_milli > 1000 {
        return invalid("rules.min_rp_quality_milli", "must be inside 0..=1000");
    }
    if rules.max_empowerment_tier == 0 || rules.max_empowerment_tier > 100 {
        return invalid("rules.max_empowerment_tier", "must be inside 1..=100");
    }
    Ok(())
}

pub(super) fn validate_immortal_input(
    input: &ImmortalEvaluationInput,
) -> Result<ImmortalIndexes<'_>, EngineError> {
    let indexes = validate_immortal_catalog(&input.catalog)?;
    validate_immortal_signal(&input.signal)?;
    let immortal = indexes
        .immortals
        .get(input.signal.immortal_id.as_str())
        .ok_or_else(|| invalid_error("signal.immortal_id", "unknown immortal"))?;
    if !immortal
        .allowed_actions
        .contains(&input.signal.requested_action)
    {
        return invalid(
            "signal.requested_action",
            "immortal is not allowed to perform requested action",
        );
    }
    for tag in &input.signal.tags {
        if !immortal.ward_tags.contains(tag) {
            return invalid(
                "signal.tags",
                format!("signal tag {tag} is outside immortal Ward envelope"),
            );
        }
    }
    if input.signal.kind == ImmortalSignalKind::PossessionRequest
        && (input.signal.target_body_id.is_none() || input.signal.possession_velocity.is_none())
    {
        return invalid(
            "signal.possession",
            "possession requests require body id and deterministic velocity",
        );
    }
    Ok(indexes)
}

pub(super) fn validate_immortal_catalog(
    catalog: &ImmortalCatalog,
) -> Result<ImmortalIndexes<'_>, EngineError> {
    if catalog.schema != RPG_IMMORTAL_SCHEMA {
        return invalid("catalog.schema", format!("expected {RPG_IMMORTAL_SCHEMA}"));
    }
    validate_token("catalog.catalog_id", &catalog.catalog_id)?;
    if catalog.immortals.is_empty() {
        return invalid("catalog.immortals", "at least one immortal is required");
    }
    let mut immortals = BTreeMap::new();
    for immortal in &catalog.immortals {
        validate_immortal(immortal)?;
        if immortals
            .insert(immortal.immortal_id.as_str(), immortal)
            .is_some()
        {
            return invalid("immortals.immortal_id", "duplicate immortal");
        }
    }
    Ok(ImmortalIndexes { immortals })
}

fn validate_immortal(immortal: &ImmortalDefinition) -> Result<(), EngineError> {
    validate_token("immortals.immortal_id", &immortal.immortal_id)?;
    validate_display("immortals.display_name", &immortal.display_name)?;
    validate_token("immortals.religion_id", &immortal.religion_id)?;
    if immortal.allowed_actions.is_empty() {
        return invalid(
            "immortals.allowed_actions",
            "at least one allowed action is required",
        );
    }
    let mut actions = BTreeSet::new();
    for action in &immortal.allowed_actions {
        if !actions.insert(*action) {
            return invalid("immortals.allowed_actions", "duplicate action");
        }
    }
    validate_tag_rows("immortals.ward_tags", &immortal.ward_tags)?;
    if immortal.lowered_agent_id == 0 {
        return invalid("immortals.lowered_agent_id", "must be non-zero");
    }
    if immortal.max_empowerment_tier == 0 || immortal.max_empowerment_tier > 100 {
        return invalid("immortals.max_empowerment_tier", "must be inside 1..=100");
    }
    Ok(())
}

fn validate_immortal_signal(signal: &ImmortalSignal) -> Result<(), EngineError> {
    validate_token("signal.signal_id", &signal.signal_id)?;
    validate_token("signal.immortal_id", &signal.immortal_id)?;
    validate_token("signal.source_cx_id", &signal.source_cx_id)?;
    validate_token("signal.target_character_id", &signal.target_character_id)?;
    if signal.strength_milli == 0 || signal.strength_milli > 1000 {
        return invalid("signal.strength_milli", "must be inside 1..=1000");
    }
    if signal.rp_quality_milli > 1000 {
        return invalid("signal.rp_quality_milli", "must be inside 0..=1000");
    }
    validate_tag_rows("signal.tags", &signal.tags)?;
    validate_ref_rows("signal.evidence_refs", &signal.evidence_refs)?;
    Ok(())
}

fn validate_ref_rows(field: &str, rows: &[String]) -> Result<(), EngineError> {
    if rows.is_empty() {
        return invalid(field, "at least one reference is required");
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        if row.trim().is_empty() || row.len() > 160 {
            return invalid(field, "references must be 1..=160 bytes");
        }
        if !seen.insert(row.as_str()) {
            return invalid(field, "duplicate reference");
        }
    }
    Ok(())
}

fn validate_tag_rows(field: &str, rows: &[String]) -> Result<(), EngineError> {
    if rows.is_empty() || rows.len() > 32 {
        return invalid(field, "must contain 1..=32 tags");
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        validate_token(field, row)?;
        if !seen.insert(row.as_str()) {
            return invalid(field, "duplicate tag");
        }
    }
    Ok(())
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 96 {
        return invalid(field, "must be 1..=96 bytes");
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be 1..=96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgImmortalInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    }
}
