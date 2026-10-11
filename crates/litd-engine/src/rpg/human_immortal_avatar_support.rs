use serde_json::json;

use crate::EngineError;

use super::{HumanImmortalAvatarEffect, HumanImmortalAvatarReadback};

pub(super) fn audit_ledger_payload(readback: &HumanImmortalAvatarReadback) -> serde_json::Value {
    json!({
        "session_id": readback.session_id,
        "admin_id": readback.admin_id,
        "public_identity": readback.public_identity,
        "coembodiment_result_sha256": readback.coembodiment_result_sha256,
        "command_count": readback.command_count,
        "effects": readback.lowered_effects,
        "built_in_map_required": readback.built_in_map_required,
        "grants_map_or_minimap": readback.grants_map_or_minimap,
        "grants_gps_markers": readback.grants_gps_markers,
    })
}

pub(super) fn readback_fingerprint(readback: &HumanImmortalAvatarReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "policy_id": readback.policy_id,
        "source_issue": readback.source_issue,
        "session_id": readback.session_id,
        "admin_id": readback.admin_id,
        "immortal_id": readback.immortal_id,
        "public_identity": readback.public_identity,
        "command_count": readback.command_count,
        "possession_order_count": readback.possession_order_count,
        "grant_count": readback.grant_count,
        "revocable_grant_count": readback.revocable_grant_count,
        "lowered_effect_count": readback.lowered_effect_count,
        "ai_hot_path_mutation_allowed": readback.ai_hot_path_mutation_allowed,
        "built_in_map_required": readback.built_in_map_required,
        "policy_sha256": readback.policy_sha256,
        "session_sha256": readback.session_sha256,
        "audit_ledger_sha256": readback.audit_ledger_sha256,
    })
}

pub(super) fn effect_payload(effect: &HumanImmortalAvatarEffect) -> serde_json::Value {
    json!({
        "effect_id": effect.effect_id,
        "command_id": effect.command_id,
        "kind": effect.kind,
        "subject_ref": effect.subject_ref,
        "artifact_ref": effect.artifact_ref,
        "ledger_entry": effect.ledger_entry,
        "audit_actor_id": effect.audit_actor_id,
        "public_identity": effect.public_identity,
        "revocable": effect.revocable,
        "sim_order": effect.sim_order,
        "evidence_refs": effect.evidence_refs,
    })
}

pub(super) fn validate_hash(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid(field, "must be a SHA-256 hex digest");
    }
    Ok(())
}

pub(super) fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    Ok(())
}

pub(super) fn validate_ref(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 160 || !value.is_ascii() {
        return invalid(field, "must be non-empty ASCII and <= 160 bytes");
    }
    if value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return invalid(field, "must not contain whitespace");
    }
    Ok(())
}

pub(super) fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > max_len {
        return invalid(field, format!("must be <= {max_len} bytes"));
    }
    Ok(())
}

pub(super) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
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

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(EngineError::RpgImmortalInvalidConfig {
        field: format!("human_immortal_avatar.{}", field.into()),
        reason: reason.to_string(),
    })
}
