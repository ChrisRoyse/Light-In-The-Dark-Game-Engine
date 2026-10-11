use std::collections::BTreeSet;

use calyx_core::CxId;
use calyx_paths::{AssocGraph, PathsError, reach, reach_scored};
use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

#[path = "world_graph_navigation_store.rs"]
mod world_graph_navigation_store;
#[path = "world_graph_navigation_support.rs"]
mod world_graph_navigation_support;

pub use world_graph_navigation_store::{
    WorldGraphNavigationWriteback, parse_world_graph_navigation_ledger,
    read_world_graph_navigation_ledger, write_world_graph_navigation_ledger,
};
use world_graph_navigation_support::{
    build_assoc_graph, invalid_navigation, path_edges, scored_path, stable_node_cx_id,
    validate_graph, validate_query, validate_report,
};

pub const WORLD_GRAPH_NAVIGATION_SCHEMA: &str = "litd.world_graph_navigation.ledger.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldGraphTraversalMode {
    FewestHops,
    HighestScore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldGraphTraversalStatus {
    Reached,
    Unreachable,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldKnowledgeNode {
    pub node_id: String,
    pub cx_id: CxId,
    pub kind: String,
    pub title: String,
    pub frequency_weight: f32,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldKnowledgeEdge {
    pub edge_id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub relation: String,
    pub weight: f32,
    pub provenance_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldKnowledgeGraph {
    pub schema: String,
    pub graph_id: String,
    pub source_of_truth: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes: Vec<WorldKnowledgeNode>,
    pub edges: Vec<WorldKnowledgeEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldGraphTraverseQuery {
    pub query_id: String,
    pub start_node_id: String,
    pub target_node_id: String,
    pub max_hops: usize,
    pub mode: WorldGraphTraversalMode,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldGraphPathNode {
    pub node_id: String,
    pub cx_id: CxId,
    pub kind: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldGraphPathEdge {
    pub edge_id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub relation: String,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldGraphTraversalReport {
    pub schema: String,
    pub query_id: String,
    pub graph_id: String,
    pub status: WorldGraphTraversalStatus,
    pub code: String,
    pub reasons: Vec<String>,
    pub start_node_id: String,
    pub target_node_id: String,
    pub max_hops: usize,
    pub mode: WorldGraphTraversalMode,
    pub requested_by: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub visited_count: usize,
    pub hop_count: Option<usize>,
    pub score_milli: Option<u32>,
    pub path: Vec<WorldGraphPathNode>,
    pub edges: Vec<WorldGraphPathEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldGraphNavigationLedger {
    pub schema: String,
    pub ledger_id: String,
    pub graph: WorldKnowledgeGraph,
    pub traversal_count: usize,
    pub traversals: Vec<WorldGraphTraversalReport>,
}

pub fn world_knowledge_node(
    graph_id: &str,
    node_id: &str,
    kind: &str,
    title: &str,
    frequency_weight: f32,
    provenance_refs: Vec<String>,
) -> WorldKnowledgeNode {
    WorldKnowledgeNode {
        node_id: node_id.to_string(),
        cx_id: stable_node_cx_id(graph_id, node_id),
        kind: kind.to_string(),
        title: title.to_string(),
        frequency_weight,
        provenance_refs,
    }
}

pub fn world_knowledge_edge(
    from_node_id: &str,
    to_node_id: &str,
    relation: &str,
    weight: f32,
    provenance_refs: Vec<String>,
) -> WorldKnowledgeEdge {
    WorldKnowledgeEdge {
        edge_id: format!("{from_node_id}->{to_node_id}:{relation}"),
        from_node_id: from_node_id.to_string(),
        to_node_id: to_node_id.to_string(),
        relation: relation.to_string(),
        weight,
        provenance_refs,
    }
}

pub fn build_world_knowledge_graph(
    graph_id: impl Into<String>,
    source_of_truth: impl Into<String>,
    nodes: Vec<WorldKnowledgeNode>,
    edges: Vec<WorldKnowledgeEdge>,
) -> Result<WorldKnowledgeGraph, EngineError> {
    let graph = WorldKnowledgeGraph {
        schema: WORLD_GRAPH_NAVIGATION_SCHEMA.to_string(),
        graph_id: graph_id.into(),
        source_of_truth: source_of_truth.into(),
        node_count: nodes.len(),
        edge_count: edges.len(),
        nodes,
        edges,
    };
    validate_graph(&graph)?;
    Ok(graph)
}

pub fn traverse_world_graph(
    graph: &WorldKnowledgeGraph,
    query: &WorldGraphTraverseQuery,
) -> Result<WorldGraphTraversalReport, EngineError> {
    validate_graph(graph)?;
    validate_query(query)?;
    let assoc = build_assoc_graph(graph)?;
    let start = graph.node_cx_id(&query.start_node_id)?;
    let target = graph.node_cx_id(&query.target_node_id)?;
    let report = match query.mode {
        WorldGraphTraversalMode::FewestHops => {
            fewest_hops_report(graph, query, &assoc, start, target)?
        }
        WorldGraphTraversalMode::HighestScore => {
            highest_score_report(graph, query, &assoc, start, target)?
        }
    };
    validate_report(&report)?;
    Ok(report)
}

pub fn build_world_graph_navigation_ledger(
    ledger_id: impl Into<String>,
    graph: WorldKnowledgeGraph,
    traversals: Vec<WorldGraphTraversalReport>,
) -> Result<WorldGraphNavigationLedger, EngineError> {
    validate_graph(&graph)?;
    if traversals.is_empty() {
        return invalid_navigation(
            "ledger.traversals",
            "at least one traversal report is required",
        );
    }
    let ledger = WorldGraphNavigationLedger {
        schema: WORLD_GRAPH_NAVIGATION_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        traversal_count: traversals.len(),
        graph,
        traversals,
    };
    validate_world_graph_navigation_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_world_graph_navigation_ledger(
    ledger: &WorldGraphNavigationLedger,
) -> Result<(), EngineError> {
    if ledger.schema != WORLD_GRAPH_NAVIGATION_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.traversal_count == 0
        || ledger.traversal_count != ledger.traversals.len()
    {
        return invalid_navigation("ledger", "schema, id, and traversal count must be valid");
    }
    validate_graph(&ledger.graph)?;
    let mut seen = BTreeSet::new();
    for report in &ledger.traversals {
        validate_report(report)?;
        if report.graph_id != ledger.graph.graph_id {
            return invalid_navigation("ledger.graph_id", "traversal graph id mismatch");
        }
        if !seen.insert(report.query_id.clone()) {
            return invalid_navigation("ledger.duplicate", "duplicate traversal query id");
        }
    }
    Ok(())
}

fn fewest_hops_report(
    graph: &WorldKnowledgeGraph,
    query: &WorldGraphTraverseQuery,
    assoc: &AssocGraph,
    start: CxId,
    target: CxId,
) -> Result<WorldGraphTraversalReport, EngineError> {
    match reach(assoc, start, target, query.max_hops) {
        Ok(Some(path)) => reached_report(graph, query, path, None, assoc.node_count()),
        Ok(None) => Ok(empty_report(
            graph,
            query,
            WorldGraphTraversalStatus::Unreachable,
            "LITD-GRAPH-UNREACHABLE",
            "target is not reachable from start in the directed graph",
            assoc.node_count(),
        )),
        Err(PathsError::MaxHops { required, max_hops }) => Ok(empty_report(
            graph,
            query,
            WorldGraphTraversalStatus::Refused,
            "LITD-GRAPH-HOP-CAP-EXCEEDED",
            format!("path requires {required} hops but max_hops={max_hops}"),
            assoc.node_count(),
        )),
        Err(error) => Err(path_error("fewest_hops", error)),
    }
}

fn highest_score_report(
    graph: &WorldKnowledgeGraph,
    query: &WorldGraphTraverseQuery,
    assoc: &AssocGraph,
    start: CxId,
    target: CxId,
) -> Result<WorldGraphTraversalReport, EngineError> {
    let scored = reach_scored(assoc, start, query.max_hops)
        .map_err(|error| path_error("highest_score.reach_scored", error))?;
    let Some((path, score)) = scored_path(graph, query, target)? else {
        return capped_or_unreachable_report(graph, query, assoc, start, target);
    };
    let calyx_score = scored
        .iter()
        .find_map(|(id, value)| (*id == target).then_some(*value))
        .ok_or_else(|| navigation_error("highest_score.target", "reach_scored omitted target"))?;
    if (calyx_score - score).abs() > 0.0001 {
        return Err(navigation_error(
            "highest_score.score",
            format!("local path score {score} differed from calyx reach_scored {calyx_score}"),
        ));
    }
    reached_report(
        graph,
        query,
        path,
        Some(score_to_milli(score)),
        scored.len(),
    )
}

fn capped_or_unreachable_report(
    graph: &WorldKnowledgeGraph,
    query: &WorldGraphTraverseQuery,
    assoc: &AssocGraph,
    start: CxId,
    target: CxId,
) -> Result<WorldGraphTraversalReport, EngineError> {
    match reach(assoc, start, target, graph.node_count) {
        Ok(Some(path)) if path.len().saturating_sub(1) > query.max_hops => Ok(empty_report(
            graph,
            query,
            WorldGraphTraversalStatus::Refused,
            "LITD-GRAPH-HOP-CAP-EXCEEDED",
            format!(
                "shortest path requires {} hops but max_hops={}",
                path.len().saturating_sub(1),
                query.max_hops
            ),
            assoc.node_count(),
        )),
        Ok(_) => Ok(empty_report(
            graph,
            query,
            WorldGraphTraversalStatus::Unreachable,
            "LITD-GRAPH-UNREACHABLE",
            "target is not reachable within the weighted traversal horizon",
            assoc.node_count(),
        )),
        Err(error) => Err(path_error("highest_score.reach", error)),
    }
}

fn reached_report(
    graph: &WorldKnowledgeGraph,
    query: &WorldGraphTraverseQuery,
    path: Vec<CxId>,
    score_milli: Option<u32>,
    visited_count: usize,
) -> Result<WorldGraphTraversalReport, EngineError> {
    let path_nodes = path
        .iter()
        .map(|cx_id| graph.path_node(*cx_id))
        .collect::<Result<Vec<_>, _>>()?;
    let edges = path_edges(graph, &path_nodes)?;
    Ok(WorldGraphTraversalReport {
        schema: WORLD_GRAPH_NAVIGATION_SCHEMA.to_string(),
        query_id: query.query_id.clone(),
        graph_id: graph.graph_id.clone(),
        status: WorldGraphTraversalStatus::Reached,
        code: "LITD-GRAPH-REACHED".to_string(),
        reasons: vec!["target reached by requested graph traversal mode".to_string()],
        start_node_id: query.start_node_id.clone(),
        target_node_id: query.target_node_id.clone(),
        max_hops: query.max_hops,
        mode: query.mode,
        requested_by: query.requested_by.clone(),
        node_count: graph.node_count,
        edge_count: graph.edge_count,
        visited_count,
        hop_count: Some(path_nodes.len().saturating_sub(1)),
        score_milli,
        path: path_nodes,
        edges,
    })
}

fn empty_report(
    graph: &WorldKnowledgeGraph,
    query: &WorldGraphTraverseQuery,
    status: WorldGraphTraversalStatus,
    code: &str,
    reason: impl Into<String>,
    visited_count: usize,
) -> WorldGraphTraversalReport {
    WorldGraphTraversalReport {
        schema: WORLD_GRAPH_NAVIGATION_SCHEMA.to_string(),
        query_id: query.query_id.clone(),
        graph_id: graph.graph_id.clone(),
        status,
        code: code.to_string(),
        reasons: vec![reason.into()],
        start_node_id: query.start_node_id.clone(),
        target_node_id: query.target_node_id.clone(),
        max_hops: query.max_hops,
        mode: query.mode,
        requested_by: query.requested_by.clone(),
        node_count: graph.node_count,
        edge_count: graph.edge_count,
        visited_count,
        hop_count: None,
        score_milli: None,
        path: Vec::new(),
        edges: Vec::new(),
    }
}

impl WorldKnowledgeGraph {
    fn node_cx_id(&self, node_id: &str) -> Result<CxId, EngineError> {
        self.nodes
            .iter()
            .find_map(|node| (node.node_id == node_id).then_some(node.cx_id))
            .ok_or_else(|| navigation_error("query.node", format!("unknown node id {node_id}")))
    }

    fn path_node(&self, cx_id: CxId) -> Result<WorldGraphPathNode, EngineError> {
        let node = self
            .nodes
            .iter()
            .find(|node| node.cx_id == cx_id)
            .ok_or_else(|| navigation_error("path.node", format!("unknown cx id {cx_id}")))?;
        Ok(WorldGraphPathNode {
            node_id: node.node_id.clone(),
            cx_id,
            kind: node.kind.clone(),
            title: node.title.clone(),
        })
    }
}

fn score_to_milli(score: f32) -> u32 {
    ((score.clamp(0.0, 1.0) * 1000.0).round() as u32).min(1000)
}

fn path_error(field: &str, error: PathsError) -> EngineError {
    navigation_error(field, format!("{}: {error}", error.code()))
}

fn navigation_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_graph_navigation.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "world_graph_navigation_tests.rs"]
mod tests;
