//! Bridge persisted MCP agent bindings into deterministic engine controller orders.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use litd_engine::{
    EngineError,
    ai::{
        AiControllerAction, AiControllerAgentInput, AiControllerRunReport, PerceptionRunReport,
        lower_ai_controller_orders, write_ai_controller_run_json,
    },
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
    nav::BakedNavMesh,
    sim::FixedTickSim,
};
use litd_hash::sha256_hex;

use crate::{
    history::stable_fingerprint,
    mcp_agent_tools::{
        AgentBinding, AgentWardEnvelope, MCP_AGENT_STATE_SCHEMA,
        mcp_agent_tools_store::McpAgentState, read_mcp_agent_state,
    },
    mcp_project_tools::mcp_project_tools_validation::{confine_path, resolve_project_root},
};

#[path = "agent_object_binding_types.rs"]
mod agent_object_binding_types;
use agent_object_binding_types::{
    AGENT_OBJECT_BINDING_COLLECTION, AGENT_OBJECT_BINDING_COLUMN, AGENT_OBJECT_BINDING_ISSUE,
    AGENT_OBJECT_BINDING_PURPOSE, AGENT_OBJECT_BINDING_ROW, AGENT_OBJECT_BINDING_VAULT_ID,
};
pub use agent_object_binding_types::{
    AGENT_OBJECT_BINDING_LEDGER_SCHEMA, AgentObjectBindingDecisionLedger,
    AgentObjectBindingRunArgs, AgentObjectBindingRunLedger, AgentObjectBindingRunReadback,
    BoundAgentObject,
};

struct BindingContext<'a> {
    binding: &'a AgentBinding,
    envelope: &'a AgentWardEnvelope,
    object: &'a BoundAgentObject,
}

pub fn lower_bound_agent_orders<const MAX_BODIES: usize>(
    args: &AgentObjectBindingRunArgs,
    navmesh: &BakedNavMesh,
    perception: &PerceptionRunReport,
    sim: &FixedTickSim<MAX_BODIES>,
) -> Result<AgentObjectBindingRunReadback, EngineError> {
    validate_args(args)?;
    let state_readback = read_mcp_agent_state(&args.state_path)?;
    let state = &state_readback.state;
    let prompts = state
        .prompt_models
        .iter()
        .map(|prompt| (prompt.agent_id.as_str(), prompt))
        .collect::<BTreeMap<_, _>>();
    let envelopes = state
        .ward_envelopes
        .iter()
        .map(|envelope| (envelope.agent_id.as_str(), envelope))
        .collect::<BTreeMap<_, _>>();
    let objects = object_index(&args.object_bodies)?;

    let mut contexts = BTreeMap::new();
    let mut controller_inputs = Vec::new();
    for binding in enabled_bindings(state) {
        let prompt = prompts.get(binding.agent_id.as_str()).ok_or_else(|| {
            invalid_binding(
                format!("agents.{}.prompt_model", binding.agent_id),
                "bound agent must have a prompt/model row before runtime lowering",
            )
        })?;
        if prompt.prompt.trim().is_empty() {
            return Err(invalid_binding(
                format!("agents.{}.prompt_model.prompt", binding.agent_id),
                "prompt must be non-empty",
            ));
        }
        let envelope = envelopes.get(binding.agent_id.as_str()).ok_or_else(|| {
            invalid_binding(
                format!("agents.{}.ward", binding.agent_id),
                "bound agent must have a Ward envelope before runtime lowering",
            )
        })?;
        let object = objects.get(binding.object_id.as_str()).ok_or_else(|| {
            invalid_binding(
                format!("agents.{}.object_id", binding.agent_id),
                format!("missing object/body binding {}", binding.object_id),
            )
        })?;
        validate_live_object(binding.agent_id.as_str(), object, sim)?;
        controller_inputs.push(AiControllerAgentInput {
            id: binding.agent_id.clone(),
            source_id: object.source_id,
            body_id: object.body_id,
            nav_cell: object.nav_cell,
            behavior_tree: args.behavior_tree.clone(),
            path_max_expansions: args.path_max_expansions,
            attack_damage_fp: args.attack_damage_fp,
            action_bindings: args.action_bindings.clone(),
        });
        contexts.insert(
            binding.agent_id.as_str(),
            BindingContext {
                binding,
                envelope,
                object,
            },
        );
    }
    if controller_inputs.is_empty() {
        return Err(invalid_binding(
            "agents.enabled",
            "at least one enabled binding is required",
        ));
    }

    let controller = lower_ai_controller_orders(navmesh, perception, &controller_inputs)?;
    let decisions = validate_controller_report(&controller, &contexts, sim)?;
    let controller_readback = write_ai_controller_run_json(&args.controller_run_path, &controller)?;
    let order_fingerprints = controller
        .orders
        .iter()
        .map(|order| format!("{:016X}", order.fingerprint()))
        .collect::<Vec<_>>();
    let mut ledger = AgentObjectBindingRunLedger {
        schema: AGENT_OBJECT_BINDING_LEDGER_SCHEMA.to_string(),
        issue: AGENT_OBJECT_BINDING_ISSUE,
        run_id: args.run_id.clone(),
        source_of_truth: "MCP agent state row + controller run row + binding run ledger row"
            .to_string(),
        mcp_agent_state_schema: MCP_AGENT_STATE_SCHEMA.to_string(),
        state_path: args.state_path.display().to_string(),
        state_sha256: state_readback.sha256,
        state_fingerprint: state_readback.state_fingerprint,
        controller_run_path: controller_readback.path.display().to_string(),
        controller_sha256: controller_readback.sha256,
        controller_fnv1a64: controller_readback.fnv1a64,
        tick: controller.tick,
        binding_count: controller.agent_count,
        decision_count: decisions.len(),
        order_count: controller.order_count,
        order_fingerprints,
        decisions,
        ledger_fingerprint: String::new(),
    };
    ledger.ledger_fingerprint = stable_fingerprint(&ledger)?;
    write_agent_object_binding_ledger(&args.ledger_path, &ledger)
}

pub fn write_agent_object_binding_ledger(
    path: impl AsRef<Path>,
    ledger: &AgentObjectBindingRunLedger,
) -> Result<AgentObjectBindingRunReadback, EngineError> {
    validate_ledger(ledger)?;
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
            format: "agent-object-binding-ledger-json",
            reason: format!("serialize binding ledger: {error}"),
        })?;
    write_calyx_json_bytes(&ledger_spec(path), &bytes)
        .map_err(|error| invalid_binding("ledger.write", error.to_string()))?;
    let readback = read_agent_object_binding_ledger(path)?;
    if readback.ledger != *ledger {
        return Err(invalid_binding(
            "ledger.readback",
            "ledger readback mismatch",
        ));
    }
    Ok(readback)
}

pub fn read_agent_object_binding_ledger(
    path: impl AsRef<Path>,
) -> Result<AgentObjectBindingRunReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&ledger_spec(path))
        .map_err(|error| invalid_binding("ledger.read", error.to_string()))?
        .ok_or_else(|| {
            invalid_binding(
                "ledger.read",
                format!("missing agent-object binding ledger at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let ledger =
        serde_json::from_slice::<AgentObjectBindingRunLedger>(&bytes).map_err(|error| {
            EngineError::SerializationFailed {
                format: "agent-object-binding-ledger-json",
                reason: format!("parse binding ledger {}: {error}", path.display()),
            }
        })?;
    validate_ledger(&ledger)?;
    Ok(AgentObjectBindingRunReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

fn validate_controller_report<const MAX_BODIES: usize>(
    controller: &AiControllerRunReport,
    contexts: &BTreeMap<&str, BindingContext<'_>>,
    sim: &FixedTickSim<MAX_BODIES>,
) -> Result<Vec<AgentObjectBindingDecisionLedger>, EngineError> {
    let mut rows = Vec::with_capacity(controller.decisions.len());
    for decision in &controller.decisions {
        let context = contexts.get(decision.agent_id.as_str()).ok_or_else(|| {
            invalid_binding(
                format!("controller.decisions.{}.binding", decision.agent_id),
                "controller decision has no persisted binding context",
            )
        })?;
        if decision.order_count == 0 {
            return Err(invalid_binding(
                format!("controller.decisions.{}.orders", decision.agent_id),
                "bound agent produced no deterministic order",
            ));
        }
        let source_orders = controller
            .orders
            .iter()
            .filter(|order| order.source.id == context.object.source_id)
            .collect::<Vec<_>>();
        if source_orders.len() != decision.order_count {
            return Err(invalid_binding(
                format!("controller.decisions.{}.source_orders", decision.agent_id),
                "decision order count must match orders emitted by its binding source id",
            ));
        }
        for order in source_orders {
            if sim.body_state(order.target).is_none() {
                return Err(invalid_binding(
                    format!("controller.orders.{}.target", decision.agent_id),
                    "order target body is absent from sim source of truth",
                ));
            }
        }
        let intent_id = action_intent(decision.action);
        validate_intent(context.envelope, decision.agent_id.as_str(), intent_id)?;
        rows.push(AgentObjectBindingDecisionLedger {
            binding_id: context.binding.binding_id.clone(),
            agent_id: decision.agent_id.clone(),
            object_id: context.binding.object_id.clone(),
            deterministic_order_sink: context.binding.deterministic_order_sink.clone(),
            body_id: context.object.body_id,
            source_id: context.object.source_id,
            action: decision.action,
            intent_id: intent_id.to_string(),
            order_count: decision.order_count,
            order_fingerprints: decision.order_fingerprints.clone(),
        });
    }
    Ok(rows)
}

fn validate_intent(
    envelope: &AgentWardEnvelope,
    agent_id: &str,
    intent_id: &str,
) -> Result<(), EngineError> {
    let behavior = &envelope.behavior;
    if behavior
        .forbidden_intents
        .iter()
        .any(|intent| intent == intent_id)
    {
        return Err(invalid_binding(
            format!("agents.{agent_id}.ward.forbidden_intents"),
            format!("{intent_id} is forbidden by the Ward envelope"),
        ));
    }
    if behavior
        .allowed_intents
        .iter()
        .all(|intent| intent != intent_id)
    {
        return Err(invalid_binding(
            format!("agents.{agent_id}.ward.allowed_intents"),
            format!("{intent_id} is not allowed by the Ward envelope"),
        ));
    }
    Ok(())
}

fn validate_live_object<const MAX_BODIES: usize>(
    agent_id: &str,
    object: &BoundAgentObject,
    sim: &FixedTickSim<MAX_BODIES>,
) -> Result<(), EngineError> {
    if !object.alive {
        return Err(invalid_binding(
            format!("agents.{agent_id}.object.alive"),
            "bound agent object is marked dead",
        ));
    }
    if sim.body_state(object.body_id).is_none() {
        return Err(invalid_binding(
            format!("agents.{agent_id}.object.body_id"),
            "bound agent body is absent from sim source of truth",
        ));
    }
    Ok(())
}

fn object_index(
    objects: &[BoundAgentObject],
) -> Result<BTreeMap<&str, &BoundAgentObject>, EngineError> {
    let mut index = BTreeMap::new();
    let mut source_ids = BTreeSet::new();
    for object in objects {
        validate_id("object.object_id", &object.object_id)?;
        validate_id("object.faction", &object.faction)?;
        if object.source_id == 0 {
            return Err(invalid_binding("object.source_id", "must be non-zero"));
        }
        if index.insert(object.object_id.as_str(), object).is_some() {
            return Err(invalid_binding(
                "object.object_id",
                format!("duplicate object {}", object.object_id),
            ));
        }
        if !source_ids.insert(object.source_id) {
            return Err(invalid_binding(
                "object.source_id",
                format!("duplicate source_id {}", object.source_id),
            ));
        }
    }
    Ok(index)
}

fn enabled_bindings(state: &McpAgentState) -> Vec<&AgentBinding> {
    let mut bindings = state
        .bindings
        .iter()
        .filter(|binding| binding.enabled)
        .collect::<Vec<_>>();
    bindings.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    bindings
}

fn validate_args(args: &AgentObjectBindingRunArgs) -> Result<(), EngineError> {
    validate_id("run_id", &args.run_id)?;
    validate_path("state_path", &args.state_path)?;
    validate_path("controller_run_path", &args.controller_run_path)?;
    validate_path("ledger_path", &args.ledger_path)?;
    // Confine every tool-supplied path within the trusted project root BEFORE any fs
    // effect (#1518). `lower_bound_agent_orders` reads `state_path` and writes
    // `controller_run_path` + `ledger_path`, all taken straight from arguments; without
    // this gate a poisoned run turns the binding tool into an arbitrary read/write
    // primitive outside the project directory. Same policy as `confine_delivery_paths`.
    let project_root = resolve_project_root()?;
    confine_path("state_path", &args.state_path, &project_root)?;
    confine_path(
        "controller_run_path",
        &args.controller_run_path,
        &project_root,
    )?;
    confine_path("ledger_path", &args.ledger_path, &project_root)?;
    if args.path_max_expansions == 0 {
        return Err(invalid_binding(
            "path_max_expansions",
            "must be greater than zero",
        ));
    }
    if args.attack_damage_fp <= 0 {
        return Err(invalid_binding("attack_damage_fp", "must be positive"));
    }
    Ok(())
}

fn validate_ledger(ledger: &AgentObjectBindingRunLedger) -> Result<(), EngineError> {
    let mut expected = ledger.clone();
    let fingerprint = std::mem::take(&mut expected.ledger_fingerprint);
    let decision_order_count = expected
        .decisions
        .iter()
        .map(|decision| decision.order_count)
        .sum::<usize>();
    if expected.schema != AGENT_OBJECT_BINDING_LEDGER_SCHEMA
        || expected.issue != AGENT_OBJECT_BINDING_ISSUE
        || expected.binding_count == 0
        || expected.binding_count != expected.decision_count
        || expected.decision_count != expected.decisions.len()
        || expected.order_count != expected.order_fingerprints.len()
        || expected.order_count != decision_order_count
        || fingerprint.len() != 16
    {
        return Err(invalid_binding(
            "ledger",
            "schema, counts, or fingerprint invalid",
        ));
    }
    let actual = stable_fingerprint(&expected)?;
    if actual != fingerprint {
        return Err(invalid_binding(
            "ledger.fingerprint",
            "fingerprint mismatch",
        ));
    }
    Ok(())
}

fn action_intent(action: AiControllerAction) -> &'static str {
    match action {
        AiControllerAction::Attack => "intent.attack",
        AiControllerAction::Chase => "intent.chase",
        AiControllerAction::Patrol => "intent.patrol",
        AiControllerAction::Idle => "intent.idle",
    }
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_binding(field, "path must be non-empty"));
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(invalid_binding(
            field,
            "must use stable ASCII id characters",
        ));
    }
    Ok(())
}

fn ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: AGENT_OBJECT_BINDING_VAULT_ID,
        purpose: AGENT_OBJECT_BINDING_PURPOSE,
        collection: AGENT_OBJECT_BINDING_COLLECTION,
        row_key: AGENT_OBJECT_BINDING_ROW,
        column: AGENT_OBJECT_BINDING_COLUMN,
    }
}

fn invalid_binding(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::AiInvalidConfig {
        field: format!("agent_object_binding.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "agent_object_binding_tests.rs"]
mod tests;
