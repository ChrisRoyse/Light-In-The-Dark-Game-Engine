use std::collections::BTreeSet;

use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AreaKind, GALADON_AREA_ID, area_spec_content_records, area_spec_readback,
        canonical_area_local_id, persist_and_lower_rpg_authored_content,
        validate_area_connections_against_registry, validate_area_spec,
        validate_rpg_authored_content_records,
    },
    rpg_area_specs_test_fixture::{galadon_area_spec, galadon_known_area_ids},
};

#[test]
fn issue1636_galadon_area_spec_matches_source_facts_and_canon() {
    let spec = galadon_area_spec();
    let readback = area_spec_readback(&spec).unwrap();

    assert_eq!(readback.area_id, GALADON_AREA_ID);
    assert_eq!(readback.display_name, "Galadon");
    assert_eq!(readback.player_facing_canon_name, "Galadon");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1636);
    assert_eq!(readback.level_band_min, 1);
    assert_eq!(readback.level_band_max, 30);
    assert_eq!(readback.gate_count, 4);
    assert!(readback.guildhall_count >= 3);
    assert!(readback.has_sewers);
    assert!(readback.is_hometown_option);
    assert!(readback.has_academy);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);
    assert_eq!(readback.visual_renderer, "Rust/wgpu");
    assert_eq!(readback.area_spec_sha256.len(), 64);
}

#[test]
fn issue1636_galadon_area_spec_persists_as_authored_calyx_content() {
    let spec = galadon_area_spec();
    let records = area_spec_content_records(&spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("galadon-area-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let summary = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();

    // One summary record plus one per room plus one per connection.
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(validation.record_count, expected_rows);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert_eq!(summary.kind, EngineContentKind::Terrain);
    assert_eq!(summary.id, GALADON_AREA_ID);
    assert_eq!(summary.payload["surface_kind"], "area_spec");
    assert_eq!(summary.payload["area_id"], GALADON_AREA_ID);
    assert_ne!(summary.payload["built_in_map_required"], true);
    assert_ne!(summary.payload["grants_map_or_minimap"], true);
    assert_ne!(summary.payload["grants_gps_markers"], true);
    assert!(
        summary
            .source_path
            .starts_with("editor://rpg/thera/area_specs/")
    );

    // A room row reads back identically to what was authored.
    let room = &spec.rooms[0];
    let room_row = store
        .get_content_for_editor(EngineContentKind::Terrain, &room.room_id)
        .unwrap();
    assert_eq!(room_row.payload["surface_kind"], "area_room");
    assert_eq!(room_row.payload["room_id"], room.room_id);
    assert_eq!(room_row.payload["display_name"], room.display_name);

    assert_eq!(lowered.row_count, expected_rows);
    assert!(lowered.entities[0].lowered_field_count > 0);
}

#[test]
fn issue1636_galadon_connections_pass_registry_referential_integrity() {
    let spec = galadon_area_spec();
    let known: BTreeSet<&str> = galadon_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

#[test]
fn issue1636_area_spec_fail_closed_edges() {
    // Edge: wrong schema.
    let mut wrong_schema = galadon_area_spec();
    wrong_schema.schema = "litd.rpg.area_spec.bad".to_string();
    assert_code(
        validate_area_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: empty/malformed room set rejected loud.
    let mut empty_rooms = galadon_area_spec();
    empty_rooms.rooms.clear();
    assert_code(
        validate_area_spec(&empty_rooms),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: intra-area dangling connection (from a room that does not exist).
    let mut dangling_from = galadon_area_spec();
    dangling_from.connections[0].from_room_id = "room.galadon.does_not_exist".to_string();
    assert_code(
        validate_area_spec(&dangling_from),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: cross-area dangling connection flagged by the registry pass.
    let mut dangling_area = galadon_area_spec();
    dangling_area.connections[0] = crate::rpg::AreaConnection {
        connection_id: "conn.galadon.to_void".to_string(),
        from_room_id: "room.galadon.north_gate".to_string(),
        to_area_id: "area.thera.nonexistent_void".to_string(),
        to_room_id: None,
        direction: crate::rpg::ConnectionDirection::North,
        travel_kind: crate::rpg::TravelKind::Road,
        discovery: crate::rpg::ConnectionDiscovery::Visible,
        locked_by_ref: None,
    };
    let known: BTreeSet<&str> = galadon_known_area_ids().into_iter().collect();
    assert_code(
        validate_area_connections_against_registry(&dangling_area, &known),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: recall point that is not a room in the area.
    let mut bad_recall = galadon_area_spec();
    bad_recall.recall.recall_room_id = Some("room.galadon.phantom_altar".to_string());
    assert_code(
        validate_area_spec(&bad_recall),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1725_variant_link_intra_area_validation() {
    // A base area (no link) is valid and reads back with no variant.
    let base = galadon_area_spec();
    let readback = area_spec_readback(&base).unwrap();
    assert_eq!(readback.variant_of_area_id, None);
    assert_eq!(readback.variant_label, None);

    // A well-formed link (base + label) is intra-area valid and echoed in the
    // readback (referential resolution is a corpus-registry concern, not here).
    let mut linked = galadon_area_spec();
    linked.variant_of_area_id = Some("area.thera.ruins_of_maethien".to_string());
    linked.variant_label = Some("past".to_string());
    validate_area_spec(&linked).unwrap();
    let linked_readback = area_spec_readback(&linked).unwrap();
    assert_eq!(
        linked_readback.variant_of_area_id.as_deref(),
        Some("area.thera.ruins_of_maethien")
    );
    assert_eq!(linked_readback.variant_label.as_deref(), Some("past"));

    // Edge: link without a label fails loud.
    let mut no_label = galadon_area_spec();
    no_label.variant_of_area_id = Some("area.thera.ruins_of_maethien".to_string());
    assert_code(
        validate_area_spec(&no_label),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: label without a link fails loud.
    let mut no_link = galadon_area_spec();
    no_link.variant_label = Some("past".to_string());
    assert_code(
        validate_area_spec(&no_link),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: an area cannot be a variant of itself.
    let mut self_ref = galadon_area_spec();
    self_ref.variant_of_area_id = Some(GALADON_AREA_ID.to_string());
    self_ref.variant_label = Some("past".to_string());
    assert_code(
        validate_area_spec(&self_ref),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1749_canonical_area_local_id_rule() {
    // The exact #1735 drift case: apostrophe becomes a single underscore, so the
    // seaport is `hamsah_mu_tazz` -- never the apostrophe-dropped `hamsah_mutazz`
    // the desert region batch had wrongly referenced.
    assert_eq!(canonical_area_local_id("Hamsah Mu'tazz"), "hamsah_mu_tazz");
    assert_ne!(canonical_area_local_id("Hamsah Mu'tazz"), "hamsah_mutazz");
    // Unicode right-single-quote apostrophe normalizes identically.
    assert_eq!(canonical_area_local_id("Mu\u{2019}tazz"), "mu_tazz");
    // Spaces and hyphens are token boundaries.
    assert_eq!(canonical_area_local_id("Loch Grynmear"), "loch_grynmear");
    assert_eq!(canonical_area_local_id("Delar-Tol"), "delar_tol");
    assert_eq!(canonical_area_local_id("Arin-gird"), "arin_gird");
    // Runs of punctuation/space collapse; leading/trailing boundaries trim.
    assert_eq!(canonical_area_local_id("  The   Wastes  "), "the_wastes");
    assert_eq!(
        canonical_area_local_id("Caravans of the Jhaana"),
        "caravans_of_the_jhaana"
    );
    // Idempotent on an already-canonical id segment.
    assert_eq!(canonical_area_local_id("hamsah_mu_tazz"), "hamsah_mu_tazz");
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}

// #1535: TestTempDir auto-cleans on Drop.
fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1636-{label}"))
}
