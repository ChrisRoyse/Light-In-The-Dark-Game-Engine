use std::collections::BTreeSet;

use super::serialize_sha256;
use serde_json::json;

use crate::{EngineError, sim_order::SimOrderSourceKind};

use super::{
    ImmortalCoembodimentPolicy, ImmortalCoembodimentReadback, ImmortalCoembodimentSession,
    ImmortalEmbodimentDriver, RPG_IMMORTAL_COEMBODIMENT_SCHEMA, RPG_IMMORTAL_SCHEMA,
};

pub fn compose_immortal_coembodiment_session(
    policy: &ImmortalCoembodimentPolicy,
    session: &ImmortalCoembodimentSession,
) -> Result<ImmortalCoembodimentReadback, EngineError> {
    validate_immortal_coembodiment_policy(policy)?;
    validate_session(policy, session)?;
    let first = &session.embodied_decisions[0].decision;
    let ai_driver_present = session
        .embodied_decisions
        .iter()
        .any(|row| row.driver == ImmortalEmbodimentDriver::CalyxAi);
    let human_driver_present = session
        .embodied_decisions
        .iter()
        .any(|row| row.driver == ImmortalEmbodimentDriver::HumanAdmin);
    let human_override_count = session
        .embodied_decisions
        .iter()
        .filter(|row| row.driver == ImmortalEmbodimentDriver::HumanAdmin && row.override_ai_routine)
        .count();
    let handoff_count = session
        .embodied_decisions
        .windows(2)
        .filter(|rows| rows[0].driver != rows[1].driver)
        .count();
    let decision_ledger_sha256s = session
        .embodied_decisions
        .iter()
        .map(|row| row.decision.decision_ledger_sha256.clone())
        .collect::<Vec<_>>();
    let mut readback = ImmortalCoembodimentReadback {
        schema: RPG_IMMORTAL_COEMBODIMENT_SCHEMA.to_string(),
        policy_id: policy.policy_id.clone(),
        source_issue: policy.source_issue,
        session_id: session.session_id.clone(),
        immortal_id: session.immortal_id.clone(),
        display_name: first.display_name.clone(),
        religion_id: first.religion_id.clone(),
        public_identity: session.public_identity.clone(),
        player_visible_single_identity: true,
        ai_driver_present,
        human_driver_present,
        human_override_authoritative: policy.human_override_authoritative,
        handoff_count,
        human_override_count,
        accepted_decision_count: session.embodied_decisions.len(),
        lowered_effect_count: session
            .embodied_decisions
            .iter()
            .map(|row| row.decision.effect_count)
            .sum(),
        deterministic_lowering_required: policy.deterministic_lowering_required,
        ai_hot_path_mutation_allowed: false,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        decision_ledger_sha256s,
        policy_sha256: immortal_coembodiment_policy_hash(policy)?,
        session_ledger_sha256: String::new(),
        result_sha256: String::new(),
    };
    readback.session_ledger_sha256 = serialize_sha256(&session_ledger_payload(&readback))?;
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    validate_immortal_coembodiment_readback(policy, &readback)?;
    Ok(readback)
}

pub fn validate_immortal_coembodiment_policy(
    policy: &ImmortalCoembodimentPolicy,
) -> Result<(), EngineError> {
    if policy.schema != RPG_IMMORTAL_COEMBODIMENT_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_IMMORTAL_COEMBODIMENT_SCHEMA}"),
        );
    }
    validate_token("policy.policy_id", &policy.policy_id)?;
    if policy.source_issue != 1046 {
        return invalid("policy.source_issue", "must point at issue 1046");
    }
    if !policy.ai_driver_required
        || !policy.human_driver_allowed
        || !policy.human_override_authoritative
        || !policy.player_visible_single_identity
        || !policy.deterministic_lowering_required
        || policy.ai_hot_path_mutation_allowed
    {
        return invalid(
            "policy.flags",
            "co-embodiment requires AI, human override, single identity, and deterministic lowering",
        );
    }
    if policy.built_in_map_required || policy.grants_map_or_minimap || policy.grants_gps_markers {
        return invalid(
            "policy.map_flags",
            "Immortal embodiment cannot grant map/GPS",
        );
    }
    Ok(())
}

pub fn validate_immortal_coembodiment_readback(
    policy: &ImmortalCoembodimentPolicy,
    readback: &ImmortalCoembodimentReadback,
) -> Result<(), EngineError> {
    validate_immortal_coembodiment_policy(policy)?;
    if readback.schema != RPG_IMMORTAL_COEMBODIMENT_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {RPG_IMMORTAL_COEMBODIMENT_SCHEMA}"),
        );
    }
    if readback.policy_id != policy.policy_id || readback.source_issue != 1046 {
        return invalid(
            "readback.identity",
            "must match policy and source issue 1046",
        );
    }
    if readback.policy_sha256 != immortal_coembodiment_policy_hash(policy)? {
        return invalid("readback.policy_sha256", "does not match active policy");
    }
    if !readback.player_visible_single_identity
        || !readback.ai_driver_present
        || !readback.human_driver_present
        || !readback.human_override_authoritative
        || readback.handoff_count == 0
        || readback.human_override_count == 0
        || readback.accepted_decision_count < 2
        || readback.lowered_effect_count < 2
        || !readback.deterministic_lowering_required
        || readback.ai_hot_path_mutation_allowed
    {
        return invalid(
            "readback.coverage",
            "does not satisfy co-embodiment contract",
        );
    }
    if readback.built_in_map_required
        || readback.grants_map_or_minimap
        || readback.grants_gps_markers
    {
        return invalid("readback.map_flags", "readback cannot grant map/GPS");
    }
    for hash in &readback.decision_ledger_sha256s {
        validate_hash("readback.decision_ledger_sha256s", hash)?;
    }
    validate_hash(
        "readback.session_ledger_sha256",
        &readback.session_ledger_sha256,
    )?;
    Ok(())
}

pub fn immortal_coembodiment_policy_hash(
    policy: &ImmortalCoembodimentPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn validate_session(
    policy: &ImmortalCoembodimentPolicy,
    session: &ImmortalCoembodimentSession,
) -> Result<(), EngineError> {
    validate_token("session.session_id", &session.session_id)?;
    validate_token("session.immortal_id", &session.immortal_id)?;
    validate_display("session.public_identity", &session.public_identity)?;
    if session.embodied_decisions.len() < 2 || session.embodied_decisions.len() > 16 {
        return invalid(
            "session.embodied_decisions",
            "requires 2..=16 embodied decisions",
        );
    }
    let mut seen_sequences = BTreeSet::new();
    let mut seen_decisions = BTreeSet::new();
    let mut identity = None;
    for row in &session.embodied_decisions {
        if !seen_sequences.insert(row.sequence) {
            return invalid("session.sequence", "duplicate sequence");
        }
        validate_driver_row(policy, session, row)?;
        if !seen_decisions.insert(row.decision.decision_id.as_str()) {
            return invalid("session.decision_id", "duplicate decision");
        }
        let current = (
            row.decision.immortal_id.as_str(),
            row.decision.display_name.as_str(),
            row.decision.religion_id.as_str(),
        );
        if let Some(expected) = identity {
            if expected != current {
                return invalid(
                    "session.identity",
                    "all rows must be the same Immortal identity",
                );
            }
        } else {
            identity = Some(current);
        }
    }
    if !session
        .embodied_decisions
        .iter()
        .any(|row| row.driver == ImmortalEmbodimentDriver::CalyxAi)
        || !session
            .embodied_decisions
            .iter()
            .any(|row| row.driver == ImmortalEmbodimentDriver::HumanAdmin)
    {
        return invalid(
            "session.drivers",
            "AI and human drivers must both be present",
        );
    }
    if !session
        .embodied_decisions
        .iter()
        .any(|row| row.driver == ImmortalEmbodimentDriver::HumanAdmin && row.override_ai_routine)
    {
        return invalid(
            "session.override",
            "human override must be explicitly ledgered",
        );
    }
    Ok(())
}

fn validate_driver_row(
    policy: &ImmortalCoembodimentPolicy,
    session: &ImmortalCoembodimentSession,
    row: &super::ImmortalEmbodiedDecision,
) -> Result<(), EngineError> {
    if row.decision.schema != RPG_IMMORTAL_SCHEMA {
        return invalid("decision.schema", format!("expected {RPG_IMMORTAL_SCHEMA}"));
    }
    if row.decision.immortal_id != session.immortal_id
        || row.decision.display_name != session.public_identity
        || !row.decision.accepted
        || row.decision.effect_count == 0
    {
        return invalid(
            "decision.identity",
            "accepted decision must match the public Immortal identity",
        );
    }
    validate_hash(
        "decision.decision_ledger_sha256",
        &row.decision.decision_ledger_sha256,
    )?;
    validate_hash("decision.result_sha256", &row.decision.result_sha256)?;
    match row.driver {
        ImmortalEmbodimentDriver::CalyxAi => {
            if row.human_admin_id.is_some() || row.override_ai_routine {
                return invalid(
                    "driver.calyx_ai",
                    "AI rows cannot carry human override data",
                );
            }
        }
        ImmortalEmbodimentDriver::HumanAdmin => {
            if !policy.human_driver_allowed
                || row.human_admin_id.as_deref().is_none_or(str::is_empty)
                || !row.override_ai_routine
            {
                return invalid(
                    "driver.human_admin",
                    "human rows require admin id and override ledger",
                );
            }
        }
    }
    if policy.deterministic_lowering_required {
        for effect in &row.decision.lowered_effects {
            validate_hash("effect.effect_sha256", &effect.effect_sha256)?;
            if let Some(order) = effect.sim_order
                && order.source.kind != SimOrderSourceKind::LoweredAgent
            {
                return invalid(
                    "effect.sim_order.source",
                    "sim orders must be lowered-agent orders",
                );
            }
        }
    }
    Ok(())
}

fn session_ledger_payload(readback: &ImmortalCoembodimentReadback) -> serde_json::Value {
    json!({
        "session_id": readback.session_id,
        "immortal_id": readback.immortal_id,
        "public_identity": readback.public_identity,
        "ai_driver_present": readback.ai_driver_present,
        "human_driver_present": readback.human_driver_present,
        "handoff_count": readback.handoff_count,
        "human_override_count": readback.human_override_count,
        "decision_ledger_sha256s": readback.decision_ledger_sha256s,
    })
}

fn readback_fingerprint(readback: &ImmortalCoembodimentReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "policy_id": readback.policy_id,
        "source_issue": readback.source_issue,
        "session_id": readback.session_id,
        "immortal_id": readback.immortal_id,
        "public_identity": readback.public_identity,
        "player_visible_single_identity": readback.player_visible_single_identity,
        "ai_driver_present": readback.ai_driver_present,
        "human_driver_present": readback.human_driver_present,
        "human_override_authoritative": readback.human_override_authoritative,
        "handoff_count": readback.handoff_count,
        "human_override_count": readback.human_override_count,
        "accepted_decision_count": readback.accepted_decision_count,
        "lowered_effect_count": readback.lowered_effect_count,
        "deterministic_lowering_required": readback.deterministic_lowering_required,
        "ai_hot_path_mutation_allowed": readback.ai_hot_path_mutation_allowed,
        "built_in_map_required": readback.built_in_map_required,
        "policy_sha256": readback.policy_sha256,
        "session_ledger_sha256": readback.session_ledger_sha256,
    })
}

fn validate_hash(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid(field, "must be a SHA-256 hex digest");
    }
    Ok(())
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
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

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgImmortalInvalidConfig {
        field: format!("coembodiment.{}", field.into()),
        reason: reason.to_string(),
    })
}
