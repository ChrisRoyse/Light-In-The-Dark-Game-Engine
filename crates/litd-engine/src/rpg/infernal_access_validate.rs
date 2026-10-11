use std::collections::BTreeSet;

use crate::EngineError;

use super::{
    CindermarkSource, InfernalAccessCostKind, InfernalAccessPolicy, InfernalAccessState,
    InfernalCircleId, InfernalEntryPath, InfernalEntryRequest, InfernalEntryRule,
    InfernalEscapeAttempt, RPG_INFERNAL_ACCESS_SCHEMA,
};

pub fn validate_infernal_access_policy(policy: &InfernalAccessPolicy) -> Result<(), EngineError> {
    if policy.schema != RPG_INFERNAL_ACCESS_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_INFERNAL_ACCESS_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1066 {
        return invalid("policy.source_issue", "must point at issue 1066");
    }
    if policy.destination_circle != InfernalCircleId::Threshold {
        return invalid("policy.destination_circle", "entry must land at Threshold");
    }
    if !policy.no_recall_inside {
        return invalid("policy.no_recall_inside", "Infernal Deeps must deny recall");
    }
    validate_token("policy.required_gate_id", &policy.required_gate_id)?;
    validate_token(
        "policy.required_long_descent_ref",
        &policy.required_long_descent_ref,
    )?;
    validate_token(
        "policy.required_earned_cindermark_ref",
        &policy.required_earned_cindermark_ref,
    )?;
    validate_token(
        "policy.required_hollow_pact_faction_id",
        &policy.required_hollow_pact_faction_id,
    )?;
    validate_token(
        "policy.required_hollow_pact_power_id",
        &policy.required_hollow_pact_power_id,
    )?;
    if !(1..=64).contains(&policy.max_access_history) {
        return invalid("policy.max_access_history", "must be inside 1..=64");
    }
    validate_entry_rules(policy)?;
    validate_denied_escape_kinds(policy)?;
    Ok(())
}

pub fn validate_infernal_access_state(
    state: &InfernalAccessState,
    policy: &InfernalAccessPolicy,
) -> Result<(), EngineError> {
    validate_infernal_access_policy(policy)?;
    if state.schema != RPG_INFERNAL_ACCESS_SCHEMA {
        return invalid(
            "state.schema",
            format!("expected {RPG_INFERNAL_ACCESS_SCHEMA}"),
        );
    }
    validate_token("state.character_id", &state.character_id)?;
    validate_token("state.party_id", &state.party_id)?;
    validate_token("state.location_id", &state.location_id)?;
    if let Some(faction_id) = &state.faction_id {
        validate_token("state.faction_id", faction_id)?;
    }
    if state.access_history.len() > policy.max_access_history {
        return invalid("state.access_history", "exceeds policy max_access_history");
    }
    for path in &state.access_history {
        entry_rule(policy, *path)?;
    }
    if state.inside_infernal_deeps {
        if state.current_circle.is_none() {
            return invalid(
                "state.current_circle",
                "inside state requires current circle",
            );
        }
        if policy.no_recall_inside && !state.no_recall_lock_active {
            return invalid(
                "state.no_recall_lock_active",
                "inside Infernal Deeps must carry the no-recall lock",
            );
        }
    } else {
        if state.current_circle.is_some() {
            return invalid("state.current_circle", "outside state cannot name a circle");
        }
        if state.no_recall_lock_active {
            return invalid(
                "state.no_recall_lock_active",
                "outside state cannot carry the no-recall lock",
            );
        }
    }
    Ok(())
}

pub fn validate_infernal_entry_request(
    request: &InfernalEntryRequest,
    policy: &InfernalAccessPolicy,
) -> Result<(), EngineError> {
    validate_infernal_access_policy(policy)?;
    if request.schema != RPG_INFERNAL_ACCESS_SCHEMA {
        return invalid(
            "request.schema",
            format!("expected {RPG_INFERNAL_ACCESS_SCHEMA}"),
        );
    }
    validate_token("request.request_id", &request.request_id)?;
    validate_token("request.character_id", &request.character_id)?;
    validate_optional_token("request.at_gate_id", &request.at_gate_id)?;
    validate_optional_token(
        "request.completed_long_descent_ref",
        &request.completed_long_descent_ref,
    )?;
    validate_optional_token("request.cindermark_ref", &request.cindermark_ref)?;
    validate_optional_token("request.lesser_devil_id", &request.lesser_devil_id)?;
    validate_optional_token("request.pact_contract_id", &request.pact_contract_id)?;
    validate_optional_token("request.faction_power_id", &request.faction_power_id)?;
    validate_optional_token("request.thin_place_death_id", &request.thin_place_death_id)?;
    entry_rule(policy, request.path)?;
    match request.path {
        InfernalEntryPath::LongDescentEarnedCindermark => {
            require_gate_and_descent(request, policy)?;
            require_cindermark(
                request,
                &policy.required_earned_cindermark_ref,
                CindermarkSource::EarnedQuestline,
            )
        }
        InfernalEntryPath::LongDescentDevilBrandedCindermark => {
            require_gate_and_descent(request, policy)?;
            require_cindermark(
                request,
                "sigil.cindermark.devil_brand",
                CindermarkSource::DevilBrand,
            )?;
            require_present("request.lesser_devil_id", &request.lesser_devil_id)?;
            require_hidden_cost(request)
        }
        InfernalEntryPath::LesserDevilPact => {
            require_present("request.lesser_devil_id", &request.lesser_devil_id)?;
            require_present("request.pact_contract_id", &request.pact_contract_id)?;
            require_hidden_cost(request)
        }
        InfernalEntryPath::HollowPactSeam => {
            if request.faction_power_id.as_deref()
                != Some(policy.required_hollow_pact_power_id.as_str())
            {
                return invalid(
                    "request.faction_power_id",
                    "Hollow Pact seam requires the infernal seam power",
                );
            }
            require_hidden_cost(request)
        }
        InfernalEntryPath::DraggedUnderAccident => {
            require_present("request.thin_place_death_id", &request.thin_place_death_id)?;
            if !request.current_location_thin_place {
                return invalid(
                    "request.current_location_thin_place",
                    "dragged-under requires a thin-place death",
                );
            }
            if !request.accident_not_player_chosen {
                return invalid(
                    "request.accident_not_player_chosen",
                    "dragged-under must be an accident, not a chosen travel method",
                );
            }
            Ok(())
        }
    }
}

pub fn validate_infernal_escape_attempt(
    attempt: &InfernalEscapeAttempt,
    policy: &InfernalAccessPolicy,
) -> Result<(), EngineError> {
    validate_infernal_access_policy(policy)?;
    if attempt.schema != RPG_INFERNAL_ACCESS_SCHEMA {
        return invalid(
            "attempt.schema",
            format!("expected {RPG_INFERNAL_ACCESS_SCHEMA}"),
        );
    }
    validate_token("attempt.attempt_id", &attempt.attempt_id)?;
    validate_token("attempt.character_id", &attempt.character_id)?;
    validate_token(
        "attempt.declared_destination_id",
        &attempt.declared_destination_id,
    )?;
    if !policy.denied_escape_kinds.contains(&attempt.kind) {
        return invalid("attempt.kind", "escape kind is missing from denied policy");
    }
    Ok(())
}

pub(super) fn entry_rule(
    policy: &InfernalAccessPolicy,
    path: InfernalEntryPath,
) -> Result<&InfernalEntryRule, EngineError> {
    policy
        .entry_rules
        .iter()
        .find(|rule| rule.path == path)
        .ok_or_else(|| invalid_error("entry_rules.path", format!("missing {}", path.label())))
}

pub(super) fn applies_hidden_debt(cost_kind: InfernalAccessCostKind) -> bool {
    !matches!(cost_kind, InfernalAccessCostKind::CindermarkTrialScar)
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

pub(super) fn invalid_error(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("infernal_access.{}", field.into()),
        reason: reason.to_string(),
    }
}

fn validate_entry_rules(policy: &InfernalAccessPolicy) -> Result<(), EngineError> {
    if policy.entry_rules.len() != 5 {
        return invalid("policy.entry_rules", "all five access paths are required");
    }
    let mut seen = BTreeSet::new();
    for rule in &policy.entry_rules {
        if !seen.insert(rule.path) {
            return invalid(
                "policy.entry_rules.path",
                format!("duplicate {}", rule.path.label()),
            );
        }
    }
    for required in [
        InfernalEntryPath::LongDescentEarnedCindermark,
        InfernalEntryPath::LongDescentDevilBrandedCindermark,
        InfernalEntryPath::LesserDevilPact,
        InfernalEntryPath::HollowPactSeam,
        InfernalEntryPath::DraggedUnderAccident,
    ] {
        if !seen.contains(&required) {
            return invalid(
                "policy.entry_rules.path",
                format!("missing {}", required.label()),
            );
        }
    }
    Ok(())
}

fn validate_denied_escape_kinds(policy: &InfernalAccessPolicy) -> Result<(), EngineError> {
    if policy.denied_escape_kinds.len() != 5 {
        return invalid(
            "policy.denied_escape_kinds",
            "recall, teleport, portal, hearth, and extraction denials are required",
        );
    }
    let mut seen = BTreeSet::new();
    for kind in &policy.denied_escape_kinds {
        if !seen.insert(*kind) {
            return invalid("policy.denied_escape_kinds", "duplicate escape denial");
        }
    }
    Ok(())
}

fn require_gate_and_descent(
    request: &InfernalEntryRequest,
    policy: &InfernalAccessPolicy,
) -> Result<(), EngineError> {
    if request.at_gate_id.as_deref() != Some(policy.required_gate_id.as_str()) {
        return invalid("request.at_gate_id", "Gate of Cinders is required");
    }
    if request.completed_long_descent_ref.as_deref()
        != Some(policy.required_long_descent_ref.as_str())
    {
        return invalid(
            "request.completed_long_descent_ref",
            "Long Descent through the Sunken Vaults is required",
        );
    }
    Ok(())
}

fn require_cindermark(
    request: &InfernalEntryRequest,
    expected_ref: &str,
    expected_source: CindermarkSource,
) -> Result<(), EngineError> {
    if request.cindermark_ref.as_deref() != Some(expected_ref) {
        return invalid("request.cindermark_ref", "required Cindermark is missing");
    }
    if request.cindermark_source != Some(expected_source) {
        return invalid("request.cindermark_source", "wrong Cindermark source");
    }
    Ok(())
}

fn require_hidden_cost(request: &InfernalEntryRequest) -> Result<(), EngineError> {
    if !request.accepted_hidden_cost {
        return invalid(
            "request.accepted_hidden_cost",
            "this path requires accepting its hidden cost",
        );
    }
    Ok(())
}

fn require_present(field: &str, value: &Option<String>) -> Result<(), EngineError> {
    if value.is_none() {
        return invalid(field, "must be present");
    }
    Ok(())
}

fn validate_optional_token(field: &str, value: &Option<String>) -> Result<(), EngineError> {
    if let Some(value) = value {
        validate_token(field, value)?;
    }
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
