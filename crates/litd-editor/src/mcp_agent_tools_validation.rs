use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    AgentBinding, AgentPromptModel, AgentWardEnvelope, invalid_mcp_agent,
    mcp_agent_tools_store::McpAgentState,
};

const MAX_TEXT_LEN: usize = 8192;
const MAX_LIST_LEN: usize = 128;
const MAX_MOVE_UNITS_PER_TICK: u32 = 10_000;
const MAX_SCALE_MILLI: u32 = 10_000;
const MAX_EMISSIVE_MILLI: u32 = 5_000;

pub(super) fn validate_request_identity(
    request_id: &str,
    requested_by: &str,
) -> Result<(), EngineError> {
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)
}

pub(super) fn validate_binding(binding: &AgentBinding) -> Result<(), EngineError> {
    validate_id("binding.binding_id", &binding.binding_id)?;
    validate_id("binding.agent_id", &binding.agent_id)?;
    validate_id("binding.object_id", &binding.object_id)?;
    validate_single_line("binding.display_name", &binding.display_name)?;
    validate_id(
        "binding.deterministic_order_sink",
        &binding.deterministic_order_sink,
    )?;
    validate_activation(binding)?;
    if !binding.enabled {
        return Err(invalid_mcp_agent(
            "binding.enabled",
            "new bindings must be enabled so activation can be FSV-read",
        ));
    }
    Ok(())
}

pub(super) fn validate_prompt_model(
    state: &McpAgentState,
    prompt_model: &AgentPromptModel,
) -> Result<(), EngineError> {
    validate_agent_exists(state, &prompt_model.agent_id, "prompt_model.agent_id")?;
    validate_prompt("prompt_model.prompt", &prompt_model.prompt)?;
    validate_id("prompt_model.model_id", &prompt_model.model_id)?;
    validate_single_line("prompt_model.personality", &prompt_model.personality)?;
    validate_id_list(
        "prompt_model.memory_scope_refs",
        &prompt_model.memory_scope_refs,
    )
}

pub(super) fn validate_ward_envelope(
    state: &McpAgentState,
    envelope: &AgentWardEnvelope,
) -> Result<(), EngineError> {
    validate_agent_exists(state, &envelope.agent_id, "ward.agent_id")?;
    if envelope.physics.max_move_units_per_tick == 0
        || envelope.physics.max_move_units_per_tick > MAX_MOVE_UNITS_PER_TICK
    {
        return Err(invalid_mcp_agent(
            "ward.physics.max_move_units_per_tick",
            format!("value must be in 1..={MAX_MOVE_UNITS_PER_TICK}"),
        ));
    }
    if envelope.physics.allow_teleport {
        return Err(invalid_mcp_agent(
            "ward.physics.allow_teleport",
            "teleport is outside the deterministic movement envelope",
        ));
    }
    validate_id_list(
        "ward.physics.allowed_region_ids",
        &envelope.physics.allowed_region_ids,
    )?;
    if envelope.visual.max_scale_milli == 0 || envelope.visual.max_scale_milli > MAX_SCALE_MILLI {
        return Err(invalid_mcp_agent(
            "ward.visual.max_scale_milli",
            format!("value must be in 1..={MAX_SCALE_MILLI}"),
        ));
    }
    if envelope.visual.max_emissive_milli > MAX_EMISSIVE_MILLI {
        return Err(invalid_mcp_agent(
            "ward.visual.max_emissive_milli",
            format!("value must be <= {MAX_EMISSIVE_MILLI}"),
        ));
    }
    validate_id_list(
        "ward.visual.allowed_effect_ids",
        &envelope.visual.allowed_effect_ids,
    )?;
    if envelope.behavior.allowed_intents.is_empty() || envelope.behavior.grounding_refs.is_empty() {
        return Err(invalid_mcp_agent(
            "ward.behavior",
            "allowed intents and grounding refs are required",
        ));
    }
    validate_id_list(
        "ward.behavior.allowed_intents",
        &envelope.behavior.allowed_intents,
    )?;
    validate_id_list(
        "ward.behavior.forbidden_intents",
        &envelope.behavior.forbidden_intents,
    )?;
    validate_id_list(
        "ward.behavior.grounding_refs",
        &envelope.behavior.grounding_refs,
    )?;
    let allowed = envelope
        .behavior
        .allowed_intents
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if envelope
        .behavior
        .forbidden_intents
        .iter()
        .any(|intent| allowed.contains(intent.as_str()))
    {
        return Err(invalid_mcp_agent(
            "ward.behavior.intents",
            "allowed and forbidden intents must not overlap",
        ));
    }
    Ok(())
}

pub(super) fn validate_agent_state(state: &McpAgentState) -> Result<(), EngineError> {
    let mut agents = BTreeSet::new();
    let mut bindings = BTreeSet::new();
    for binding in &state.bindings {
        validate_binding(binding)?;
        if !bindings.insert(binding.binding_id.as_str()) {
            return Err(invalid_mcp_agent(
                "state.bindings",
                format!("duplicate binding {}", binding.binding_id),
            ));
        }
        if !agents.insert(binding.agent_id.as_str()) {
            return Err(invalid_mcp_agent(
                "state.bindings",
                format!("duplicate agent binding {}", binding.agent_id),
            ));
        }
    }
    let mut prompts = BTreeSet::new();
    for prompt_model in &state.prompt_models {
        validate_prompt_model(state, prompt_model)?;
        if !prompts.insert(prompt_model.agent_id.as_str()) {
            return Err(invalid_mcp_agent(
                "state.prompt_models",
                format!("duplicate prompt model {}", prompt_model.agent_id),
            ));
        }
    }
    let mut envelopes = BTreeSet::new();
    for envelope in &state.ward_envelopes {
        validate_ward_envelope(state, envelope)?;
        if !envelopes.insert(envelope.agent_id.as_str()) {
            return Err(invalid_mcp_agent(
                "state.ward_envelopes",
                format!("duplicate Ward envelope {}", envelope.agent_id),
            ));
        }
    }
    Ok(())
}

fn validate_agent_exists(
    state: &McpAgentState,
    agent_id: &str,
    field: &str,
) -> Result<(), EngineError> {
    validate_id(field, agent_id)?;
    if !state
        .bindings
        .iter()
        .any(|binding| binding.agent_id == agent_id)
    {
        return Err(invalid_mcp_agent(
            field,
            format!("missing bound agent {agent_id}"),
        ));
    }
    Ok(())
}

fn validate_activation(binding: &AgentBinding) -> Result<(), EngineError> {
    validate_single_line("binding.activation.mode", &binding.activation.mode)?;
    match binding.activation.mode.as_str() {
        "manual" => {}
        "signal" => {
            if binding.activation.signal_refs.is_empty() {
                return Err(invalid_mcp_agent(
                    "binding.activation.signal_refs",
                    "signal activation requires at least one signal ref",
                ));
            }
        }
        other => {
            return Err(invalid_mcp_agent(
                "binding.activation.mode",
                format!("unsupported activation mode {other}"),
            ));
        }
    }
    if binding.activation.cooldown_ticks == 0 {
        return Err(invalid_mcp_agent(
            "binding.activation.cooldown_ticks",
            "cooldown must be greater than zero",
        ));
    }
    validate_id_list(
        "binding.activation.signal_refs",
        &binding.activation.signal_refs,
    )
}

fn validate_id_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_agent(
            field,
            format!("list length must be <= {MAX_LIST_LEN}"),
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_id(field, value)?;
        if !seen.insert(value.as_str()) {
            return Err(invalid_mcp_agent(field, format!("duplicate value {value}")));
        }
    }
    Ok(())
}

fn validate_prompt(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains('\0') {
        return Err(invalid_mcp_agent(
            field,
            "prompt must be bounded non-empty text",
        ));
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_agent(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_mcp_agent(field, "id cannot be empty"));
    };
    if !(first.is_ascii_alphanumeric() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(invalid_mcp_agent(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}
