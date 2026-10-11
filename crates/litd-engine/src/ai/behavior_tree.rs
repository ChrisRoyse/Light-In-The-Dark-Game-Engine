use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::{
    behavior_tree_hash::{
        compute_behavior_run_fnv1a64, compute_behavior_tick_fnv1a64, compute_behavior_tree_fnv1a64,
        kind_label,
    },
    behavior_tree_validate::validate_tree,
};

pub const BEHAVIOR_TREE_SCHEMA: &str = "litd.ai.behavior_tree.v1";
pub const BEHAVIOR_TREE_TICK_SCHEMA: &str = "litd.ai.behavior_tree.tick.v1";
pub const BEHAVIOR_TREE_RUN_SCHEMA: &str = "litd.ai.behavior_tree.run.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BehaviorStatus {
    Success,
    Failure,
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BehaviorDecorator {
    Inverter,
    ForceSuccess,
    ForceFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BehaviorNodeKind {
    Sequence,
    Selector,
    Parallel {
        success_threshold: usize,
        failure_threshold: usize,
    },
    Decorator {
        decorator: BehaviorDecorator,
    },
    Action,
    Condition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorNode {
    pub id: String,
    pub kind: BehaviorNodeKind,
    pub children: Vec<String>,
    pub binding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorTree {
    pub schema: String,
    pub root: String,
    pub nodes: Vec<BehaviorNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorLeafState {
    pub conditions: BTreeMap<String, BehaviorStatus>,
    pub actions: BTreeMap<String, BehaviorStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorTraceEntry {
    pub evaluation_index: usize,
    pub tick: u64,
    pub depth: usize,
    pub node_id: String,
    pub node_kind: String,
    pub status: BehaviorStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorTickReport {
    pub schema: String,
    pub tick: u64,
    pub tree_fnv1a64: String,
    pub root_status: BehaviorStatus,
    pub trace: Vec<BehaviorTraceEntry>,
    pub fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorRunReport {
    pub schema: String,
    pub tree_fnv1a64: String,
    pub ticks: Vec<BehaviorTickReport>,
    pub fnv1a64: String,
}

pub fn tick_behavior_tree(
    tree: &BehaviorTree,
    leaf_state: &BehaviorLeafState,
    tick: u64,
) -> Result<BehaviorTickReport, EngineError> {
    let index = validate_tree(tree)?;
    let tree_fnv1a64 = compute_behavior_tree_fnv1a64(tree);
    let mut trace = Vec::new();
    let root_status = evaluate_node(tree.root.as_str(), &index, leaf_state, tick, 0, &mut trace)?;
    let mut report = BehaviorTickReport {
        schema: BEHAVIOR_TREE_TICK_SCHEMA.to_string(),
        tick,
        tree_fnv1a64,
        root_status,
        trace,
        fnv1a64: String::new(),
    };
    report.fnv1a64 = compute_behavior_tick_fnv1a64(&report);
    verify_behavior_tick_integrity(&report)?;
    Ok(report)
}

pub fn verify_behavior_tick_integrity(report: &BehaviorTickReport) -> Result<(), EngineError> {
    if report.schema != BEHAVIOR_TREE_TICK_SCHEMA {
        return invalid_config("tick.schema", "unsupported behavior-tree tick schema");
    }
    if report.tree_fnv1a64.is_empty() {
        return invalid_config("tick.tree_fnv1a64", "tree hash must be non-empty");
    }
    if report.trace.is_empty() {
        return invalid_config("tick.trace", "trace must contain at least the root node");
    }
    for (index, entry) in report.trace.iter().enumerate() {
        if entry.evaluation_index != index {
            return invalid_config("tick.trace", "evaluation_index must match trace order");
        }
        if entry.tick != report.tick {
            return invalid_config("tick.trace.tick", "trace tick must match report tick");
        }
        if entry.node_id.trim().is_empty() || entry.node_kind.trim().is_empty() {
            return invalid_config(
                "tick.trace.node",
                "trace node id and kind must be non-empty",
            );
        }
    }
    let actual = compute_behavior_tick_fnv1a64(report);
    if report.fnv1a64 != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "behavior_tick.fnv1a64".to_string(),
            expected: report.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn verify_behavior_run_integrity(report: &BehaviorRunReport) -> Result<(), EngineError> {
    if report.schema != BEHAVIOR_TREE_RUN_SCHEMA {
        return invalid_config("run.schema", "unsupported behavior-tree run schema");
    }
    if report.tree_fnv1a64.is_empty() {
        return invalid_config("run.tree_fnv1a64", "tree hash must be non-empty");
    }
    if report.ticks.is_empty() {
        return invalid_config("run.ticks", "run must contain at least one tick");
    }
    for tick in &report.ticks {
        verify_behavior_tick_integrity(tick)?;
        if tick.tree_fnv1a64 != report.tree_fnv1a64 {
            return invalid_config(
                "run.ticks.tree_fnv1a64",
                "all ticks must use the run tree hash",
            );
        }
    }
    let actual = compute_behavior_run_fnv1a64(report);
    if report.fnv1a64 != actual {
        return Err(EngineError::AiReadbackMismatch {
            field: "behavior_run.fnv1a64".to_string(),
            expected: report.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

fn evaluate_node(
    id: &str,
    index: &BTreeMap<&str, &BehaviorNode>,
    leaf_state: &BehaviorLeafState,
    tick: u64,
    depth: usize,
    trace: &mut Vec<BehaviorTraceEntry>,
) -> Result<BehaviorStatus, EngineError> {
    let Some(node) = index.get(id) else {
        return invalid_config("tree.node", format!("node '{id}' does not exist"));
    };
    let trace_index = trace.len();
    trace.push(BehaviorTraceEntry {
        evaluation_index: trace_index,
        tick,
        depth,
        node_id: node.id.clone(),
        node_kind: kind_label(&node.kind).to_string(),
        status: BehaviorStatus::Running,
    });
    let status = match node.kind {
        BehaviorNodeKind::Sequence => eval_sequence(node, index, leaf_state, tick, depth, trace)?,
        BehaviorNodeKind::Selector => eval_selector(node, index, leaf_state, tick, depth, trace)?,
        BehaviorNodeKind::Parallel {
            success_threshold,
            failure_threshold,
        } => eval_parallel(
            node,
            index,
            leaf_state,
            tick,
            depth,
            trace,
            success_threshold,
            failure_threshold,
        )?,
        BehaviorNodeKind::Decorator { decorator } => {
            let child =
                evaluate_node(&node.children[0], index, leaf_state, tick, depth + 1, trace)?;
            apply_decorator(decorator, child)
        }
        BehaviorNodeKind::Action => action_status(node, leaf_state)?,
        BehaviorNodeKind::Condition => condition_status(node, leaf_state)?,
    };
    trace[trace_index].status = status;
    Ok(status)
}

fn eval_sequence(
    node: &BehaviorNode,
    index: &BTreeMap<&str, &BehaviorNode>,
    leaf_state: &BehaviorLeafState,
    tick: u64,
    depth: usize,
    trace: &mut Vec<BehaviorTraceEntry>,
) -> Result<BehaviorStatus, EngineError> {
    for child in &node.children {
        let status = evaluate_node(child, index, leaf_state, tick, depth + 1, trace)?;
        if status != BehaviorStatus::Success {
            return Ok(status);
        }
    }
    Ok(BehaviorStatus::Success)
}

fn eval_selector(
    node: &BehaviorNode,
    index: &BTreeMap<&str, &BehaviorNode>,
    leaf_state: &BehaviorLeafState,
    tick: u64,
    depth: usize,
    trace: &mut Vec<BehaviorTraceEntry>,
) -> Result<BehaviorStatus, EngineError> {
    for child in &node.children {
        let status = evaluate_node(child, index, leaf_state, tick, depth + 1, trace)?;
        if status != BehaviorStatus::Failure {
            return Ok(status);
        }
    }
    Ok(BehaviorStatus::Failure)
}

#[allow(clippy::too_many_arguments)]
fn eval_parallel(
    node: &BehaviorNode,
    index: &BTreeMap<&str, &BehaviorNode>,
    leaf_state: &BehaviorLeafState,
    tick: u64,
    depth: usize,
    trace: &mut Vec<BehaviorTraceEntry>,
    success_threshold: usize,
    failure_threshold: usize,
) -> Result<BehaviorStatus, EngineError> {
    let mut successes = 0_usize;
    let mut failures = 0_usize;
    for child in &node.children {
        match evaluate_node(child, index, leaf_state, tick, depth + 1, trace)? {
            BehaviorStatus::Success => successes += 1,
            BehaviorStatus::Failure => failures += 1,
            BehaviorStatus::Running => {}
        }
    }
    if failures >= failure_threshold {
        Ok(BehaviorStatus::Failure)
    } else if successes >= success_threshold {
        Ok(BehaviorStatus::Success)
    } else {
        Ok(BehaviorStatus::Running)
    }
}

fn apply_decorator(decorator: BehaviorDecorator, status: BehaviorStatus) -> BehaviorStatus {
    match decorator {
        BehaviorDecorator::Inverter => match status {
            BehaviorStatus::Success => BehaviorStatus::Failure,
            BehaviorStatus::Failure => BehaviorStatus::Success,
            BehaviorStatus::Running => BehaviorStatus::Running,
        },
        BehaviorDecorator::ForceSuccess => match status {
            BehaviorStatus::Running => BehaviorStatus::Running,
            BehaviorStatus::Success | BehaviorStatus::Failure => BehaviorStatus::Success,
        },
        BehaviorDecorator::ForceFailure => match status {
            BehaviorStatus::Running => BehaviorStatus::Running,
            BehaviorStatus::Success | BehaviorStatus::Failure => BehaviorStatus::Failure,
        },
    }
}

fn action_status(
    node: &BehaviorNode,
    leaf_state: &BehaviorLeafState,
) -> Result<BehaviorStatus, EngineError> {
    let binding = binding(node)?;
    leaf_state
        .actions
        .get(binding)
        .copied()
        .ok_or_else(|| EngineError::AiInvalidConfig {
            field: format!("actions.{binding}"),
            reason: "action binding missing from leaf state".to_string(),
        })
}

fn condition_status(
    node: &BehaviorNode,
    leaf_state: &BehaviorLeafState,
) -> Result<BehaviorStatus, EngineError> {
    let binding = binding(node)?;
    let status = leaf_state.conditions.get(binding).copied().ok_or_else(|| {
        EngineError::AiInvalidConfig {
            field: format!("conditions.{binding}"),
            reason: "condition binding missing from leaf state".to_string(),
        }
    })?;
    if status == BehaviorStatus::Running {
        return Err(EngineError::AiInvalidConfig {
            field: format!("conditions.{binding}"),
            reason: "conditions must be pure boolean leaves and cannot return Running".to_string(),
        });
    }
    Ok(status)
}

fn binding(node: &BehaviorNode) -> Result<&str, EngineError> {
    node.binding
        .as_deref()
        .filter(|binding| !binding.trim().is_empty())
        .ok_or_else(|| EngineError::AiInvalidConfig {
            field: format!("node.{}.binding", node.id),
            reason: "binding must be non-empty".to_string(),
        })
}

fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::AiInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}
