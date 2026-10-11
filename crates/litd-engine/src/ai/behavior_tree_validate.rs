use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::{BEHAVIOR_TREE_SCHEMA, BehaviorNode, BehaviorNodeKind, BehaviorTree};

pub fn validate_tree(tree: &BehaviorTree) -> Result<BTreeMap<&str, &BehaviorNode>, EngineError> {
    if tree.schema != BEHAVIOR_TREE_SCHEMA {
        return invalid_config("tree.schema", "unsupported behavior-tree schema");
    }
    if tree.root.trim().is_empty() {
        return invalid_config("tree.root", "root id must be non-empty");
    }
    if tree.nodes.is_empty() {
        return invalid_config("tree.nodes", "tree must contain at least one node");
    }
    let mut index = BTreeMap::new();
    for node in &tree.nodes {
        validate_node_shape(node)?;
        if index.insert(node.id.as_str(), node).is_some() {
            return invalid_config("tree.nodes.id", format!("duplicate node id '{}'", node.id));
        }
    }
    if !index.contains_key(tree.root.as_str()) {
        return invalid_config("tree.root", "root id must reference an existing node");
    }
    for node in &tree.nodes {
        for child in &node.children {
            if !index.contains_key(child.as_str()) {
                return invalid_config(
                    format!("node.{}.children", node.id),
                    format!("child '{child}' does not exist"),
                );
            }
        }
    }
    detect_cycle(
        tree.root.as_str(),
        &index,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )?;
    Ok(index)
}

fn validate_node_shape(node: &BehaviorNode) -> Result<(), EngineError> {
    if node.id.trim().is_empty() {
        return invalid_config("node.id", "node id must be non-empty");
    }
    match node.kind {
        BehaviorNodeKind::Sequence | BehaviorNodeKind::Selector => {
            validate_composite(node, "composite")?;
        }
        BehaviorNodeKind::Parallel {
            success_threshold,
            failure_threshold,
        } => {
            validate_parallel(node, success_threshold, failure_threshold)?;
        }
        BehaviorNodeKind::Decorator { .. } => {
            if node.children.len() != 1 {
                return invalid_config(
                    format!("node.{}.children", node.id),
                    "decorator nodes require exactly one child",
                );
            }
            if node.binding.is_some() {
                return invalid_config(
                    format!("node.{}.binding", node.id),
                    "decorator nodes must not bind leaf state",
                );
            }
        }
        BehaviorNodeKind::Action | BehaviorNodeKind::Condition => {
            if !node.children.is_empty() {
                return invalid_config(
                    format!("node.{}.children", node.id),
                    "leaf nodes must not have children",
                );
            }
            if !has_binding(node) {
                return invalid_config(
                    format!("node.{}.binding", node.id),
                    "leaf nodes require a non-empty binding",
                );
            }
        }
    }
    Ok(())
}

fn validate_composite(node: &BehaviorNode, label: &str) -> Result<(), EngineError> {
    if node.children.is_empty() {
        return invalid_config(
            format!("node.{}.children", node.id),
            format!("{label} nodes require at least one child"),
        );
    }
    if node.binding.is_some() {
        return invalid_config(
            format!("node.{}.binding", node.id),
            format!("{label} nodes must not bind leaf state"),
        );
    }
    Ok(())
}

fn validate_parallel(
    node: &BehaviorNode,
    success_threshold: usize,
    failure_threshold: usize,
) -> Result<(), EngineError> {
    validate_composite(node, "parallel")?;
    let child_count = node.children.len();
    if success_threshold == 0 || success_threshold > child_count {
        return invalid_config(
            format!("node.{}.success_threshold", node.id),
            "success threshold must be within 1..=child_count",
        );
    }
    if failure_threshold == 0 || failure_threshold > child_count {
        return invalid_config(
            format!("node.{}.failure_threshold", node.id),
            "failure threshold must be within 1..=child_count",
        );
    }
    Ok(())
}

fn detect_cycle<'a>(
    id: &'a str,
    index: &BTreeMap<&'a str, &'a BehaviorNode>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<(), EngineError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return invalid_config("tree.nodes", format!("cycle detected at node '{id}'"));
    }
    for child in &index[id].children {
        detect_cycle(child.as_str(), index, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

fn has_binding(node: &BehaviorNode) -> bool {
    node.binding
        .as_deref()
        .is_some_and(|binding| !binding.trim().is_empty())
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
