use std::collections::BTreeSet;

use crate::EngineError;

use super::{
    object_micro_agents::total_model_bytes,
    object_micro_agents_hash::{effect_fingerprint, readback_fingerprint},
    object_micro_agents_types::*,
};

pub fn validate_object_micro_agent_readback(
    readback: &ObjectMicroAgentReadback,
) -> Result<(), EngineError> {
    if readback.schema != AI_OBJECT_MICRO_AGENT_SCHEMA {
        return invalid("readback.schema", "unsupported object micro-agent schema");
    }
    if readback.agent_count == 0 || readback.signal_count == 0 {
        return invalid(
            "readback.counts",
            "agent and signal counts must be non-zero",
        );
    }
    if readback.decision_count != readback.decisions.len()
        || readback.visual_effect_count != readback.lowered_effects.len()
        || readback.accepted_count + readback.skipped_count != readback.decision_count
    {
        return invalid("readback.counts", "readback counts must match vectors");
    }
    if readback.hot_path_policy != AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY {
        return invalid("readback.hot_path_policy", "unexpected hot-path policy");
    }
    if readback.calyx_live_query_allowed_in_hot_path {
        return invalid(
            "readback.calyx_live_query_allowed_in_hot_path",
            "live Calyx queries are forbidden in deterministic render/sim ticks",
        );
    }
    let mut effects = Vec::new();
    for decision in &readback.decisions {
        if decision.accepted {
            let Some(effect) = &decision.lowered_effect else {
                return invalid(
                    "readback.decision",
                    "accepted decisions need lowered effects",
                );
            };
            let actual = effect_fingerprint(effect)?;
            if effect.fingerprint != actual {
                return Err(EngineError::AiReadbackMismatch {
                    field: "object_micro_agent.effect.fingerprint".to_string(),
                    expected: effect.fingerprint.clone(),
                    actual,
                });
            }
            effects.push(effect.clone());
        } else if decision.lowered_effect.is_some() {
            return invalid(
                "readback.decision",
                "skipped decisions must not emit effects",
            );
        }
    }
    if effects != readback.lowered_effects {
        return invalid(
            "readback.lowered_effects",
            "accepted decision effects must match lowered effects",
        );
    }
    let actual = readback_fingerprint(readback)?;
    if readback.readback_fingerprint != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "object_micro_agent.readback_fingerprint".to_string(),
            expected: readback.readback_fingerprint.clone(),
            actual,
        });
    }
    Ok(())
}

pub(super) fn validate_input(input: &ObjectMicroAgentInput) -> Result<(), EngineError> {
    validate_id("input.run_id", &input.run_id)?;
    validate_id("input.world_id", &input.world_id)?;
    validate_budget(&input.budget)?;
    if input.definitions.is_empty() || input.definitions.len() > input.budget.max_agents {
        return invalid(
            "input.definitions",
            "definition count must fit the object budget",
        );
    }
    if input.signals.is_empty() {
        return invalid("input.signals", "must contain at least one Calyx signal");
    }
    let mut agent_ids = BTreeSet::new();
    let mut object_ids = BTreeSet::new();
    for definition in &input.definitions {
        validate_definition(definition, &input.budget)?;
        if !agent_ids.insert(definition.agent_id.as_str()) {
            return invalid("input.agent_id", "agent ids must be unique");
        }
        if !object_ids.insert(definition.object_id.as_str()) {
            return invalid("input.object_id", "object ids must be unique");
        }
    }
    if total_model_bytes(&input.definitions) > input.budget.max_total_embedded_model_bytes {
        return invalid(
            "input.total_model_bytes",
            "embedded model byte budget exceeded",
        );
    }
    validate_states(&agent_ids, &input.states)?;
    for signal in &input.signals {
        validate_signal(&input.world_id, signal)?;
    }
    Ok(())
}

fn validate_budget(budget: &ObjectMicroAgentBudget) -> Result<(), EngineError> {
    if budget.max_agents == 0 || budget.max_activations_per_tick == 0 {
        return invalid(
            "budget.agent_counts",
            "agent and activation budgets must be non-zero",
        );
    }
    if budget.min_embedded_model_bytes == 0
        || budget.min_embedded_model_bytes > budget.max_embedded_model_bytes
    {
        return invalid("budget.model_bytes", "model byte range is invalid");
    }
    validate_milli(
        "budget.min_signal_strength_milli",
        budget.min_signal_strength_milli,
    )?;
    if budget.max_particles_per_tick == 0 {
        return invalid("budget.max_particles_per_tick", "must be non-zero");
    }
    Ok(())
}

fn validate_definition(
    definition: &ObjectMicroAgentDefinition,
    budget: &ObjectMicroAgentBudget,
) -> Result<(), EngineError> {
    validate_id("definition.agent_id", &definition.agent_id)?;
    validate_id("definition.object_id", &definition.object_id)?;
    validate_text("definition.display_name", &definition.display_name)?;
    validate_id(
        "definition.embedded_model_id",
        &definition.embedded_model_id,
    )?;
    validate_sha256(&definition.embedded_model_sha256)?;
    if definition.embedded_model_bytes < budget.min_embedded_model_bytes
        || definition.embedded_model_bytes > budget.max_embedded_model_bytes
    {
        return invalid("definition.embedded_model_bytes", "must be within 5-20 MiB");
    }
    validate_tag_list("definition.trigger_tags", &definition.trigger_tags)?;
    validate_tag_list("definition.ward_tags", &definition.ward_tags)?;
    validate_id("definition.vfx_effect_id", &definition.vfx_effect_id)?;
    validate_id("definition.material_variant", &definition.material_variant)?;
    if definition.particle_budget == 0 {
        return invalid("definition.particle_budget", "must be non-zero");
    }
    Ok(())
}

fn validate_states(
    agent_ids: &BTreeSet<&str>,
    states: &[ObjectMicroAgentState],
) -> Result<(), EngineError> {
    if states.len() != agent_ids.len() {
        return invalid("states", "must contain one state per object micro-agent");
    }
    let mut seen = BTreeSet::new();
    for state in states {
        validate_id("state.agent_id", &state.agent_id)?;
        if !agent_ids.contains(state.agent_id.as_str()) {
            return invalid("state.agent_id", "state references unknown agent");
        }
        if !seen.insert(state.agent_id.as_str()) {
            return invalid("state.agent_id", "state ids must be unique");
        }
    }
    Ok(())
}

fn validate_signal(world_id: &str, signal: &CalyxObjectVisualSignal) -> Result<(), EngineError> {
    validate_id("signal.signal_id", &signal.signal_id)?;
    if signal.world_id != world_id {
        return invalid("signal.world_id", "must match input world_id");
    }
    validate_milli("signal.strength_milli", signal.strength_milli)?;
    validate_tag_list("signal.tags", &signal.tags)?;
    validate_text("signal.summary", &signal.summary)?;
    validate_ref_list("signal.provenance_refs", &signal.provenance_refs)?;
    if let Some(object_id) = &signal.target_object_id {
        validate_id("signal.target_object_id", object_id)?;
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    validate_text(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return invalid(field, "must contain only stable ASCII id characters");
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    Ok(())
}

fn validate_tag_list(field: &str, tags: &[String]) -> Result<(), EngineError> {
    if tags.is_empty() {
        return invalid(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for tag in tags {
        validate_id(field, tag)?;
        if !seen.insert(tag.as_str()) {
            return invalid(field, "must be unique");
        }
    }
    Ok(())
}

fn validate_ref_list(field: &str, refs: &[String]) -> Result<(), EngineError> {
    if refs.is_empty() {
        return invalid(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for reference in refs {
        validate_text(field, reference)?;
        if !reference.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'#')
        }) {
            return invalid(field, "must contain only stable ASCII reference characters");
        }
        if !seen.insert(reference.as_str()) {
            return invalid(field, "must be unique");
        }
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), EngineError> {
    if !litd_hash::is_sha256_hex(value) {
        return invalid("sha256", "must be a 64 character lowercase hex digest");
    }
    Ok(())
}

fn validate_milli(field: &str, value: u16) -> Result<(), EngineError> {
    if value == 0 || value > 1000 {
        return invalid(field, "must be within 1..=1000");
    }
    Ok(())
}

fn invalid<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::AiInvalidConfig {
        field: format!("object_micro_agent.{field}"),
        reason: reason.into(),
    })
}

#[cfg(test)]
mod issue1514_tests {
    use super::*;

    #[test]
    fn validate_sha256_rejects_uppercase_accepts_canonical_lowercase() {
        // A real canonical digest (lowercase, as sha256_hex always emits).
        let digest = litd_hash::sha256_hex(b"object-micro-agent-model");
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        );
        // Canonical lowercase is accepted.
        assert!(
            validate_sha256(&digest).is_ok(),
            "canonical lowercase digest must pass: {digest}"
        );
        // The SAME digest uppercased is now rejected (was wrongly accepted before
        // #1514 when this used is_ascii_hexdigit()).
        let upper = digest.to_ascii_uppercase();
        assert!(
            validate_sha256(&upper).is_err(),
            "uppercase digest must be rejected: {upper}"
        );
    }
}
