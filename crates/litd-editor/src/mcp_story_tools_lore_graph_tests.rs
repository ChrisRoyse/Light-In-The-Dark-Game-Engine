use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::world_graph_navigation::{
    WorldGraphTraversalMode, WorldGraphTraversalStatus, WorldGraphTraverseQuery,
    parse_world_graph_navigation_ledger, read_world_graph_navigation_ledger,
};

use super::*;

#[test]
fn issue1050_lore_relationship_graph_builds_and_traverses_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1050_lore_relationship_graph", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    let seed_ledger = root.join("seed-ledger-aster");
    let graph_ledger_path = root.join("relationship-graph-aster");
    seed_entries(&state_path, &seed_ledger, sample_lore_entries());

    let output = call_mcp_story_tool(
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "graph_ledger_path": graph_ledger_path,
            "state_id": "issue1050-lore-state",
            "graph_id": "issue1050-lore-relationship-graph",
            "graph_ledger_id": "issue1050-lore-relationship-ledger",
            "request_id": "issue1050-lore-graph",
            "requested_by": "litd-editor-issue1050-test",
            "queries": [
                query("issue1050-forward", "cabal_sable_choir", "history_black_bell_year", 4, WorldGraphTraversalMode::FewestHops),
                query("issue1050-reciprocal", "place_ashen_reach", "cabal_sable_choir", 3, WorldGraphTraversalMode::FewestHops),
                query("issue1050-weighted", "artifact_cinder_astrolabe", "history_black_bell_year", 4, WorldGraphTraversalMode::HighestScore)
            ]
        }),
    )
    .unwrap();
    assert_eq!(output.tool_name, BUILD_LORE_RELATIONSHIP_GRAPH_TOOL);
    assert_eq!(output.result["relationship_graph"]["node_count"], 6);
    assert_eq!(output.result["relationship_graph"]["direct_edge_count"], 6);
    assert_eq!(
        output.result["relationship_graph"]["reciprocal_edge_count"],
        6
    );

    let writeback = read_world_graph_navigation_ledger(
        &graph_ledger_path,
        "issue1050-lore-relationship-ledger",
    )
    .unwrap()
    .unwrap();
    let ledger = parse_world_graph_navigation_ledger(&writeback.bytes).unwrap();
    assert_eq!(ledger.graph.node_count, 6);
    assert_eq!(ledger.graph.edge_count, 12);
    assert_eq!(ledger.traversal_count, 3);
    assert!(ledger.traversals.iter().all(|report| {
        report.status == WorldGraphTraversalStatus::Reached
            && report.graph_id == "issue1050-lore-relationship-graph"
    }));
    let reciprocal = ledger
        .traversals
        .iter()
        .find(|report| report.query_id == "issue1050-reciprocal")
        .unwrap();
    assert_eq!(
        path_ids(reciprocal),
        vec!["place_ashen_reach", "deity_vhael", "cabal_sable_choir"]
    );
    assert!(
        reciprocal
            .edges
            .iter()
            .all(|edge| edge.relation.starts_with("inverse_"))
    );
}

#[test]
fn issue1050_lore_relationship_graph_edges_fail_without_graph_write() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1050_lore_relationship_graph", "edges").unwrap();
    reset_root(&root);
    let missing_state = root.join("missing-story-state-aster");
    let missing_graph = root.join("missing-graph-aster");
    assert_graph_no_write(
        &missing_state,
        &missing_graph,
        vec![query(
            "issue1050-missing-state",
            "cabal_sable_choir",
            "place_ashen_reach",
            4,
            WorldGraphTraversalMode::FewestHops,
        )],
        "missing Calyx Aster story state row",
    );

    let state_path = root.join("story-state-aster");
    seed_entries(
        &state_path,
        &root.join("seed-ledger-aster"),
        sample_lore_entries(),
    );
    assert_graph_no_write(
        &state_path,
        &root.join("empty-query-graph-aster"),
        Vec::new(),
        "traversal query count",
    );
    assert_graph_no_write(
        &state_path,
        &root.join("unknown-node-graph-aster"),
        vec![query(
            "issue1050-unknown",
            "missing_entry",
            "place_ashen_reach",
            4,
            WorldGraphTraversalMode::FewestHops,
        )],
        "unknown node id",
    );

    let isolated_state = root.join("isolated-story-state-aster");
    seed_entries(
        &isolated_state,
        &root.join("isolated-seed-ledger-aster"),
        isolated_lore_entries(),
    );
    assert_graph_no_write(
        &isolated_state,
        &root.join("isolated-graph-aster"),
        vec![query(
            "issue1050-isolated",
            "cabal_sable_choir",
            "place_ashen_reach",
            4,
            WorldGraphTraversalMode::FewestHops,
        )],
        "isolated",
    );
}

fn seed_entries(state_path: &Path, ledger_path: &Path, entries: Value) {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1050-lore-state",
            "ledger_id": "issue1050-seed-ledger",
            "request_id": "issue1050-seed",
            "requested_by": "litd-editor-issue1050-test",
            "entries": entries
        }),
    )
    .unwrap();
}

fn assert_graph_no_write(
    state_path: &Path,
    graph_ledger_path: &Path,
    queries: Vec<WorldGraphTraverseQuery>,
    expected: &str,
) {
    let before = state_path
        .exists()
        .then(|| read_mcp_story_state(state_path).unwrap().sha256);
    let error = call_mcp_story_tool(
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "graph_ledger_path": graph_ledger_path,
            "state_id": "issue1050-lore-state",
            "graph_id": "issue1050-lore-relationship-graph",
            "graph_ledger_id": "issue1050-edge-ledger",
            "request_id": "issue1050-edge",
            "requested_by": "litd-editor-issue1050-test",
            "queries": queries
        }),
    )
    .unwrap_err();
    let after = state_path
        .exists()
        .then(|| read_mcp_story_state(state_path).unwrap().sha256);
    assert_eq!(before, after);
    assert!(!graph_ledger_path.exists());
    assert!(error.to_string().contains(expected), "{error}");
}

fn sample_lore_entries() -> Value {
    json!([
        entry(
            "deity_vhael",
            "deity",
            "Vhael",
            "Star-deity of ember oaths.",
            [link("place_ashen_reach", "watches")]
        ),
        entry(
            "cabal_sable_choir",
            "cabal",
            "Sable Choir",
            "A hidden cabal of ash-singers.",
            [link("deity_vhael", "serves")]
        ),
        entry(
            "place_ashen_reach",
            "place",
            "Ashen Reach",
            "A frontier of cinder roads.",
            [link("history_black_bell_year", "scarred_by")]
        ),
        entry(
            "history_black_bell_year",
            "history",
            "Black Bell Year",
            "The year temple bells darkened.",
            [link("entity_orren_mapless", "chronicles")]
        ),
        entry(
            "entity_orren_mapless",
            "entity",
            "Orren Mapless",
            "A guide who remembers roads by scent.",
            [link("cabal_sable_choir", "opposes")]
        ),
        entry(
            "artifact_cinder_astrolabe",
            "artifact",
            "Cinder Astrolabe",
            "A relic that maps ember constellations.",
            [link("deity_vhael", "attuned_to")]
        )
    ])
}

fn isolated_lore_entries() -> Value {
    let mut entries = sample_lore_entries();
    entries.as_array_mut().unwrap().push(entry(
        "culture_isolated_rite",
        "culture",
        "Isolated Rite",
        "A disconnected rite with no relationship proof.",
        json!([]),
    ));
    entries
}

fn entry(
    entry_id: &str,
    kind: &str,
    title: &str,
    summary: &str,
    links: impl serde::Serialize,
) -> Value {
    let timeline_markers = if kind == "history" {
        json!([{
            "marker_id": "marker_black_bell",
            "label": "Black Bell Year",
            "order": 1,
            "description": "The bells darken across the Reach."
        }])
    } else {
        json!([])
    };
    json!({
        "entry_id": entry_id,
        "kind": kind,
        "title": title,
        "summary": summary,
        "body_rich_text": format!("## {title}\n{summary}"),
        "metadata": { "issue": "1050" },
        "source_refs": ["issue_1049", "issue_1050"],
        "tags": ["ashen_reach", "relationship_graph"],
        "links": links,
        "timeline_markers": timeline_markers
    })
}

fn link(target_entry_id: &str, relation: &str) -> Value {
    json!({
        "target_entry_id": target_entry_id,
        "relation": relation,
        "description": format!("{relation} relationship to {target_entry_id}")
    })
}

fn query(
    query_id: &str,
    start_node_id: &str,
    target_node_id: &str,
    max_hops: usize,
    mode: WorldGraphTraversalMode,
) -> WorldGraphTraverseQuery {
    WorldGraphTraverseQuery {
        query_id: query_id.to_string(),
        start_node_id: start_node_id.to_string(),
        target_node_id: target_node_id.to_string(),
        max_hops,
        mode,
        requested_by: "litd-editor-issue1050-test".to_string(),
    }
}

fn path_ids(report: &crate::world_graph_navigation::WorldGraphTraversalReport) -> Vec<&str> {
    report
        .path
        .iter()
        .map(|node| node.node_id.as_str())
        .collect()
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
