use std::fs;

use super::*;

#[test]
fn traverses_fewest_hops_and_highest_score_differently() {
    let graph = fixture_graph();
    let fewest = traverse_world_graph(
        &graph,
        &query(
            "issue1205-hop",
            "world.ashen",
            "quest.antidote",
            4,
            WorldGraphTraversalMode::FewestHops,
        ),
    )
    .unwrap();
    let weighted = traverse_world_graph(
        &graph,
        &query(
            "issue1205-weighted",
            "world.ashen",
            "quest.antidote",
            4,
            WorldGraphTraversalMode::HighestScore,
        ),
    )
    .unwrap();

    assert_eq!(fewest.status, WorldGraphTraversalStatus::Reached);
    assert_eq!(fewest.hop_count, Some(1));
    assert_eq!(path_ids(&fewest), vec!["world.ashen", "quest.antidote"]);
    assert_eq!(weighted.status, WorldGraphTraversalStatus::Reached);
    assert_eq!(weighted.hop_count, Some(3));
    assert_eq!(
        path_ids(&weighted),
        vec![
            "world.ashen",
            "faction.venom",
            "place.toxin_grove",
            "quest.antidote",
        ]
    );
    assert_eq!(weighted.score_milli, Some(625));
    println!(
        "issue1205 happy fewest_path={:?} weighted_path={:?} weighted_score={:?}",
        path_ids(&fewest),
        path_ids(&weighted),
        weighted.score_milli
    );
}

#[test]
fn unreachable_and_hop_cap_are_explicit_reports() {
    let graph = fixture_graph();
    let unreachable = traverse_world_graph(
        &graph,
        &query(
            "issue1205-unreachable",
            "world.ashen",
            "lore.isolated",
            4,
            WorldGraphTraversalMode::FewestHops,
        ),
    )
    .unwrap();
    let capped = traverse_world_graph(
        &graph,
        &query(
            "issue1205-hopcap",
            "world.ashen",
            "place.toxin_grove",
            1,
            WorldGraphTraversalMode::FewestHops,
        ),
    )
    .unwrap();

    assert_eq!(unreachable.status, WorldGraphTraversalStatus::Unreachable);
    assert_eq!(unreachable.code, "LITD-GRAPH-UNREACHABLE");
    assert!(unreachable.path.is_empty());
    assert_eq!(capped.status, WorldGraphTraversalStatus::Refused);
    assert_eq!(capped.code, "LITD-GRAPH-HOP-CAP-EXCEEDED");
    assert!(capped.reasons[0].contains("2 hops"));
    println!(
        "issue1205 edges unreachable={} hopcap={} reason={}",
        unreachable.code, capped.code, capped.reasons[0]
    );
}

#[test]
fn invalid_zero_weight_graph_fails_closed() {
    let mut graph = fixture_graph();
    graph.edges[0].weight = 0.0;

    let error = validate_world_graph_navigation_ledger(&WorldGraphNavigationLedger {
        schema: WORLD_GRAPH_NAVIGATION_SCHEMA.to_string(),
        ledger_id: "issue1205-invalid".to_string(),
        graph,
        traversal_count: 1,
        traversals: vec![empty_test_report()],
    })
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("editor.world_graph_navigation.graph.edge")
    );
    println!("issue1205 edge=invalid_zero_weight error={error}");
}

#[test]
fn ledger_persists_to_aster_and_reads_back() {
    let graph = fixture_graph();
    let traversals = vec![
        traverse_world_graph(
            &graph,
            &query(
                "issue1205-ledger-hop",
                "world.ashen",
                "quest.antidote",
                4,
                WorldGraphTraversalMode::FewestHops,
            ),
        )
        .unwrap(),
        traverse_world_graph(
            &graph,
            &query(
                "issue1205-ledger-score",
                "world.ashen",
                "quest.antidote",
                4,
                WorldGraphTraversalMode::HighestScore,
            ),
        )
        .unwrap(),
    ];
    let ledger =
        build_world_graph_navigation_ledger("issue1205-ledger-test", graph, traversals).unwrap();
    let root = test_root("world-graph-navigation");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("world-graph-navigation-ledger.json");
    let aster_dir = root.join("aster");

    let writeback = write_world_graph_navigation_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_world_graph_navigation_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_world_graph_navigation_ledger(&readback.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(readback.bytes, writeback.bytes);
    assert_eq!(parsed.traversal_count, 2);
    assert_eq!(parsed.graph.node_count, 6);
    assert_eq!(parsed.traversals[1].score_milli, Some(625));
    println!(
        "issue1205 aster traversals={} graph_rows={} sha={}",
        parsed.traversal_count, readback.graph_rows, readback.sha256
    );
}

fn fixture_graph() -> WorldKnowledgeGraph {
    let graph_id = "issue1205-world-knowledge";
    build_world_knowledge_graph(
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
    )
    .unwrap()
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
) -> WorldGraphTraverseQuery {
    WorldGraphTraverseQuery {
        query_id: query_id.to_string(),
        start_node_id: start_node_id.to_string(),
        target_node_id: target_node_id.to_string(),
        max_hops,
        mode,
        requested_by: "issue1205-test".to_string(),
    }
}

fn path_ids(report: &WorldGraphTraversalReport) -> Vec<&str> {
    report
        .path
        .iter()
        .map(|node| node.node_id.as_str())
        .collect()
}

fn empty_test_report() -> WorldGraphTraversalReport {
    WorldGraphTraversalReport {
        schema: WORLD_GRAPH_NAVIGATION_SCHEMA.to_string(),
        query_id: "issue1205-empty".to_string(),
        graph_id: "issue1205-world-knowledge".to_string(),
        status: WorldGraphTraversalStatus::Unreachable,
        code: "LITD-GRAPH-UNREACHABLE".to_string(),
        reasons: vec!["test report".to_string()],
        start_node_id: "world.ashen".to_string(),
        target_node_id: "lore.isolated".to_string(),
        max_hops: 4,
        mode: WorldGraphTraversalMode::FewestHops,
        requested_by: "issue1205-test".to_string(),
        node_count: 6,
        edge_count: 5,
        visited_count: 6,
        hop_count: None,
        score_milli: None,
        path: Vec::new(),
        edges: Vec::new(),
    }
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn test_root(name: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("editor-{name}"))
}
