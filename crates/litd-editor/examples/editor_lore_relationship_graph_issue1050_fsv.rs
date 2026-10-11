use std::path::Path;

use litd_editor::{
    mcp_fsv_tools::{READ_STORY_RENDER_STATE_TOOL, call_mcp_fsv_tool},
    mcp_story_tools::{
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL, READ_STORY_TOOL, UPSERT_LORE_ENTRY_TOOL,
        call_mcp_story_tool, read_mcp_story_state,
    },
    world_graph_navigation::{
        WorldGraphTraversalMode, WorldGraphTraversalStatus, WorldGraphTraverseQuery,
        parse_world_graph_navigation_ledger, read_world_graph_navigation_ledger,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1050_lore_relationship_graph")?;
    let state_path = run.run_dir().join("story-state-aster");
    let seed_ledger_path = run.run_dir().join("story-seed-ledger-aster");
    let graph_ledger_path = run.run_dir().join("relationship-graph-aster");

    seed_lore(
        &state_path,
        &seed_ledger_path,
        "issue1050-lore-state",
        lore_entries(),
    )?;
    let graph_output = call_mcp_story_tool(
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "graph_ledger_path": graph_ledger_path,
            "state_id": "issue1050-lore-state",
            "graph_id": "issue1050-lore-relationship-graph",
            "graph_ledger_id": "issue1050-lore-relationship-ledger",
            "request_id": "issue1050-lore-relationship-graph",
            "requested_by": "litd-editor-issue1050-fsv",
            "queries": [
                query("issue1050-fsv-forward", "cabal_sable_choir", "history_black_bell_year", 4, WorldGraphTraversalMode::FewestHops),
                query("issue1050-fsv-reciprocal", "place_ashen_reach", "cabal_sable_choir", 3, WorldGraphTraversalMode::FewestHops),
                query("issue1050-fsv-weighted", "artifact_cinder_astrolabe", "history_black_bell_year", 4, WorldGraphTraversalMode::HighestScore)
            ]
        }),
    )?;
    let story_read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path }))?;
    let state = read_mcp_story_state(&state_path)?;
    let graph_readback = read_world_graph_navigation_ledger(
        &graph_ledger_path,
        "issue1050-lore-relationship-ledger",
    )?
    .ok_or("missing issue1050 lore relationship graph ledger")?;
    let graph_ledger = parse_world_graph_navigation_ledger(&graph_readback.bytes)?;
    let fsv_readback = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({
            "artifacts": [
                { "label": "issue1050_story_state", "kind": "story_state", "path": state_path },
                { "label": "issue1050_story_seed_ledger", "kind": "story_ledger", "path": seed_ledger_path }
            ]
        }),
    )?;
    let empty_queries = graph_edge_no_write(
        &state_path,
        &run.run_dir().join("empty-query-graph-aster"),
        Vec::new(),
        "traversal query count",
    )?;
    let unknown_node = graph_edge_no_write(
        &state_path,
        &run.run_dir().join("unknown-node-graph-aster"),
        vec![query(
            "issue1050-fsv-unknown",
            "missing_entry",
            "place_ashen_reach",
            4,
            WorldGraphTraversalMode::FewestHops,
        )],
        "unknown node id",
    )?;
    let isolated_state = run.run_dir().join("isolated-story-state-aster");
    seed_lore(
        &isolated_state,
        &run.run_dir().join("isolated-seed-ledger-aster"),
        "issue1050-isolated-state",
        isolated_lore_entries(),
    )?;
    let isolated_entry = graph_edge_no_write_with_state_id(
        &isolated_state,
        &run.run_dir().join("isolated-graph-aster"),
        "issue1050-isolated-state",
        vec![query(
            "issue1050-fsv-isolated",
            "cabal_sable_choir",
            "place_ashen_reach",
            4,
            WorldGraphTraversalMode::FewestHops,
        )],
        "isolated",
    )?;

    let reciprocal = graph_ledger
        .traversals
        .iter()
        .find(|report| report.query_id == "issue1050-fsv-reciprocal")
        .ok_or("missing reciprocal traversal")?;
    let all_reached = graph_ledger
        .traversals
        .iter()
        .all(|report| report.status == WorldGraphTraversalStatus::Reached);
    let reciprocal_path = path_ids(reciprocal);
    let reciprocal_edges_are_inverse = reciprocal
        .edges
        .iter()
        .all(|edge| edge.relation.starts_with("inverse_"));
    let report = json!({
        "schema_version": 1,
        "issue": 1050,
        "source_of_truth": {
            "state_path": state_path,
            "graph_ledger_path": graph_ledger_path,
            "state_sha256": state.sha256,
            "graph_ledger_sha256": graph_readback.sha256,
            "fsv_aggregate_sha256": fsv_readback.aggregate_sha256,
            "research_refs": [
                "https://www.gamedeveloper.com/design/game-design-tools-narrative-graphs",
                "https://www.gamedeveloper.com/design/building-a-basic-story-bible-for-your-game",
                "https://book.leveldesignbook.com/process/preproduction/worldbuilding",
                "https://milvus.io/ai-quick-reference/how-do-you-ensure-data-consistency-in-a-knowledge-graph",
                "https://www.meegle.com/en_us/topics/knowledge-graphs/knowledge-graph-validation"
            ],
            "readme_refs": [
                "docs/readme/02-storytelling.png",
                "docs/readme/ED07-cinematic-director.png",
                "docs/readme/GP10-dialogue.png",
                "docs/readme/GP11-quest.png",
                "docs/readme/S16-quest-designer.png"
            ]
        },
        "happy_path": {
            "tool_name": graph_output.tool_name,
            "operation_id": graph_output.operation_id,
            "state_readback": story_read.result["state_readback"],
            "graph_output": graph_output.result,
            "graph_readback": {
                "sha256": graph_readback.sha256,
                "seq": graph_readback.seq,
                "graph_rows": graph_readback.graph_rows,
                "node_count": graph_readback.node_count,
                "edge_count": graph_readback.edge_count,
                "traversal_count": graph_readback.traversal_count
            },
            "reciprocal_path": reciprocal_path,
            "reciprocal_edges_are_inverse": reciprocal_edges_are_inverse,
            "fsv_readback": fsv_readback
        },
        "edges": {
            "empty_queries": empty_queries,
            "unknown_node": unknown_node,
            "isolated_entry": isolated_entry
        },
        "assertions": {
            "lore_entry_count": state.lore_entry_count,
            "lore_entry_link_count": state.lore_entry_link_count,
            "node_count": graph_ledger.graph.node_count,
            "edge_count": graph_ledger.graph.edge_count,
            "traversal_count": graph_ledger.traversal_count,
            "all_traversals_reached": all_reached,
            "reciprocal_path_matches": reciprocal_path == vec!["place_ashen_reach", "deity_vhael", "cabal_sable_choir"],
            "reciprocal_edges_are_inverse": reciprocal_edges_are_inverse,
            "fsv_artifact_count": fsv_readback.artifact_count
        }
    });
    if state.lore_entry_count != 6
        || state.lore_entry_link_count != 6
        || graph_ledger.graph.node_count != 6
        || graph_ledger.graph.edge_count != 12
        || graph_ledger.traversal_count != 3
        || !all_reached
        || reciprocal_path != vec!["place_ashen_reach", "deity_vhael", "cabal_sable_choir"]
        || !reciprocal_edges_are_inverse
        || fsv_readback.artifact_count != 2
    {
        return Err(format!("issue1050 FSV assertions failed: {report}").into());
    }
    let report_readback = run.write_report(&report)?;
    let report_sha256 = sha256_hex(&report_readback.bytes);
    if report_sha256 != report_readback.sha256 {
        return Err("report sha256 readback mismatch".into());
    }
    println!(
        "ISSUE1050_LORE_RELATIONSHIP_GRAPH_FSV report={} bytes={} sha256={} state={} state_sha256={} graph_ledger={} graph_ledger_sha256={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        state_path.display(),
        state.sha256,
        graph_ledger_path.display(),
        graph_readback.sha256
    );
    Ok(())
}

fn seed_lore(
    state_path: &Path,
    ledger_path: &Path,
    state_id: &str,
    entries: Value,
) -> Result<(), Box<dyn std::error::Error>> {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": state_id,
            "ledger_id": format!("{state_id}-seed-ledger"),
            "request_id": format!("{state_id}-seed"),
            "requested_by": "litd-editor-issue1050-fsv",
            "entries": entries
        }),
    )?;
    Ok(())
}

fn graph_edge_no_write(
    state_path: &Path,
    graph_ledger_path: &Path,
    queries: Vec<WorldGraphTraverseQuery>,
    expected_error: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    graph_edge_no_write_with_state_id(
        state_path,
        graph_ledger_path,
        "issue1050-lore-state",
        queries,
        expected_error,
    )
}

fn graph_edge_no_write_with_state_id(
    state_path: &Path,
    graph_ledger_path: &Path,
    state_id: &str,
    queries: Vec<WorldGraphTraverseQuery>,
    expected_error: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_story_state(state_path)?;
    let error = call_mcp_story_tool(
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "graph_ledger_path": graph_ledger_path,
            "state_id": state_id,
            "graph_id": "issue1050-lore-relationship-graph",
            "graph_ledger_id": "issue1050-edge-ledger",
            "request_id": "issue1050-edge",
            "requested_by": "litd-editor-issue1050-fsv",
            "queries": queries
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path)?;
    let message = error.to_string();
    if before.sha256 != after.sha256
        || graph_ledger_path.exists()
        || !message.contains(expected_error)
    {
        return Err(format!("edge did not fail closed: {message}").into());
    }
    Ok(json!({
        "failed_closed": true,
        "expected_error": expected_error,
        "actual_error": message,
        "state_sha256_before": before.sha256,
        "state_sha256_after": after.sha256,
        "graph_ledger_exists_after": graph_ledger_path.exists()
    }))
}

fn lore_entries() -> Value {
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
    let mut entries = lore_entries();
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
        requested_by: "litd-editor-issue1050-fsv".to_string(),
    }
}

fn path_ids(report: &litd_editor::world_graph_navigation::WorldGraphTraversalReport) -> Vec<&str> {
    report
        .path
        .iter()
        .map(|node| node.node_id.as_str())
        .collect()
}
