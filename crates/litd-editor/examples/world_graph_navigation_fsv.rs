use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::world_graph_navigation::{
    WorldGraphNavigationLedger, WorldGraphTraversalMode, WorldGraphTraversalReport,
    WorldGraphTraversalStatus, WorldKnowledgeEdge, WorldKnowledgeGraph, WorldKnowledgeNode,
    build_world_graph_navigation_ledger, build_world_knowledge_graph,
    parse_world_graph_navigation_ledger, read_world_graph_navigation_ledger, traverse_world_graph,
    world_knowledge_edge, world_knowledge_node, write_world_graph_navigation_ledger,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_WORLD_GRAPH_NAV_FSV",
        "issue1205_world_graph_navigation",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1205 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let state_path = root.join("world-graph-navigation-ledger.json");
    let aster_dir = root.join("aster");
    let before = path_state(&output_path, &state_path, &aster_dir);

    let graph = fixture_graph()?;
    let fewest = traverse_world_graph(
        &graph,
        &query(
            "issue1205-happy-hop",
            "world.ashen",
            "quest.antidote",
            4,
            WorldGraphTraversalMode::FewestHops,
        ),
    )?;
    let weighted = traverse_world_graph(
        &graph,
        &query(
            "issue1205-happy-weighted",
            "world.ashen",
            "quest.antidote",
            4,
            WorldGraphTraversalMode::HighestScore,
        ),
    )?;
    let unreachable = traverse_world_graph(
        &graph,
        &query(
            "issue1205-edge-unreachable",
            "world.ashen",
            "lore.isolated",
            4,
            WorldGraphTraversalMode::FewestHops,
        ),
    )?;
    let hop_cap = traverse_world_graph(
        &graph,
        &query(
            "issue1205-edge-hop-cap",
            "world.ashen",
            "place.toxin_grove",
            1,
            WorldGraphTraversalMode::FewestHops,
        ),
    )?;
    let invalid_graph_error = invalid_graph_error()?;
    let ledger = build_world_graph_navigation_ledger(
        "issue1205-world-graph-navigation-fsv",
        graph,
        vec![
            fewest.clone(),
            weighted.clone(),
            unreachable.clone(),
            hop_cap.clone(),
        ],
    )?;
    verify_expected(&ledger)?;

    let writeback = write_world_graph_navigation_ledger(&state_path, &aster_dir, &ledger)?;
    let readback = read_world_graph_navigation_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing issue1205 world-graph navigation ledger")?;
    let disk_bytes = fs::read(&state_path)?;
    if disk_bytes != readback.bytes {
        return Err(
            "state file bytes differ from Aster world-graph navigation ledger bytes".into(),
        );
    }
    let parsed = parse_world_graph_navigation_ledger(&readback.bytes)?;
    verify_expected(&parsed)?;

    let document = json!({
        "schema": "litd.issue1205.world_graph_navigation_fsv.v1",
        "issue": 1205,
        "source_of_truth": "Aster world-graph navigation ledger row and JSON mirror readback",
        "before": before,
        "happy_path": {
            "before": {"start": "world.ashen", "target": "quest.antidote", "max_hops": 4},
            "after": {
                "state_path": state_path,
                "state_sha256": sha256_hex(&disk_bytes),
                "aster_sha256": readback.sha256,
                "writeback_sha256": writeback.sha256,
                "state_matches_aster": disk_bytes == readback.bytes,
                "node_count": parsed.graph.node_count,
                "edge_count": parsed.graph.edge_count,
                "traversal_count": parsed.traversal_count,
                "graph_rows": readback.graph_rows,
                "seq": readback.seq,
                "fewest": summarize(&fewest),
                "weighted": summarize(&weighted),
                "passed": true
            }
        },
        "edges": [
            {
                "edge": "unreachable_target",
                "before": {"target": "lore.isolated"},
                "after": summarize(&unreachable)
            },
            {
                "edge": "weighted_vs_unweighted",
                "before": {"direct_weight": 0.2, "three_hop_weights": [0.95, 0.95, 0.95]},
                "after": {"fewest": summarize(&fewest), "weighted": summarize(&weighted)}
            },
            {
                "edge": "hop_cap_enforced",
                "before": {"target": "place.toxin_grove", "max_hops": 1},
                "after": summarize(&hop_cap)
            },
            {
                "edge": "invalid_zero_weight_rejected",
                "before": {"edge_weight": 0.0},
                "after": {"error": invalid_graph_error}
            }
        ]
    });
    write_document_with_final_state(&output_path, document)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote issue1205 world graph navigation FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    println!(
        "ledger traversals={} nodes={} edges={} graph_rows={} state_matches_aster={} sha={}",
        parsed.traversal_count,
        parsed.graph.node_count,
        parsed.graph.edge_count,
        readback.graph_rows,
        disk_bytes == readback.bytes,
        readback.sha256
    );
    println!(
        "paths fewest={:?} weighted={:?} weighted_score={:?} unreachable={} hopcap={}",
        path_ids(&fewest),
        path_ids(&weighted),
        weighted.score_milli,
        unreachable.code,
        hop_cap.code
    );
    Ok(())
}

fn verify_expected(ledger: &WorldGraphNavigationLedger) -> Result<(), Box<dyn std::error::Error>> {
    let fewest = report(ledger, "issue1205-happy-hop")?;
    let weighted = report(ledger, "issue1205-happy-weighted")?;
    let unreachable = report(ledger, "issue1205-edge-unreachable")?;
    let hop_cap = report(ledger, "issue1205-edge-hop-cap")?;
    if fewest.status != WorldGraphTraversalStatus::Reached
        || path_ids(fewest) != vec!["world.ashen", "quest.antidote"]
    {
        return Err("fewest-hop traversal did not use the direct association".into());
    }
    if weighted.status != WorldGraphTraversalStatus::Reached
        || weighted.score_milli != Some(625)
        || path_ids(weighted)
            != vec![
                "world.ashen",
                "faction.venom",
                "place.toxin_grove",
                "quest.antidote",
            ]
    {
        return Err("weighted traversal did not use the expected scored path".into());
    }
    if unreachable.code != "LITD-GRAPH-UNREACHABLE" || !unreachable.path.is_empty() {
        return Err("unreachable target did not persist an empty-path refusal".into());
    }
    if hop_cap.code != "LITD-GRAPH-HOP-CAP-EXCEEDED" || !hop_cap.path.is_empty() {
        return Err("hop cap was not enforced as an empty-path refusal".into());
    }
    Ok(())
}

fn fixture_graph() -> Result<WorldKnowledgeGraph, Box<dyn std::error::Error>> {
    let graph_id = "issue1205-world-knowledge";
    Ok(build_world_knowledge_graph(
        graph_id,
        "synthetic lore/quest/place/faction association graph for issue1205 FSV",
        vec![
            node(graph_id, "world.ashen", "World", "Ashen Reach"),
            node(graph_id, "faction.venom", "Faction", "Venom Circle"),
            node(graph_id, "place.toxin_grove", "Place", "Toxin Grove"),
            node(graph_id, "quest.antidote", "Quest", "Antidote Run"),
            node(graph_id, "agent.scout", "Agent", "Mire Scout"),
            node(graph_id, "lore.isolated", "Lore", "Isolated Moon Shrine"),
        ],
        vec![
            edge("world.ashen", "quest.antidote", "rumor_direct", 0.2),
            edge("world.ashen", "faction.venom", "dominant_faction", 0.95),
            edge("faction.venom", "place.toxin_grove", "operates_in", 0.95),
            edge("place.toxin_grove", "quest.antidote", "quest_source", 0.95),
            edge("quest.antidote", "agent.scout", "assigned_agent", 0.85),
        ],
    )?)
}

fn invalid_graph_error() -> Result<String, Box<dyn std::error::Error>> {
    let mut graph = fixture_graph()?;
    graph.edges[0].weight = 0.0;
    let query = query(
        "issue1205-invalid",
        "world.ashen",
        "quest.antidote",
        4,
        WorldGraphTraversalMode::HighestScore,
    );
    Ok(traverse_world_graph(&graph, &query)
        .unwrap_err()
        .to_string())
}

fn node(graph_id: &str, node_id: &str, kind: &str, title: &str) -> WorldKnowledgeNode {
    world_knowledge_node(
        graph_id,
        node_id,
        kind,
        title,
        1.0,
        vec![format!("Aster://issue1205/{node_id}")],
    )
}

fn edge(from: &str, to: &str, relation: &str, weight: f32) -> WorldKnowledgeEdge {
    world_knowledge_edge(
        from,
        to,
        relation,
        weight,
        vec![format!("Aster://issue1205/{from}/{to}/{relation}")],
    )
}

fn query(
    query_id: &str,
    start_node_id: &str,
    target_node_id: &str,
    max_hops: usize,
    mode: WorldGraphTraversalMode,
) -> litd_editor::world_graph_navigation::WorldGraphTraverseQuery {
    litd_editor::world_graph_navigation::WorldGraphTraverseQuery {
        query_id: query_id.to_string(),
        start_node_id: start_node_id.to_string(),
        target_node_id: target_node_id.to_string(),
        max_hops,
        mode,
        requested_by: "issue1205-fsv".to_string(),
    }
}

fn report<'a>(
    ledger: &'a WorldGraphNavigationLedger,
    query_id: &str,
) -> Result<&'a WorldGraphTraversalReport, Box<dyn std::error::Error>> {
    ledger
        .traversals
        .iter()
        .find(|report| report.query_id == query_id)
        .ok_or_else(|| format!("missing traversal {query_id}").into())
}

fn summarize(report: &WorldGraphTraversalReport) -> serde_json::Value {
    json!({
        "query_id": report.query_id,
        "status": format!("{:?}", report.status),
        "code": report.code,
        "mode": format!("{:?}", report.mode),
        "hop_count": report.hop_count,
        "score_milli": report.score_milli,
        "path": path_ids(report),
        "edges": report.edges.iter().map(|edge| json!({
            "edge_id": edge.edge_id,
            "weight": edge.weight,
            "relation": edge.relation
        })).collect::<Vec<_>>(),
        "reasons": report.reasons
    })
}

fn path_ids(report: &WorldGraphTraversalReport) -> Vec<String> {
    report
        .path
        .iter()
        .map(|node| node.node_id.clone())
        .collect()
}

fn path_state(output_path: &Path, state_path: &Path, aster_dir: &Path) -> serde_json::Value {
    json!({
        "output_exists": output_path.exists(),
        "state_exists": state_path.exists(),
        "aster_exists": aster_dir.exists()
    })
}

fn write_document_with_final_state(
    path: &PathBuf,
    mut document: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    document["after_artifact_write"] = json!({"output_path": path});
    let bytes = serde_json::to_vec_pretty(&document)?;
    fs::write(path, &bytes)?;
    let readback = fs::read(path)?;
    if readback != bytes {
        return Err("manual FSV artifact bytes differed after readback".into());
    }
    Ok(())
}
