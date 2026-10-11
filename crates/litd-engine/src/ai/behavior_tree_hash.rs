use litd_hash::Fnv1a64;

use super::{
    BehaviorDecorator, BehaviorNodeKind, BehaviorRunReport, BehaviorStatus, BehaviorTickReport,
    BehaviorTree,
};

pub fn compute_behavior_tree_fnv1a64(tree: &BehaviorTree) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &tree.schema);
    update_str(&mut hasher, &tree.root);
    let mut nodes = tree.nodes.iter().collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    hasher.update_u64_le(nodes.len() as u64);
    for node in nodes {
        update_str(&mut hasher, &node.id);
        hash_kind(&mut hasher, &node.kind);
        hasher.update_u64_le(node.children.len() as u64);
        for child in &node.children {
            update_str(&mut hasher, child);
        }
        match &node.binding {
            Some(binding) => {
                hasher.update_u8(1);
                update_str(&mut hasher, binding);
            }
            None => {
                hasher.update_u8(0);
            }
        }
    }
    hasher.finish_hex()
}

pub fn compute_behavior_tick_fnv1a64(report: &BehaviorTickReport) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &report.schema);
    hasher.update_u64_le(report.tick);
    update_str(&mut hasher, &report.tree_fnv1a64);
    hash_status(&mut hasher, report.root_status);
    hasher.update_u64_le(report.trace.len() as u64);
    for entry in &report.trace {
        hasher.update_u64_le(entry.evaluation_index as u64);
        hasher.update_u64_le(entry.tick);
        hasher.update_u64_le(entry.depth as u64);
        update_str(&mut hasher, &entry.node_id);
        update_str(&mut hasher, &entry.node_kind);
        hash_status(&mut hasher, entry.status);
    }
    hasher.finish_hex()
}

pub fn compute_behavior_run_fnv1a64(report: &BehaviorRunReport) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &report.schema);
    update_str(&mut hasher, &report.tree_fnv1a64);
    hasher.update_u64_le(report.ticks.len() as u64);
    for tick in &report.ticks {
        update_str(&mut hasher, &tick.fnv1a64);
    }
    hasher.finish_hex()
}

fn hash_kind(hasher: &mut Fnv1a64, kind: &BehaviorNodeKind) {
    update_str(hasher, kind_label(kind));
    match kind {
        BehaviorNodeKind::Parallel {
            success_threshold,
            failure_threshold,
        } => {
            hasher.update_u64_le(*success_threshold as u64);
            hasher.update_u64_le(*failure_threshold as u64);
        }
        BehaviorNodeKind::Decorator { decorator } => hash_decorator(hasher, *decorator),
        _ => {}
    }
}

fn hash_decorator(hasher: &mut Fnv1a64, decorator: BehaviorDecorator) {
    hasher.update_u8(match decorator {
        BehaviorDecorator::Inverter => 1,
        BehaviorDecorator::ForceSuccess => 2,
        BehaviorDecorator::ForceFailure => 3,
    });
}

fn hash_status(hasher: &mut Fnv1a64, status: BehaviorStatus) {
    hasher.update_u8(match status {
        BehaviorStatus::Success => 1,
        BehaviorStatus::Failure => 2,
        BehaviorStatus::Running => 3,
    });
}

fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

pub(super) fn kind_label(kind: &BehaviorNodeKind) -> &'static str {
    match kind {
        BehaviorNodeKind::Sequence => "sequence",
        BehaviorNodeKind::Selector => "selector",
        BehaviorNodeKind::Parallel { .. } => "parallel",
        BehaviorNodeKind::Decorator { .. } => "decorator",
        BehaviorNodeKind::Action => "action",
        BehaviorNodeKind::Condition => "condition",
    }
}
