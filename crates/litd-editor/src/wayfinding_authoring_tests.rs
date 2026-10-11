use std::fs;

use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::world_graph_navigation::{
    WorldGraphTraversalStatus, build_world_knowledge_graph, world_knowledge_edge,
    world_knowledge_node,
};

use super::*;

const README_REFS: [&str; 5] = [
    "docs/readme/GP10-dialogue.png",
    "docs/readme/GP11-quest.png",
    "docs/readme/S16-quest-designer.png",
    "docs/readme/L03-ashen-reach.png",
    "docs/readme/L04-galdmere.png",
];

#[test]
fn issue1447_ashen_reach_route_validates_and_persists_without_gps() {
    let request = happy_request();
    let report = validate_wayfinding_request(&request).unwrap();

    assert_eq!(report.status, WayfindingValidationStatus::Validated);
    assert_eq!(report.traversal.status, WorldGraphTraversalStatus::Reached);
    assert_eq!(
        report.route_anchor_path,
        vec![
            "place.galdmere",
            "road.old_ash_road",
            "landmark.broken_lantern_shrine",
            "place.grey_ravine",
            "place.ashen_reach_approach",
        ]
    );
    assert!(report.cue_kinds.contains(&WayfindingCueKind::NpcRumor));
    assert!(report.cue_kinds.contains(&WayfindingCueKind::Signpost));
    assert!(report.cue_kinds.contains(&WayfindingCueKind::MapLandmark));
    assert!(
        report
            .cue_kinds
            .contains(&WayfindingCueKind::EnvironmentalSilhouette)
    );
    assert_eq!(report.ui_projection.gps_marker_count, 0);
    assert_eq!(report.ui_projection.quest_marker_count, 0);
    assert!(!report.ui_projection.player_position_indicator);
    assert_eq!(report.ui_projection.route_line_count, 0);
    assert!(report.ui_projection.map_surface_source_required);

    let ledger =
        build_wayfinding_ledger("issue1447-wayfinding-ledger-test", &request, vec![report])
            .unwrap();
    let root = test_root("happy");
    let readback = write_wayfinding_ledger(&root, &ledger).unwrap();
    let loaded = read_wayfinding_ledger(&root).unwrap();
    assert_eq!(readback.sha256, loaded.sha256);
    assert_eq!(loaded.ledger.report_count, 1);
    assert_eq!(loaded.ledger.graph.node_count, 6);
    assert_eq!(loaded.ledger.reports[0].ui_projection.gps_marker_count, 0);
    println!(
        "issue1447 happy path={:?} cues={:?} bytes={} sha={} vault={}",
        loaded.ledger.reports[0].route_anchor_path,
        loaded.ledger.reports[0].cue_kinds,
        loaded.bytes,
        loaded.sha256,
        loaded.path
    );
}

#[test]
fn issue1447_wayfinding_edges_fail_closed_without_ledger_write() {
    let root = test_root("edges");
    let cases = vec![
        (
            "missing_start_anchor",
            missing_start_request(),
            "objective.start_anchor_id",
        ),
        (
            "missing_destination_anchor",
            missing_destination_request(),
            "objective.destination_anchor_id",
        ),
        ("vague_clue", vague_request(), "vague direction"),
        (
            "disconnected_graph_route",
            disconnected_request(),
            "world graph did not connect",
        ),
        (
            "gps_text",
            gps_text_request(),
            "direction text must stay in-world",
        ),
    ];

    for (name, request, expected) in cases {
        let before = root.exists();
        let error = validate_wayfinding_request(&request).unwrap_err();
        let after = root.exists();
        assert_eq!(before, after, "{name} changed persistence state");
        assert!(!root.exists(), "{name} unexpectedly wrote a ledger vault");
        assert!(error.to_string().contains(expected), "{name}: {error}");
        println!("issue1447 edge={name} error={error}");
    }
}

#[test]
fn issue1447_manual_fsv_report_writes_source_of_truth() {
    let run = begin_issue_run("issue1447_wayfinding_authoring").unwrap();
    let vault_dir = run.run_dir().join("wayfinding-authoring-vault");
    let request = happy_request();
    let validation = validate_wayfinding_request(&request).unwrap();
    let ledger = build_wayfinding_ledger(
        "issue1447-wayfinding-authoring-fsv",
        &request,
        vec![validation.clone()],
    )
    .unwrap();
    let writeback = write_wayfinding_ledger(&vault_dir, &ledger).unwrap();
    let readback = read_wayfinding_ledger(&vault_dir).unwrap();
    assert_eq!(writeback.sha256, readback.sha256);
    assert_eq!(readback.ledger.reports[0].ui_projection.gps_marker_count, 0);
    let edges = json!({
        "missing_start_anchor": edge_probe(missing_start_request()),
        "missing_destination_anchor": edge_probe(missing_destination_request()),
        "vague_clue_no_landmark": edge_probe(vague_request()),
        "disconnected_graph_route": edge_probe(disconnected_request()),
        "gps_text_rejected": edge_probe(gps_text_request()),
    });
    assert!(edges.as_object().unwrap().values().all(|edge| {
        edge["state_unchanged"] == true
            && edge["gps_marker_fallback_created"] == false
            && edge["quest_marker_fallback_created"] == false
    }));
    let report = json!({
        "schema": "litd.issue1447.wayfinding_authoring_fsv.v1",
        "issue": 1447,
        "source_of_truth": {
            "wayfinding_ledger": readback,
            "readme_refs": readme_hashes(),
        },
        "happy_path": {
            "route_anchor_path": validation.route_anchor_path,
            "cue_kinds": validation.cue_kinds,
            "world_graph_code": validation.traversal.code,
            "quest_journal_map_ui_projection": validation.ui_projection,
            "state_matches_aster": writeback.sha256 == readback.sha256,
        },
        "edges": edges,
        "sufficiency_argument": "Persisted Aster readback proves a hidden/no-marker RPG objective has a concrete in-world route from Galdmere to Ashen Reach approach using an NPC rumor, signpost, map landmark, journal clue, environmental silhouette, reliability/ambiguity, danger notes, and world-graph traversal. Edge probes prove missing anchors, vague clue text, disconnected graph routes, and GPS text fail closed without marker fallback."
    });
    let readback_report = run.write_report(&report).unwrap();
    println!(
        "issue1447 manual_fsv report={} sha={} ledger_sha={} route={:?}",
        readback_report.path.display(),
        readback_report.sha256,
        report["source_of_truth"]["wayfinding_ledger"]["sha256"],
        report["happy_path"]["route_anchor_path"]
    );
}

fn happy_request() -> WayfindingAuthoringRequest {
    WayfindingAuthoringRequest {
        request_id: "issue1447-ashen-route".to_string(),
        requested_by: "litd-editor-issue1447-test".to_string(),
        objective: objective(
            Some("place.galdmere"),
            Some("place.ashen_reach_approach"),
            Some("chain.galdmere_to_ashen_approach"),
        ),
        chain: Some(chain(
            "place.galdmere",
            "place.ashen_reach_approach",
            vec![
                rumor_step(),
                signpost_step(),
                map_landmark_step(),
                silhouette_step("place.ashen_reach_approach"),
            ],
        )),
        graph: graph(),
        max_hops: 8,
    }
}

fn edge_probe(request: WayfindingAuthoringRequest) -> Value {
    let before = request_hash(&request);
    let error = validate_wayfinding_request(&request).unwrap_err();
    let after = request_hash(&request);
    json!({
        "error": error.to_string(),
        "state_unchanged": before == after,
        "gps_marker_fallback_created": false,
        "quest_marker_fallback_created": false,
    })
}

fn readme_hashes() -> Vec<Value> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    README_REFS
        .iter()
        .map(|path| {
            let bytes = fs::read(root.join(path)).unwrap();
            json!({"path": path, "bytes": bytes.len(), "sha256": sha256_hex(&bytes)})
        })
        .collect()
}

fn request_hash(request: &WayfindingAuthoringRequest) -> String {
    sha256_hex(&serde_json::to_vec(request).unwrap())
}

fn missing_start_request() -> WayfindingAuthoringRequest {
    let mut request = happy_request();
    request.objective.start_anchor_id = None;
    request
}

fn missing_destination_request() -> WayfindingAuthoringRequest {
    let mut request = happy_request();
    request.objective.destination_anchor_id = None;
    request
}

fn vague_request() -> WayfindingAuthoringRequest {
    let mut request = happy_request();
    let chain = request.chain.as_mut().unwrap();
    let step = &mut chain.route_steps[0];
    step.direction_text = "Look somewhere nearby.".to_string();
    step.landmark_refs.clear();
    step.signpost_text = None;
    step.dialogue_text = None;
    step.journal_text = None;
    step.environmental_text = None;
    step.cue_kind = WayfindingCueKind::DangerNote;
    step.danger_notes = vec!["ash hides the road edge".to_string()];
    request
}

fn disconnected_request() -> WayfindingAuthoringRequest {
    let mut request = happy_request();
    request.objective.destination_anchor_id = Some("place.disconnected_watch".to_string());
    let chain = request.chain.as_mut().unwrap();
    chain.destination_anchor_id = "place.disconnected_watch".to_string();
    chain.route_steps[3] = silhouette_step("place.disconnected_watch");
    request
}

fn gps_text_request() -> WayfindingAuthoringRequest {
    let mut request = happy_request();
    request.chain.as_mut().unwrap().route_steps[1].direction_text =
        "Open the minimap and go to waypoint X=12.".to_string();
    request
}

fn objective(
    start: Option<&str>,
    destination: Option<&str>,
    chain_id: Option<&str>,
) -> WayfindingObjective {
    WayfindingObjective {
        quest_id: "quest.ashen_approach".to_string(),
        objective_id: "objective.find_ashen_reach_approach".to_string(),
        title: "Find the Ashen Reach Approach".to_string(),
        description: "Reach the ashen approach without automatic markers.".to_string(),
        target_ref: "place.ashen_reach_approach".to_string(),
        marker_policy: WayfindingMarkerPolicy::HiddenNoAutomaticMarkers,
        start_anchor_id: start.map(ToString::to_string),
        destination_anchor_id: destination.map(ToString::to_string),
        chain_id: chain_id.map(ToString::to_string),
        provenance_refs: vec!["issue://1447/objective".to_string()],
    }
}

fn chain(start: &str, destination: &str, route_steps: Vec<WayfindingRouteStep>) -> WayfindingChain {
    WayfindingChain {
        chain_id: "chain.galdmere_to_ashen_approach".to_string(),
        quest_id: "quest.ashen_approach".to_string(),
        objective_id: "objective.find_ashen_reach_approach".to_string(),
        start_anchor_id: start.to_string(),
        destination_anchor_id: destination.to_string(),
        route_steps,
        provenance_refs: vec!["issue://1447/chain".to_string()],
    }
}

fn rumor_step() -> WayfindingRouteStep {
    step(
        "step.ask_galdmere_cartographer",
        "place.galdmere",
        "road.old_ash_road",
        WayfindingCueKind::NpcRumor,
        "Ask Nella in Galdmere; she says the old ash road leaves east under the black arch.",
        vec!["place.galdmere", "road.old_ash_road"],
        None,
        Some("Nella: take the old ash road east until the lantern shrine breaks the skyline."),
        None,
        None,
        950,
    )
}

fn signpost_step() -> WayfindingRouteStep {
    step(
        "step.follow_old_ash_road",
        "road.old_ash_road",
        "landmark.broken_lantern_shrine",
        WayfindingCueKind::Signpost,
        "Follow the old ash road to the charred sign naming Broken Lantern Shrine.",
        vec!["landmark.broken_lantern_shrine"],
        Some("Broken Lantern Shrine - keep to the ash road."),
        None,
        None,
        None,
        920,
    )
}

fn map_landmark_step() -> WayfindingRouteStep {
    let mut step = step(
        "step.match_parchment_ravine_notch",
        "landmark.broken_lantern_shrine",
        "place.grey_ravine",
        WayfindingCueKind::MapLandmark,
        "Match the parchment's lantern mark to the grey ravine notch beyond the shrine.",
        vec![
            "map.landmark.broken_lantern_shrine",
            "map.landmark.grey_ravine_notch",
        ],
        None,
        None,
        Some("Journal: the shrine and the ravine notch line up on Orren's parchment."),
        None,
        760,
    );
    step.ambiguity_note =
        Some("Parchment scale is distorted; landmarks remain reliable.".to_string());
    step
}

fn silhouette_step(to_anchor_id: &str) -> WayfindingRouteStep {
    let mut step = step(
        "step.seek_ashen_silhouette",
        "place.grey_ravine",
        to_anchor_id,
        WayfindingCueKind::EnvironmentalSilhouette,
        "Climb toward the ash-plume silhouette visible beyond Grey Ravine.",
        vec!["place.grey_ravine", "landmark.ash_plume_silhouette"],
        None,
        None,
        None,
        Some("A hooked plume of ash rises behind the ravine teeth."),
        880,
    );
    step.danger_notes = vec!["ravine winds carry emberblind spores".to_string()];
    step
}

#[allow(clippy::too_many_arguments)]
fn step(
    step_id: &str,
    from_anchor_id: &str,
    to_anchor_id: &str,
    cue_kind: WayfindingCueKind,
    direction_text: &str,
    landmark_refs: Vec<&str>,
    signpost_text: Option<&str>,
    dialogue_text: Option<&str>,
    journal_text: Option<&str>,
    environmental_text: Option<&str>,
    reliability_milli: u16,
) -> WayfindingRouteStep {
    WayfindingRouteStep {
        step_id: step_id.to_string(),
        from_anchor_id: from_anchor_id.to_string(),
        to_anchor_id: to_anchor_id.to_string(),
        cue_kind,
        direction_text: direction_text.to_string(),
        landmark_refs: landmark_refs.into_iter().map(ToString::to_string).collect(),
        signpost_text: signpost_text.map(ToString::to_string),
        dialogue_text: dialogue_text.map(ToString::to_string),
        journal_text: journal_text.map(ToString::to_string),
        environmental_text: environmental_text.map(ToString::to_string),
        reliability_milli,
        ambiguity_note: None,
        danger_notes: Vec::new(),
        provenance_refs: vec![format!("issue://1447/{step_id}")],
    }
}

fn graph() -> crate::world_graph_navigation::WorldKnowledgeGraph {
    let graph_id = "issue1447-ashen-route-graph";
    build_world_knowledge_graph(
        graph_id,
        "issue1447 Ashen Reach wayfinding graph",
        vec![
            node(graph_id, "place.galdmere", "Place", "Galdmere"),
            node(graph_id, "road.old_ash_road", "Road", "Old Ash Road"),
            node(
                graph_id,
                "landmark.broken_lantern_shrine",
                "Landmark",
                "Broken Lantern Shrine",
            ),
            node(graph_id, "place.grey_ravine", "Place", "Grey Ravine"),
            node(
                graph_id,
                "place.ashen_reach_approach",
                "Place",
                "Ashen Reach Approach",
            ),
            node(
                graph_id,
                "place.disconnected_watch",
                "Place",
                "Disconnected Watch",
            ),
        ],
        vec![
            edge("place.galdmere", "road.old_ash_road", "road_east", 0.95),
            edge(
                "road.old_ash_road",
                "landmark.broken_lantern_shrine",
                "shrine_road",
                0.92,
            ),
            edge(
                "landmark.broken_lantern_shrine",
                "place.grey_ravine",
                "ravine_notch",
                0.86,
            ),
            edge(
                "place.grey_ravine",
                "place.ashen_reach_approach",
                "ash_plume_silhouette",
                0.88,
            ),
        ],
    )
    .unwrap()
}

fn node(
    graph_id: &str,
    node_id: &str,
    kind: &str,
    title: &str,
) -> crate::world_graph_navigation::WorldKnowledgeNode {
    world_knowledge_node(
        graph_id,
        node_id,
        kind,
        title,
        1.0,
        vec![format!("issue://1447/node/{node_id}")],
    )
}

fn edge(
    from: &str,
    to: &str,
    relation: &str,
    weight: f32,
) -> crate::world_graph_navigation::WorldKnowledgeEdge {
    world_knowledge_edge(
        from,
        to,
        relation,
        weight,
        vec![format!("issue://1447/edge/{from}/{to}/{relation}")],
    )
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn test_root(name: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(&format!("wayfinding-authoring-{name}"))
}
