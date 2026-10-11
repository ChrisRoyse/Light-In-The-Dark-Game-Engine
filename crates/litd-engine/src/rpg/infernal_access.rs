use super::serialize_sha256;

use crate::EngineError;

use super::{
    InfernalAccessPolicy, InfernalAccessReadback, InfernalAccessState, InfernalEntryPath,
    InfernalEntryRequest, InfernalEscapeAttempt, InfernalEscapeReadback,
    RPG_INFERNAL_ACCESS_SCHEMA,
    infernal_access_validate::{
        applies_hidden_debt, entry_rule, invalid, invalid_error, validate_infernal_access_state,
        validate_infernal_entry_request, validate_infernal_escape_attempt,
    },
};

pub fn resolve_infernal_entry(
    state: &InfernalAccessState,
    policy: &InfernalAccessPolicy,
    request: &InfernalEntryRequest,
) -> Result<InfernalAccessReadback, EngineError> {
    validate_infernal_access_state(state, policy)?;
    validate_infernal_entry_request(request, policy)?;
    if state.character_id != request.character_id {
        return invalid("request.character_id", "does not match access state");
    }
    if state.inside_infernal_deeps {
        return invalid("state.inside_infernal_deeps", "character is already inside");
    }
    if request.path == InfernalEntryPath::HollowPactSeam
        && state.faction_id.as_deref() != Some(policy.required_hollow_pact_faction_id.as_str())
    {
        return invalid(
            "state.faction_id",
            "Hollow Pact seam requires Hollow Pact membership",
        );
    }

    let rule = entry_rule(policy, request.path)?;
    let state_before_sha256 = infernal_access_state_hash(state)?;
    let policy_sha256 = infernal_access_policy_hash(policy)?;
    let request_sha256 = serialize_sha256(request)?;
    let mut after = state.clone();
    after.inside_infernal_deeps = true;
    after.current_circle = Some(policy.destination_circle);
    after.no_recall_lock_active = policy.no_recall_inside;
    if applies_hidden_debt(rule.cost_kind) {
        after.infernal_debt_count = after
            .infernal_debt_count
            .checked_add(1)
            .ok_or_else(|| invalid_error("state.infernal_debt_count", "overflowed u16"))?;
    }
    after.access_history.push(request.path);
    validate_infernal_access_state(&after, policy)?;
    let state_after_sha256 = infernal_access_state_hash(&after)?;
    let mut readback = InfernalAccessReadback {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        request_id: request.request_id.clone(),
        character_id: state.character_id.clone(),
        path: request.path,
        granted: true,
        destination_circle: policy.destination_circle,
        no_recall_lock_active: after.no_recall_lock_active,
        cost_kind: rule.cost_kind,
        infernal_debt_before: state.infernal_debt_count,
        infernal_debt_after: after.infernal_debt_count,
        state_before_sha256,
        state_after_sha256,
        policy_sha256,
        request_sha256,
        result_sha256: String::new(),
        state_after: after,
    };
    readback.result_sha256 = serialize_sha256(&entry_fingerprint(&readback))?;
    Ok(readback)
}

pub fn resolve_infernal_escape_attempt(
    state: &InfernalAccessState,
    policy: &InfernalAccessPolicy,
    attempt: &InfernalEscapeAttempt,
) -> Result<InfernalEscapeReadback, EngineError> {
    validate_infernal_access_state(state, policy)?;
    validate_infernal_escape_attempt(attempt, policy)?;
    if state.character_id != attempt.character_id {
        return invalid("attempt.character_id", "does not match access state");
    }
    if !state.inside_infernal_deeps || !state.no_recall_lock_active {
        return invalid(
            "state.no_recall_lock_active",
            "Infernal escape denial only applies inside the locked Deeps",
        );
    }
    if !policy.denied_escape_kinds.contains(&attempt.kind) {
        return invalid("attempt.kind", "escape kind is not denied by policy");
    }
    let state_before_sha256 = infernal_access_state_hash(state)?;
    let state_after_sha256 = infernal_access_state_hash(state)?;
    let policy_sha256 = infernal_access_policy_hash(policy)?;
    let attempt_sha256 = serialize_sha256(attempt)?;
    let mut readback = InfernalEscapeReadback {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        attempt_id: attempt.attempt_id.clone(),
        character_id: state.character_id.clone(),
        kind: attempt.kind,
        denied: true,
        denial_reason: "infernal_no_recall_lock".to_string(),
        state_unchanged: state_before_sha256 == state_after_sha256,
        state_before_sha256,
        state_after_sha256,
        policy_sha256,
        attempt_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&escape_fingerprint(&readback))?;
    Ok(readback)
}

pub fn infernal_access_state_hash(state: &InfernalAccessState) -> Result<String, EngineError> {
    serialize_sha256(state)
}

pub fn infernal_access_policy_hash(policy: &InfernalAccessPolicy) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn entry_fingerprint(readback: &InfernalAccessReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "request_id": readback.request_id,
        "character_id": readback.character_id,
        "path": readback.path,
        "granted": readback.granted,
        "destination_circle": readback.destination_circle,
        "no_recall_lock_active": readback.no_recall_lock_active,
        "cost_kind": readback.cost_kind,
        "infernal_debt_after": readback.infernal_debt_after,
        "state_before_sha256": readback.state_before_sha256,
        "state_after_sha256": readback.state_after_sha256,
        "policy_sha256": readback.policy_sha256,
        "request_sha256": readback.request_sha256,
    })
}

fn escape_fingerprint(readback: &InfernalEscapeReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "attempt_id": readback.attempt_id,
        "character_id": readback.character_id,
        "kind": readback.kind,
        "denied": readback.denied,
        "denial_reason": readback.denial_reason,
        "state_unchanged": readback.state_unchanged,
        "state_before_sha256": readback.state_before_sha256,
        "state_after_sha256": readback.state_after_sha256,
        "policy_sha256": readback.policy_sha256,
        "attempt_sha256": readback.attempt_sha256,
    })
}
