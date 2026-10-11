// FSV for the #1632 overworld topology and #1633 road network. Both are
// validated against the SAME #1704 registry the corpus suite builds (single
// source of truth), persisted + read back through Calyx, and -- for the
// topology -- the committed master-map doc is asserted byte-equal to the doc
// rendered FROM the data, so it can never drift.

use crate::{
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AreaRegistry, RegionSpine, RoadExposure, overworld_topology_content_records,
        overworld_topology_readback, persist_and_lower_rpg_authored_content,
        render_overworld_master_map_markdown, road_network_content_records, road_network_readback,
        validate_road_network_against_registry, validate_topology_against_registry,
    },
    rpg_area_registry_corpus_tests::full_authored_corpus,
    rpg_overworld_topology_thera_fixture::thera_overworld_topology_spec,
    rpg_road_network_thera_fixture::thera_road_network_spec,
};

fn registry() -> AreaRegistry {
    AreaRegistry::from_specs(&full_authored_corpus()).expect("registry builds from corpus")
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1632-1633-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}

// ---------------------------------------------------------------------------
// #1632 overworld topology
// ---------------------------------------------------------------------------

#[test]
fn issue1632_topology_is_registry_valid_and_connected() {
    let spec = thera_overworld_topology_spec();
    // Every member area must resolve to an authored area (fails loud otherwise).
    validate_topology_against_registry(&spec, &registry())
        .expect("topology members all resolve to authored areas");

    let readback = overworld_topology_readback(&spec).unwrap();
    eprintln!(
        "issue1632 topology: regions={} edges={} members={} sectors={} \
         road={} sea={} portal={} underdark={} connected={}",
        readback.region_count,
        readback.edge_count,
        readback.member_area_count,
        readback.sector_count,
        readback.road_edge_count,
        readback.sea_edge_count,
        readback.portal_edge_count,
        readback.underdark_edge_count,
        readback.is_connected,
    );

    assert_eq!(readback.region_count, 10);
    assert_eq!(readback.edge_count, 12);
    assert!(readback.is_connected, "the overworld must be one component");
    assert_eq!(readback.sector_count, 8, "all eight world sectors are used");
    assert!(readback.member_area_count >= 60);
    assert_eq!(readback.road_edge_count, 6);
    assert_eq!(readback.sea_edge_count, 2);
    assert_eq!(readback.portal_edge_count, 1);
    assert_eq!(readback.underdark_edge_count, 2);
    assert_eq!(readback.topology_sha256.len(), 64);
}

#[test]
fn issue1632_topology_persists_and_reads_back() {
    let spec = thera_overworld_topology_spec();
    let records = overworld_topology_content_records(&spec).unwrap();
    // One summary + one per region + one per edge.
    let expected_rows = 1 + spec.regions.len() + spec.edges.len();
    assert_eq!(records.len(), expected_rows);

    let (store, _vault) = content_store("topology");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, expected_rows);

    // Fresh readback of the world summary and the first region.
    let summary = store
        .get_content_for_editor(EngineContentKind::Terrain, &spec.world_id)
        .unwrap();
    assert_eq!(summary.payload["surface_kind"], "overworld_topology");
    assert_eq!(summary.payload["region_count"], spec.regions.len());
    assert_ne!(summary.payload["grants_map_or_minimap"], true);

    let first = &spec.regions[0];
    let region_row = store
        .get_content_for_editor(EngineContentKind::Terrain, &first.region_id)
        .unwrap();
    assert_eq!(region_row.payload["surface_kind"], "overworld_region");
    assert_eq!(region_row.payload["display_name"], first.display_name);
    assert_eq!(
        region_row.payload["member_area_count"],
        first.member_area_ids.len()
    );
}

#[test]
fn issue1632_master_map_doc_matches_generated() {
    let spec = thera_overworld_topology_spec();
    let generated = render_overworld_master_map_markdown(&spec);
    let committed_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/world/thera_master_map.md"
    );
    let committed = std::fs::read_to_string(committed_path)
        .unwrap_or_else(|e| panic!("committed master map doc missing at {committed_path}: {e}"))
        .replace("\r\n", "\n");
    assert_eq!(
        committed, generated,
        "committed docs/world/thera_master_map.md drifted from the generated topology doc; \
         regenerate it from thera_overworld_topology_spec()"
    );
}

/// Regenerates the committed golden doc FROM the topology data. Run explicitly
/// after changing the topology: `cargo test -p litd-engine --lib
/// regenerate_master_map_doc -- --ignored`. The normal
/// `issue1632_master_map_doc_matches_generated` test then guards it against drift.
#[test]
#[ignore = "golden-file regenerator; run manually after topology changes"]
fn regenerate_master_map_doc() {
    let spec = thera_overworld_topology_spec();
    let generated = render_overworld_master_map_markdown(&spec);
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/world/thera_master_map.md"
    );
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, generated).unwrap();
    eprintln!("wrote {path}");
}

// ---------------------------------------------------------------------------
// #1633 road network
// ---------------------------------------------------------------------------

#[test]
fn issue1633_road_network_is_registry_valid() {
    let spec = thera_road_network_spec();
    validate_road_network_against_registry(&spec, &registry())
        .expect("every road waypoint/gate/crossroad area resolves to an authored area");

    let readback = road_network_readback(&spec).unwrap();
    eprintln!(
        "issue1633 roads: roads={} segments={} waypoints={} gates={} law_gates={} \
         crossroads={} exposed={} chokepoint={}",
        readback.road_count,
        readback.segment_count,
        readback.waypoint_count,
        readback.gate_count,
        readback.law_zone_gate_count,
        readback.crossroad_count,
        readback.exposed_road_count,
        readback.chokepoint_road_count,
    );

    assert_eq!(readback.road_count, 6);
    assert_eq!(readback.crossroad_count, 2);
    assert!(readback.gate_count >= 6);
    assert!(
        readback.law_zone_gate_count >= 4,
        "Galadon's four gates are Tribunal law zones"
    );
    assert_eq!(readback.exposed_road_count, 4);
    assert_eq!(readback.chokepoint_road_count, 1);
    assert_eq!(readback.road_network_sha256.len(), 64);

    // Named-road sanity: Silverpine Road and the North Road are present.
    let names: Vec<&str> = spec.roads.iter().map(|r| r.display_name.as_str()).collect();
    assert!(names.contains(&"Silverpine Road"));
    assert!(names.contains(&"The North Road"));
    // Exposure vocabulary is exercised (exposed roads + a chokepoint pass).
    assert!(
        spec.roads
            .iter()
            .any(|r| r.exposure == RoadExposure::Exposed)
    );
    assert!(
        spec.roads
            .iter()
            .any(|r| r.exposure == RoadExposure::Chokepoint)
    );
}

#[test]
fn issue1633_road_network_persists_and_reads_back() {
    let spec = thera_road_network_spec();
    let records = road_network_content_records(&spec).unwrap();
    // One summary + one per road + one per crossroad.
    let expected_rows = 1 + spec.roads.len() + spec.crossroads.len();
    assert_eq!(records.len(), expected_rows);

    let (store, _vault) = content_store("roads");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, expected_rows);

    let summary = store
        .get_content_for_editor(EngineContentKind::Terrain, &spec.world_id)
        .unwrap();
    assert_eq!(summary.payload["surface_kind"], "road_network");
    assert_eq!(summary.payload["road_count"], spec.roads.len());
    assert_ne!(summary.payload["grants_gps_markers"], true);

    let first = &spec.roads[0];
    let road_row = store
        .get_content_for_editor(EngineContentKind::Terrain, &first.road_id)
        .unwrap();
    assert_eq!(road_row.payload["surface_kind"], "named_road");
    assert_eq!(road_row.payload["display_name"], first.display_name);
}

#[test]
fn issue1632_1633_share_one_registry_no_parallel_id_list() {
    // Both specs validate against the registry derived from the authored corpus:
    // there is no hand-maintained id list either could drift from.
    let reg = registry();
    validate_topology_against_registry(&thera_overworld_topology_spec(), &reg).unwrap();
    validate_road_network_against_registry(&thera_road_network_spec(), &reg).unwrap();
    // A spine kind we authored is really represented (guards the enum wiring).
    let topo = thera_overworld_topology_spec();
    assert!(topo.edges.iter().any(|e| e.spine == RegionSpine::Portal));
}
