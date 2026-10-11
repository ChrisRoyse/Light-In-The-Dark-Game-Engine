use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Fixed, FixedVec3},
    nav::{
        BakedNavMesh, NavAvoidanceAgent, NavAvoidanceResolution, NavCellCoord, NavIndex, NavPath,
        NavPathQuery, find_path_with_index, resolve_local_avoidance,
    },
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

use super::{
    AgentPerception, BehaviorLeafState, BehaviorStatus, BehaviorTickReport, BehaviorTree,
    PerceivedEnemy, PerceptionRunReport,
    controller_hash::compute_controller_run_fnv1a64,
    controller_validate::{
        invalid_config, validate_controller_inputs, verify_ai_controller_run_integrity,
    },
    tick_behavior_tree, verify_perception_run_integrity,
};

pub const AI_CONTROLLER_RUN_SCHEMA: &str = "litd.ai.controller.run.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiControllerAction {
    Attack,
    Chase,
    Patrol,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiControllerActionBindings {
    pub attack: String,
    pub chase: String,
    pub patrol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiControllerAgentInput {
    pub id: String,
    pub source_id: u32,
    pub body_id: SimBodyId,
    pub nav_cell: NavCellCoord,
    pub behavior_tree: BehaviorTree,
    pub path_max_expansions: usize,
    pub attack_damage_fp: i64,
    pub action_bindings: AiControllerActionBindings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiControllerDecision {
    pub agent_id: String,
    pub source_id: u32,
    pub body_id: SimBodyId,
    pub action: AiControllerAction,
    pub selected_binding: Option<String>,
    pub target_enemy_id: Option<String>,
    pub target_body_id: Option<SimBodyId>,
    pub path_fnv1a64: Option<String>,
    pub avoidance_fnv1a64: Option<String>,
    pub blackboard_fnv1a64: String,
    pub behavior_tick: BehaviorTickReport,
    pub order_count: usize,
    pub order_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiControllerRunReport {
    pub schema: String,
    pub tick: u64,
    pub perception_fnv1a64: String,
    pub navmesh_fnv1a64: String,
    pub agent_count: usize,
    pub order_count: usize,
    pub decisions: Vec<AiControllerDecision>,
    pub orders: Vec<SimOrder>,
    pub fnv1a64: String,
}

struct PendingDecision {
    input: AiControllerAgentInput,
    action: AiControllerAction,
    selected_binding: Option<String>,
    target: Option<ControllerTarget>,
    blackboard_fnv1a64: String,
    behavior_tick: BehaviorTickReport,
    path: Option<NavPath>,
}

#[derive(Clone)]
struct ControllerTarget {
    enemy_id: String,
    body_id: SimBodyId,
    nav_cell: NavCellCoord,
}

pub fn lower_ai_controller_orders(
    navmesh: &BakedNavMesh,
    perception: &PerceptionRunReport,
    agents: &[AiControllerAgentInput],
) -> Result<AiControllerRunReport, EngineError> {
    // Verify + index the immutable navmesh ONCE per controller run instead of re-verifying
    // and rebuilding the coordinate index inside every per-agent path query (#1486 #1).
    let nav_index = NavIndex::build(navmesh)?;
    verify_perception_run_integrity(perception)?;
    validate_controller_inputs(navmesh, perception, agents)?;

    let perceptions = perception_index(perception);
    let mut pending = Vec::with_capacity(agents.len());
    let mut avoidance_agents = Vec::new();
    for input in sorted_inputs(agents) {
        let sensed =
            perceptions
                .get(input.id.as_str())
                .ok_or_else(|| EngineError::AiInvalidConfig {
                    field: format!("agents.{}.perception", input.id),
                    reason: "controller input has no matching perception agent".to_string(),
                })?;
        let attack_target = nearest_attackable(sensed);
        let visible_target = nearest_visible(sensed);
        let leaf_state = controller_leaf_state(
            &sensed.blackboard.leaf_state,
            &input.action_bindings,
            attack_target.is_some(),
            visible_target.is_some(),
        )?;
        let behavior_tick = tick_behavior_tree(&input.behavior_tree, &leaf_state, perception.tick)?;
        let selected_binding = selected_action_binding(&input.behavior_tree, &behavior_tick)?;
        let action = action_from_binding(
            selected_binding.as_deref(),
            &input.action_bindings,
            &behavior_tick,
        )?;
        let target = match action {
            AiControllerAction::Attack => require_target(&input.id, "attack", attack_target)?,
            AiControllerAction::Chase => require_target(&input.id, "chase", visible_target)?,
            AiControllerAction::Patrol | AiControllerAction::Idle => None,
        };
        let path = if action == AiControllerAction::Chase {
            let target = target.as_ref().expect("chase target exists");
            let path = find_path_with_index(
                navmesh,
                &nav_index,
                NavPathQuery::new(input.nav_cell, target.nav_cell, input.path_max_expansions),
            )?;
            avoidance_agents.push(NavAvoidanceAgent {
                id: input.id.clone(),
                current: input.nav_cell,
                path: path.clone(),
            });
            Some(path)
        } else {
            None
        };
        pending.push(PendingDecision {
            input: input.clone(),
            action,
            selected_binding,
            target,
            blackboard_fnv1a64: sensed.blackboard.fnv1a64.clone(),
            behavior_tick,
            path,
        });
    }

    let avoidance = if avoidance_agents.is_empty() {
        None
    } else {
        Some(resolve_local_avoidance(
            navmesh,
            perception.tick,
            &avoidance_agents,
        )?)
    };
    let mut orders = Vec::new();
    let decisions = pending
        .into_iter()
        .map(|decision| lower_pending(decision, perception.tick, avoidance.as_ref(), &mut orders))
        .collect::<Result<Vec<_>, _>>()?;
    let mut report = AiControllerRunReport {
        schema: AI_CONTROLLER_RUN_SCHEMA.to_string(),
        tick: perception.tick,
        perception_fnv1a64: perception.fnv1a64.clone(),
        navmesh_fnv1a64: navmesh.fnv1a64.clone(),
        agent_count: decisions.len(),
        order_count: orders.len(),
        decisions,
        orders,
        fnv1a64: String::new(),
    };
    report.fnv1a64 = compute_controller_run_fnv1a64(&report);
    verify_ai_controller_run_integrity(&report)?;
    Ok(report)
}

fn lower_pending(
    pending: PendingDecision,
    tick: u64,
    avoidance: Option<&NavAvoidanceResolution>,
    orders: &mut Vec<SimOrder>,
) -> Result<AiControllerDecision, EngineError> {
    let start = orders.len();
    let source = SimOrderSource::lowered_agent(pending.input.source_id);
    let execute_tick = tick.checked_add(1).ok_or(EngineError::SimTickOverflow)?;
    match pending.action {
        AiControllerAction::Attack => {
            let target = pending.target.as_ref().expect("attack target exists");
            orders.push(SimOrder::new(
                execute_tick,
                source,
                1,
                pending.input.body_id,
                SimOrderKind::SetVelocity(FixedVec3::ZERO),
            ));
            orders.push(SimOrder::new(
                execute_tick,
                source,
                2,
                target.body_id,
                SimOrderKind::ApplyAttack {
                    attacker: pending.input.body_id,
                    damage_fp: pending.input.attack_damage_fp,
                },
            ));
        }
        AiControllerAction::Chase => {
            let resolution = avoidance_resolution(avoidance, &pending.input.id)?;
            orders.push(SimOrder::new(
                execute_tick,
                source,
                1,
                pending.input.body_id,
                SimOrderKind::SetVelocity(FixedVec3::new(
                    Fixed::from_raw(resolution.velocity_x_fp),
                    Fixed::ZERO,
                    Fixed::from_raw(resolution.velocity_z_fp),
                )),
            ));
        }
        AiControllerAction::Patrol | AiControllerAction::Idle => {}
    }
    let order_fingerprints = orders[start..]
        .iter()
        .map(|order| format!("{:016X}", order.fingerprint()))
        .collect::<Vec<_>>();
    Ok(AiControllerDecision {
        agent_id: pending.input.id,
        source_id: pending.input.source_id,
        body_id: pending.input.body_id,
        action: pending.action,
        selected_binding: pending.selected_binding,
        target_enemy_id: pending
            .target
            .as_ref()
            .map(|target| target.enemy_id.clone()),
        target_body_id: pending.target.as_ref().map(|target| target.body_id),
        path_fnv1a64: pending.path.as_ref().map(|path| path.fnv1a64.clone()),
        avoidance_fnv1a64: avoidance.map(|resolution| resolution.state_fnv1a64.clone()),
        blackboard_fnv1a64: pending.blackboard_fnv1a64,
        behavior_tick: pending.behavior_tick,
        order_count: order_fingerprints.len(),
        order_fingerprints,
    })
}

fn controller_leaf_state(
    base: &BehaviorLeafState,
    bindings: &AiControllerActionBindings,
    can_attack: bool,
    can_chase: bool,
) -> Result<BehaviorLeafState, EngineError> {
    let mut leaf_state = base.clone();
    leaf_state.actions.insert(
        bindings.attack.clone(),
        if can_attack {
            BehaviorStatus::Success
        } else {
            BehaviorStatus::Failure
        },
    );
    leaf_state.actions.insert(
        bindings.chase.clone(),
        if can_chase && !can_attack {
            BehaviorStatus::Running
        } else {
            BehaviorStatus::Failure
        },
    );
    leaf_state.actions.insert(
        bindings.patrol.clone(),
        if can_chase {
            BehaviorStatus::Failure
        } else {
            BehaviorStatus::Running
        },
    );
    Ok(leaf_state)
}

fn selected_action_binding(
    tree: &BehaviorTree,
    tick: &BehaviorTickReport,
) -> Result<Option<String>, EngineError> {
    let bindings = tree
        .nodes
        .iter()
        .filter_map(|node| {
            node.binding
                .as_ref()
                .map(|binding| (node.id.as_str(), binding))
        })
        .collect::<BTreeMap<_, _>>();
    for entry in tick.trace.iter().rev() {
        if entry.node_kind != "action"
            || !matches!(
                entry.status,
                BehaviorStatus::Success | BehaviorStatus::Running
            )
        {
            continue;
        }
        return bindings
            .get(entry.node_id.as_str())
            .map(|binding| Some((*binding).clone()))
            .ok_or_else(|| EngineError::AiInvalidConfig {
                field: format!("controller.behavior_tree.{}.binding", entry.node_id),
                reason: "selected action node must have a binding".to_string(),
            });
    }
    Ok(None)
}

fn action_from_binding(
    binding: Option<&str>,
    bindings: &AiControllerActionBindings,
    tick: &BehaviorTickReport,
) -> Result<AiControllerAction, EngineError> {
    let Some(binding) = binding else {
        return if tick.root_status == BehaviorStatus::Failure {
            Ok(AiControllerAction::Idle)
        } else {
            invalid_config(
                "controller.behavior_tick.action",
                "successful behavior tick must select an action",
            )
        };
    };
    if binding == bindings.attack {
        Ok(AiControllerAction::Attack)
    } else if binding == bindings.chase {
        Ok(AiControllerAction::Chase)
    } else if binding == bindings.patrol {
        Ok(AiControllerAction::Patrol)
    } else {
        invalid_config(
            "controller.behavior_tick.action",
            format!("selected action binding '{binding}' is not declared"),
        )
    }
}

fn nearest_attackable(agent: &AgentPerception) -> Option<ControllerTarget> {
    nearest_enemy(agent, |enemy| {
        enemy.in_attack_range && enemy.has_line_of_sight
    })
}

fn nearest_visible(agent: &AgentPerception) -> Option<ControllerTarget> {
    nearest_enemy(agent, |enemy| {
        enemy.in_sight_range && enemy.has_line_of_sight
    })
}

fn nearest_enemy(
    agent: &AgentPerception,
    predicate: impl Fn(&PerceivedEnemy) -> bool,
) -> Option<ControllerTarget> {
    agent
        .enemies
        .iter()
        .find(|enemy| predicate(enemy))
        .map(|enemy| ControllerTarget {
            enemy_id: enemy.id.clone(),
            body_id: enemy.body_id,
            nav_cell: enemy.nav_cell,
        })
}

fn require_target(
    agent_id: &str,
    action: &str,
    target: Option<ControllerTarget>,
) -> Result<Option<ControllerTarget>, EngineError> {
    target
        .map(Some)
        .ok_or_else(|| EngineError::AiInvalidConfig {
            field: format!("controller.agents.{agent_id}.{action}_target"),
            reason: "selected action has no valid sensed target".to_string(),
        })
}

fn avoidance_resolution<'a>(
    avoidance: Option<&'a NavAvoidanceResolution>,
    agent_id: &str,
) -> Result<&'a crate::nav::NavAvoidanceAgentResolution, EngineError> {
    let resolution = avoidance.ok_or_else(|| EngineError::AiInvalidConfig {
        field: format!("controller.agents.{agent_id}.avoidance"),
        reason: "chase action requires local avoidance resolution".to_string(),
    })?;
    resolution
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| EngineError::AiInvalidConfig {
            field: format!("controller.agents.{agent_id}.avoidance"),
            reason: "local avoidance result missing controller agent".to_string(),
        })
}

fn perception_index(report: &PerceptionRunReport) -> BTreeMap<&str, &AgentPerception> {
    report
        .agents
        .iter()
        .map(|agent| (agent.agent_id.as_str(), agent))
        .collect()
}

fn sorted_inputs(agents: &[AiControllerAgentInput]) -> Vec<&AiControllerAgentInput> {
    let mut sorted = agents.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.id.cmp(&right.id));
    sorted
}
