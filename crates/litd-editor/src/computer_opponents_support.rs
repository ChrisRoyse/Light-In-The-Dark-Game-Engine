use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use litd_engine::{
    EngineError,
    ai::{
        AiControllerAction, AiControllerActionBindings, AiControllerDecision, BEHAVIOR_TREE_SCHEMA,
        BehaviorNode, BehaviorNodeKind, BehaviorTree, PerceptionEntityInput,
    },
    calyx_json_store::CalyxJsonStoreSpec,
    sim::FixedTickSim,
};
use serde_json::json;

use crate::mcp_agent_tools::{
    AgentActivationPolicy, AgentBinding, AgentPromptModel, AgentWardEnvelope, BIND_AGENT_TOOL,
    SET_PROMPT_MODEL_TOOL, SET_WARD_ENVELOPE_TOOL, WardBehaviorEnvelope, WardPhysicsEnvelope,
    WardVisualEnvelope, call_mcp_agent_tool, mcp_agent_tools_store::McpAgentState,
};

use super::computer_opponents_types::{
    COMPUTER_OPPONENT_COLLECTION, COMPUTER_OPPONENT_COLUMN, COMPUTER_OPPONENT_PURPOSE,
    COMPUTER_OPPONENT_ROW, COMPUTER_OPPONENT_VAULT_ID, ComputerOpponentAuthoringArgs,
    ComputerOpponentDecisionLedger, ComputerOpponentDefinition, ComputerOpponentObject,
    ComputerOpponentScript, ComputerOpponentTarget, ComputerOpponentToolLedgerSummary,
};
#[path = "computer_opponents_validate.rs"]
mod validate;
pub(super) use validate::{
    invalid_opponent, validate_authoring_args, validate_id, validate_ledger, validate_opponents,
    validate_path, validate_runtime_args,
};

const PRESET_MELEE_GUARDIAN: &str = "melee_guardian";

pub(super) fn call_agent_mutation(
    args: &ComputerOpponentAuthoringArgs,
    opponent: &ComputerOpponentDefinition,
) -> Result<Vec<ComputerOpponentToolLedgerSummary>, EngineError> {
    let suffix = stable_suffix(&opponent.agent_id)?;
    let binding = AgentBinding {
        binding_id: format!("computer.binding.{suffix}"),
        agent_id: opponent.agent_id.clone(),
        object_id: opponent.object_id.clone(),
        display_name: opponent.display_name.clone(),
        deterministic_order_sink: format!("computer.orders.{suffix}"),
        activation: AgentActivationPolicy {
            mode: "manual".to_string(),
            signal_refs: Vec::new(),
            cooldown_ticks: 1,
        },
        enabled: opponent.enabled,
    };
    let prompt_model = AgentPromptModel {
        agent_id: opponent.agent_id.clone(),
        prompt: opponent.script.prompt.clone(),
        model_id: opponent.script.model_id.clone(),
        personality: opponent.script.personality.clone(),
        memory_scope_refs: vec![
            format!("player.{}", opponent.player_id),
            opponent.object_id.clone(),
            opponent.region_id.clone(),
            opponent.script.script_id.clone(),
        ],
    };
    let envelope = ward_envelope(opponent)?;
    Ok(vec![
        call_one_tool(
            BIND_AGENT_TOOL,
            args,
            &opponent.agent_id,
            &suffix,
            "bind",
            json!({ "binding": binding }),
        )?,
        call_one_tool(
            SET_PROMPT_MODEL_TOOL,
            args,
            &opponent.agent_id,
            &suffix,
            "prompt",
            json!({ "prompt_model": prompt_model }),
        )?,
        call_one_tool(
            SET_WARD_ENVELOPE_TOOL,
            args,
            &opponent.agent_id,
            &suffix,
            "ward",
            json!({ "envelope": envelope }),
        )?,
    ])
}

pub(super) fn validate_controller_decisions<const MAX_BODIES: usize>(
    decisions: &[AiControllerDecision],
    opponents: &BTreeMap<&str, &ComputerOpponentDefinition>,
    state: &McpAgentState,
    sim: &FixedTickSim<MAX_BODIES>,
) -> Result<Vec<ComputerOpponentDecisionLedger>, EngineError> {
    let state_index = RuntimeStateIndex::new(state);
    let mut rows = Vec::with_capacity(decisions.len());
    for decision in decisions {
        let opponent = opponents.get(decision.agent_id.as_str()).ok_or_else(|| {
            invalid_opponent(
                format!("controller.{}.agent_id", decision.agent_id),
                "decision has no computer opponent definition",
            )
        })?;
        state_index.validate_intent(&decision.agent_id, action_intent(decision.action))?;
        if let Some(target) = decision.target_body_id
            && sim.body_state(target).is_none()
        {
            return Err(invalid_opponent(
                format!("controller.{}.target", decision.agent_id),
                "decision target body is absent from sim",
            ));
        }
        rows.push(ComputerOpponentDecisionLedger {
            player_id: opponent.player_id,
            agent_id: decision.agent_id.clone(),
            object_id: opponent.object_id.clone(),
            source_id: decision.source_id,
            action: decision.action,
            selected_binding: decision.selected_binding.clone(),
            target_entity_id: decision.target_enemy_id.clone(),
            target_body_id: decision.target_body_id,
            order_count: decision.order_count,
            order_fingerprints: decision.order_fingerprints.clone(),
        });
    }
    Ok(rows)
}

pub(super) struct RuntimeStateIndex<'a> {
    bindings: BTreeMap<&'a str, &'a AgentBinding>,
    prompts: BTreeMap<&'a str, &'a AgentPromptModel>,
    wards: BTreeMap<&'a str, &'a AgentWardEnvelope>,
}

impl<'a> RuntimeStateIndex<'a> {
    pub(super) fn new(state: &'a McpAgentState) -> Self {
        Self {
            bindings: state
                .bindings
                .iter()
                .map(|row| (row.agent_id.as_str(), row))
                .collect(),
            prompts: state
                .prompt_models
                .iter()
                .map(|row| (row.agent_id.as_str(), row))
                .collect(),
            wards: state
                .ward_envelopes
                .iter()
                .map(|row| (row.agent_id.as_str(), row))
                .collect(),
        }
    }

    pub(super) fn require_complete(
        &self,
        opponent: &ComputerOpponentDefinition,
    ) -> Result<(), EngineError> {
        let binding = self
            .bindings
            .get(opponent.agent_id.as_str())
            .ok_or_else(|| {
                invalid_opponent(format!("{}.binding", opponent.agent_id), "missing binding")
            })?;
        if !binding.enabled {
            return Err(invalid_opponent(
                format!("{}.binding.enabled", opponent.agent_id),
                "computer opponent binding is disabled",
            ));
        }
        if binding.object_id != opponent.object_id {
            return Err(invalid_opponent(
                format!("{}.binding.object_id", opponent.agent_id),
                "binding object does not match opponent definition",
            ));
        }
        let prompt = self
            .prompts
            .get(opponent.agent_id.as_str())
            .ok_or_else(|| {
                invalid_opponent(
                    format!("{}.prompt", opponent.agent_id),
                    "missing prompt model",
                )
            })?;
        if prompt.prompt.trim().is_empty() {
            return Err(invalid_opponent(
                format!("{}.prompt", opponent.agent_id),
                "prompt must be non-empty",
            ));
        }
        self.wards.get(opponent.agent_id.as_str()).ok_or_else(|| {
            invalid_opponent(
                format!("{}.ward", opponent.agent_id),
                "missing Ward envelope",
            )
        })?;
        Ok(())
    }

    fn validate_intent(&self, agent_id: &str, intent: &str) -> Result<(), EngineError> {
        let ward = self
            .wards
            .get(agent_id)
            .ok_or_else(|| invalid_opponent(format!("{agent_id}.ward"), "missing Ward envelope"))?;
        if ward
            .behavior
            .forbidden_intents
            .iter()
            .any(|row| row == intent)
        {
            return Err(invalid_opponent(
                format!("{agent_id}.ward.forbidden_intents"),
                format!("{intent} is forbidden"),
            ));
        }
        if ward
            .behavior
            .allowed_intents
            .iter()
            .all(|row| row != intent)
        {
            return Err(invalid_opponent(
                format!("{agent_id}.ward.allowed_intents"),
                format!("{intent} is not allowed",),
            ));
        }
        Ok(())
    }
}

pub(super) fn target_inputs<const MAX_BODIES: usize>(
    targets: &[ComputerOpponentTarget],
    sim: &FixedTickSim<MAX_BODIES>,
) -> Result<Vec<PerceptionEntityInput>, EngineError> {
    if targets.is_empty() {
        return Err(invalid_opponent(
            "targets",
            "at least one target is required",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut out = Vec::with_capacity(targets.len());
    for target in targets {
        validate_id("target.entity_id", &target.entity_id)?;
        validate_id("target.faction", &target.faction)?;
        if !ids.insert(target.entity_id.as_str()) {
            return Err(invalid_opponent("target.entity_id", "duplicate target id"));
        }
        if sim.body_state(target.body_id).is_none() {
            return Err(invalid_opponent("target.body_id", "target body is absent"));
        }
        out.push(PerceptionEntityInput {
            id: target.entity_id.clone(),
            body_id: target.body_id,
            faction: target.faction.clone(),
            nav_cell: target.nav_cell,
            alive: target.alive,
        });
    }
    Ok(out)
}

pub(super) fn validate_live_object<const MAX_BODIES: usize>(
    opponent: &ComputerOpponentDefinition,
    object: &ComputerOpponentObject,
    sim: &FixedTickSim<MAX_BODIES>,
) -> Result<(), EngineError> {
    if !object.alive {
        return Err(invalid_opponent(
            format!("opponents.{}.object.alive", opponent.agent_id),
            "computer opponent object is dead",
        ));
    }
    if object.source_id != opponent.source_id || object.faction != opponent.faction {
        return Err(invalid_opponent(
            format!("opponents.{}.object", opponent.agent_id),
            "runtime object source/faction must match opponent definition",
        ));
    }
    if sim.body_state(object.body_id).is_none() {
        return Err(invalid_opponent(
            format!("opponents.{}.object.body_id", opponent.agent_id),
            "computer opponent body is absent from sim",
        ));
    }
    Ok(())
}

pub(super) fn behavior_tree(script: &ComputerOpponentScript) -> Result<BehaviorTree, EngineError> {
    if script.behavior_preset != PRESET_MELEE_GUARDIAN {
        return Err(invalid_opponent(
            "script.behavior_preset",
            "unsupported behavior preset",
        ));
    }
    Ok(BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: vec![
            node(
                "root",
                BehaviorNodeKind::Selector,
                &["attack_seq", "chase"],
                None,
            ),
            node(
                "attack_seq",
                BehaviorNodeKind::Sequence,
                &["in_range", "los", "attack"],
                None,
            ),
            node(
                "in_range",
                BehaviorNodeKind::Condition,
                &[],
                Some("in_attack_range"),
            ),
            node(
                "los",
                BehaviorNodeKind::Condition,
                &[],
                Some("line_of_sight"),
            ),
            node("attack", BehaviorNodeKind::Action, &[], Some("attack")),
            node("chase", BehaviorNodeKind::Action, &[], Some("chase")),
        ],
    })
}

pub(super) fn action_bindings() -> AiControllerActionBindings {
    AiControllerActionBindings {
        attack: "attack".to_string(),
        chase: "chase".to_string(),
        patrol: "patrol".to_string(),
    }
}

pub(super) fn ward_envelope(
    opponent: &ComputerOpponentDefinition,
) -> Result<AgentWardEnvelope, EngineError> {
    Ok(AgentWardEnvelope {
        agent_id: opponent.agent_id.clone(),
        physics: WardPhysicsEnvelope {
            max_move_units_per_tick: 1,
            allowed_region_ids: vec![opponent.region_id.clone()],
            allow_teleport: false,
        },
        visual: WardVisualEnvelope {
            allowed_effect_ids: vec![format!("vfx.{}", stable_suffix(&opponent.agent_id)?)],
            max_scale_milli: 1000,
            max_emissive_milli: 0,
        },
        behavior: WardBehaviorEnvelope {
            allowed_intents: opponent.script.allowed_intents.clone(),
            forbidden_intents: opponent.script.forbidden_intents.clone(),
            grounding_refs: vec![
                "computer_opponent".to_string(),
                opponent.object_id.clone(),
                opponent.region_id.clone(),
                opponent.script.script_id.clone(),
            ],
        },
    })
}

pub(super) fn opponent_index(
    opponents: &[ComputerOpponentDefinition],
) -> Result<BTreeMap<&str, &ComputerOpponentDefinition>, EngineError> {
    validate_opponents(opponents)?;
    Ok(opponents
        .iter()
        .map(|row| (row.agent_id.as_str(), row))
        .collect())
}

pub(super) fn sorted_opponents(
    opponents: &[ComputerOpponentDefinition],
) -> Vec<&ComputerOpponentDefinition> {
    let mut rows = opponents.iter().collect::<Vec<_>>();
    rows.sort_by_key(|row| row.player_id);
    rows
}

pub(super) fn ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: COMPUTER_OPPONENT_VAULT_ID,
        purpose: COMPUTER_OPPONENT_PURPOSE,
        collection: COMPUTER_OPPONENT_COLLECTION,
        row_key: COMPUTER_OPPONENT_ROW,
        column: COMPUTER_OPPONENT_COLUMN,
    }
}

fn call_one_tool(
    tool: &str,
    args: &ComputerOpponentAuthoringArgs,
    agent_id: &str,
    suffix: &str,
    label: &str,
    extra: serde_json::Value,
) -> Result<ComputerOpponentToolLedgerSummary, EngineError> {
    let mut payload = json!({
        "state_path": args.state_path,
        "ledger_path": args.ledger_dir.join(format!("{suffix}-{label}-ledger")),
        "state_id": args.state_id,
        "ledger_id": format!("issue852_{suffix}_{label}"),
        "request_id": format!("issue852-{suffix}-{label}"),
        "requested_by": args.requested_by,
    });
    merge_object(&mut payload, extra)?;
    let output = call_mcp_agent_tool(tool, payload)?;
    let ledger = output
        .ledger_readback
        .ok_or_else(|| invalid_opponent("agent_tool.ledger", "mutation did not write a ledger"))?;
    Ok(ComputerOpponentToolLedgerSummary {
        agent_id: agent_id.to_string(),
        tool: tool.to_string(),
        ledger_path: ledger.path,
        ledger_sha256: ledger.sha256,
    })
}

fn action_intent(action: AiControllerAction) -> &'static str {
    match action {
        AiControllerAction::Attack => "intent.attack",
        AiControllerAction::Chase => "intent.chase",
        AiControllerAction::Patrol => "intent.patrol",
        AiControllerAction::Idle => "intent.idle",
    }
}

fn node(
    id: &str,
    kind: BehaviorNodeKind,
    children: &[&str],
    binding: Option<&str>,
) -> BehaviorNode {
    BehaviorNode {
        id: id.to_string(),
        kind,
        children: children.iter().map(|child| (*child).to_string()).collect(),
        binding: binding.map(str::to_string),
    }
}

fn merge_object(
    target: &mut serde_json::Value,
    extra: serde_json::Value,
) -> Result<(), EngineError> {
    let target = target
        .as_object_mut()
        .ok_or_else(|| invalid_opponent("arguments", "target must be object"))?;
    let extra = extra
        .as_object()
        .ok_or_else(|| invalid_opponent("arguments", "extra must be object"))?;
    for (key, value) in extra {
        target.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn stable_suffix(agent_id: &str) -> Result<String, EngineError> {
    validate_id("agent_id", agent_id)?;
    Ok(agent_id.replace('.', "_"))
}
