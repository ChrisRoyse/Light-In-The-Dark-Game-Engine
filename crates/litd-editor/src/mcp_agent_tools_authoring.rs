use litd_engine::EngineError;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

use super::{
    AgentAuthoringVisualBinding, AgentBinding, AgentPromptModel, AgentWardEnvelope,
    AuthorAgentArgs, McpAgentToolCallOutput, WardBehaviorEnvelope, WardPhysicsEnvelope,
    WardVisualEnvelope, confine_agent_paths, invalid_mcp_agent,
    mcp_agent_tools_store::{
        McpAgentToolRecord, load_or_create_mcp_agent_state, write_mcp_agent_state,
        write_mcp_agent_tool_ledger,
    },
    mcp_agent_tools_validation::{
        validate_binding, validate_prompt_model, validate_request_identity, validate_ward_envelope,
    },
    validate_mutating_route,
};

const MODEL_TINY: &str = "model.local.tiny_agent";
const MODEL_LARGER: &str = "model.local.larger_agent";
const PRESET_GUARDIAN_LANE: &str = "guardian_lane";

pub(super) fn author_agent(
    route: McpDomainTool,
    arguments: Value,
    tool_name: &str,
) -> Result<McpAgentToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: AuthorAgentArgs = parse_args(tool_name, arguments)?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    confine_agent_paths(&args.state_path, Some(&args.ledger_path))?;
    let model_id = model_for_tier(&args.model_tier)?;
    let suffix = agent_suffix(&args.agent_id)?;
    let binding = AgentBinding {
        binding_id: format!("binding.{suffix}"),
        agent_id: args.agent_id.clone(),
        object_id: args.object_id.clone(),
        display_name: args.display_name.clone(),
        deterministic_order_sink: format!("orders.{suffix}"),
        activation: args.activation.clone(),
        enabled: true,
    };
    validate_binding(&binding)?;

    let prompt_model = AgentPromptModel {
        agent_id: args.agent_id.clone(),
        prompt: args.prompt.clone(),
        model_id: model_id.to_string(),
        personality: args.personality.clone(),
        memory_scope_refs: memory_scope_refs(&args, &suffix),
    };
    let envelope = ward_for_authoring(&args)?;

    let mut state = load_or_create_mcp_agent_state(&args.state_path, &args.state_id)?;
    reject_duplicate_authoring(&state.bindings, &binding)?;
    state.bindings.push(binding.clone());
    validate_prompt_model(&state, &prompt_model)?;
    validate_ward_envelope(&state, &envelope)?;
    state.prompt_models.push(prompt_model.clone());
    state.ward_envelopes.push(envelope.clone());
    state.update_count += 1;

    let state_readback = write_mcp_agent_state(&args.state_path, &state)?;
    let record = McpAgentToolRecord {
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: args.state_path.display().to_string(),
        state_sha256: state_readback.sha256.clone(),
        state_bytes: state_readback.bytes,
        action_summary:
            "authored no-code agent prompt, model, object binding, visual binding, and Ward envelope"
                .to_string(),
        affected_ids: vec![args.agent_id.clone(), args.object_id.clone()],
        binding_count: state_readback.binding_count,
        prompt_model_count: state_readback.prompt_model_count,
        ward_envelope_count: state_readback.ward_envelope_count,
    };
    let ledger_readback = write_mcp_agent_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(McpAgentToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth:
            "MCP no-code agent authoring request + agent state JSON + agent MCP ledger JSON"
                .to_string(),
        result: json!({
            "state_readback": state_readback,
            "authored_agent": {
                "agent_id": args.agent_id,
                "object_id": args.object_id,
                "model_tier": args.model_tier,
                "model_id": model_id,
                "behavior_preset": args.behavior_preset,
                "personality": args.personality,
                "visual": args.visual,
                "ward_auto_wired": true,
                "deterministic_order_sink": binding.deterministic_order_sink
            }
        }),
        state_readback: Some(state_readback),
        ledger_readback: Some(ledger_readback),
    })
}

fn ward_for_authoring(args: &AuthorAgentArgs) -> Result<AgentWardEnvelope, EngineError> {
    if args.behavior_preset != PRESET_GUARDIAN_LANE {
        return Err(invalid_mcp_agent(
            "author_agent.behavior_preset",
            format!("unsupported behavior preset {}", args.behavior_preset),
        ));
    }
    Ok(AgentWardEnvelope {
        agent_id: args.agent_id.clone(),
        physics: WardPhysicsEnvelope {
            max_move_units_per_tick: 1,
            allowed_region_ids: vec![args.region_id.clone()],
            allow_teleport: false,
        },
        visual: WardVisualEnvelope {
            allowed_effect_ids: vec![
                args.visual.visual_ref.clone(),
                args.visual.particle_ref.clone(),
            ],
            max_scale_milli: 1000,
            max_emissive_milli: 0,
        },
        behavior: WardBehaviorEnvelope {
            allowed_intents: vec!["intent.attack".to_string(), "intent.chase".to_string()],
            forbidden_intents: vec!["intent.attack_civilian".to_string()],
            grounding_refs: grounding_refs(&args.visual, &args.object_id, &args.region_id),
        },
    })
}

fn reject_duplicate_authoring(
    bindings: &[AgentBinding],
    binding: &AgentBinding,
) -> Result<(), EngineError> {
    if bindings
        .iter()
        .any(|existing| existing.agent_id == binding.agent_id)
    {
        return Err(invalid_mcp_agent(
            "author_agent.agent_id",
            format!("{} is already authored", binding.agent_id),
        ));
    }
    if bindings
        .iter()
        .any(|existing| existing.object_id == binding.object_id)
    {
        return Err(invalid_mcp_agent(
            "author_agent.object_id",
            format!("{} already has an authored agent", binding.object_id),
        ));
    }
    Ok(())
}

fn model_for_tier(value: &str) -> Result<&'static str, EngineError> {
    match value {
        "tiny" => Ok(MODEL_TINY),
        "larger" => Ok(MODEL_LARGER),
        other => Err(invalid_mcp_agent(
            "author_agent.model_tier",
            format!("unsupported model tier {other}; expected tiny or larger"),
        )),
    }
}

fn agent_suffix(agent_id: &str) -> Result<String, EngineError> {
    let suffix = agent_id.rsplit('.').next().unwrap_or(agent_id);
    if suffix.trim().is_empty() {
        return Err(invalid_mcp_agent(
            "author_agent.agent_id",
            "agent id must contain a stable suffix",
        ));
    }
    Ok(suffix.to_string())
}

fn memory_scope_refs(args: &AuthorAgentArgs, agent_suffix: &str) -> Vec<String> {
    vec![
        format!("memory.{agent_suffix}"),
        args.object_id.clone(),
        args.visual.character_ref.clone(),
        args.visual.visual_ref.clone(),
        args.region_id.clone(),
    ]
}

fn grounding_refs(
    visual: &AgentAuthoringVisualBinding,
    object_id: &str,
    region_id: &str,
) -> Vec<String> {
    vec![
        "ward.no_code_agent".to_string(),
        object_id.to_string(),
        visual.character_ref.clone(),
        visual.visual_ref.clone(),
        visual.particle_ref.clone(),
        region_id.to_string(),
    ]
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_agent(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}
