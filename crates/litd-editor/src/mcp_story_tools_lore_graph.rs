use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::Serialize;

use crate::world_graph_navigation::{
    WorldGraphTraverseQuery, WorldKnowledgeGraph, build_world_knowledge_graph,
    world_knowledge_edge, world_knowledge_node,
};

use super::{invalid_mcp_story, mcp_story_tools_store::McpStoryState};

const RECIPROCAL_EDGE_WEIGHT: f32 = 0.8;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct LoreRelationshipGraphBuild {
    pub graph: WorldKnowledgeGraph,
    pub summary: LoreRelationshipGraphSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct LoreRelationshipGraphSummary {
    pub graph_id: String,
    pub state_id: String,
    pub source_of_truth: String,
    pub node_count: usize,
    pub direct_edge_count: usize,
    pub reciprocal_edge_count: usize,
    pub edge_count: usize,
    pub kind_counts: BTreeMap<String, usize>,
    pub relation_counts: BTreeMap<String, usize>,
    pub isolated_nodes: Vec<String>,
}

pub(super) fn build_lore_relationship_graph(
    graph_id: &str,
    source_of_truth: String,
    state: &McpStoryState,
) -> Result<LoreRelationshipGraphBuild, EngineError> {
    if state.lore_entries.is_empty() {
        return Err(invalid_mcp_story(
            "lore_relationship_graph.entries",
            "at least one world-bible lore entry is required",
        ));
    }
    let entry_ids = state
        .lore_entries
        .iter()
        .map(|entry| entry.entry_id.as_str())
        .collect::<BTreeSet<_>>();
    let nodes = state
        .lore_entries
        .iter()
        .map(|entry| {
            world_knowledge_node(
                graph_id,
                &entry.entry_id,
                entry.kind.as_str(),
                &entry.title,
                1.0,
                node_provenance(entry),
            )
        })
        .collect::<Vec<_>>();
    let mut direct_edge_count = 0;
    let mut reciprocal_edge_count = 0;
    let mut edges = Vec::new();
    for entry in &state.lore_entries {
        for link in &entry.links {
            if !entry_ids.contains(link.target_entry_id.as_str()) {
                return Err(invalid_mcp_story(
                    "lore_relationship_graph.links",
                    format!(
                        "{} links to missing lore entry {}",
                        entry.entry_id, link.target_entry_id
                    ),
                ));
            }
            edges.push(world_knowledge_edge(
                &entry.entry_id,
                &link.target_entry_id,
                &link.relation,
                1.0,
                edge_provenance(
                    &entry.entry_id,
                    &link.target_entry_id,
                    &link.relation,
                    false,
                ),
            ));
            direct_edge_count += 1;
            let inverse_relation = format!("inverse_{}", link.relation);
            edges.push(world_knowledge_edge(
                &link.target_entry_id,
                &entry.entry_id,
                &inverse_relation,
                RECIPROCAL_EDGE_WEIGHT,
                edge_provenance(&entry.entry_id, &link.target_entry_id, &link.relation, true),
            ));
            reciprocal_edge_count += 1;
        }
    }
    if direct_edge_count == 0 {
        return Err(invalid_mcp_story(
            "lore_relationship_graph.links",
            "at least one lore relationship link is required",
        ));
    }
    validate_unique_edges(&edges)?;
    let graph = build_world_knowledge_graph(graph_id, source_of_truth.clone(), nodes, edges)?;
    let isolated_nodes = isolated_nodes(&graph);
    if !isolated_nodes.is_empty() {
        return Err(invalid_mcp_story(
            "lore_relationship_graph.isolated_nodes",
            format!("every lore entry must be connected; isolated={isolated_nodes:?}"),
        ));
    }
    let summary = LoreRelationshipGraphSummary {
        graph_id: graph.graph_id.clone(),
        state_id: state.state_id.clone(),
        source_of_truth,
        node_count: graph.node_count,
        direct_edge_count,
        reciprocal_edge_count,
        edge_count: graph.edge_count,
        kind_counts: kind_counts(state),
        relation_counts: relation_counts(&graph),
        isolated_nodes,
    };
    Ok(LoreRelationshipGraphBuild { graph, summary })
}

pub(super) fn validate_lore_relationship_graph_queries(
    queries: &[WorldGraphTraverseQuery],
    requested_by: &str,
) -> Result<(), EngineError> {
    if queries.is_empty() || queries.len() > 64 {
        return Err(invalid_mcp_story(
            "lore_relationship_graph.queries",
            "traversal query count must be in 1..=64",
        ));
    }
    let mut query_ids = BTreeSet::new();
    for query in queries {
        if query.requested_by != requested_by {
            return Err(invalid_mcp_story(
                "lore_relationship_graph.queries.requested_by",
                "query requester must match the tool requester",
            ));
        }
        if !query_ids.insert(query.query_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_relationship_graph.queries.query_id",
                format!("duplicate traversal query {}", query.query_id),
            ));
        }
    }
    Ok(())
}

fn node_provenance(entry: &super::LoreEntry) -> Vec<String> {
    let mut provenance = vec![format!("lore://entry/{}", entry.entry_id)];
    provenance.extend(entry.source_refs.iter().cloned());
    provenance
}

fn edge_provenance(source: &str, target: &str, relation: &str, reciprocal: bool) -> Vec<String> {
    let suffix = if reciprocal { "#reciprocal" } else { "" };
    vec![format!(
        "lore://entry/{source}/link/{target}/{relation}{suffix}"
    )]
}

fn validate_unique_edges(
    edges: &[crate::world_graph_navigation::WorldKnowledgeEdge],
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for edge in edges {
        if !seen.insert(edge.edge_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_relationship_graph.edge_id",
                format!("duplicate generated relation edge {}", edge.edge_id),
            ));
        }
    }
    Ok(())
}

fn isolated_nodes(graph: &WorldKnowledgeGraph) -> Vec<String> {
    graph
        .nodes
        .iter()
        .filter(|node| {
            !graph
                .edges
                .iter()
                .any(|edge| edge.from_node_id == node.node_id || edge.to_node_id == node.node_id)
        })
        .map(|node| node.node_id.clone())
        .collect()
}

fn kind_counts(state: &McpStoryState) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in &state.lore_entries {
        *counts.entry(entry.kind.as_str().to_string()).or_insert(0) += 1;
    }
    counts
}

fn relation_counts(graph: &WorldKnowledgeGraph) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for edge in &graph.edges {
        *counts.entry(edge.relation.clone()).or_insert(0) += 1;
    }
    counts
}
