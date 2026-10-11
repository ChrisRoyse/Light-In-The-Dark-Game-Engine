// Persist -> separate readback parity for every AreaSpec in the #1649-#1660
// batch, plus the #944 map/GPS doctrine check. Every area/room/connection id in
// this batch is globally unique (asserted in regions_thera_tests_readback.rs),
// so all ~57 specs are persisted into one shared store rather than one store
// per spec, mirroring how a real world would be loaded.

use crate::{
    calyx_content::EngineContentKind,
    rpg::{area_spec_content_records, persist_and_lower_rpg_authored_content},
};

use super::regions_thera_registry::all_region_specs;
use super::regions_thera_support::content_store;

#[test]
fn issue1649_1660_all_specs_persist_and_separate_readback_matches_authored() {
    let specs = all_region_specs();
    let (store, _vault) = content_store("all-specs");

    let mut total_rows = 0usize;
    for spec in &specs {
        let records = area_spec_content_records(spec).unwrap();
        let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
        assert_eq!(lowered.row_count, records.len());
        total_rows += records.len();

        // Separate readback: get_content_for_editor is a fresh call against the
        // store, not the in-memory `records` we just built.
        let summary_row = store
            .get_content_for_editor(EngineContentKind::Terrain, &spec.area_id)
            .unwrap();
        assert_eq!(summary_row.id, spec.area_id);
        assert_eq!(summary_row.payload["surface_kind"], "area_spec");
        assert_eq!(
            summary_row.payload["area_spec_sha256"],
            records[0].payload["area_spec_sha256"]
        );
        assert_ne!(summary_row.payload["built_in_map_required"], true);
        assert_ne!(summary_row.payload["grants_map_or_minimap"], true);
        assert_ne!(summary_row.payload["grants_gps_markers"], true);
        assert!(
            summary_row
                .source_path
                .starts_with("editor://rpg/thera/area_specs/")
        );

        let first_room = &spec.rooms[0];
        let room_row = store
            .get_content_for_editor(EngineContentKind::Terrain, &first_room.room_id)
            .unwrap();
        assert_eq!(room_row.payload["surface_kind"], "area_room");
        assert_eq!(room_row.payload["display_name"], first_room.display_name);
    }

    assert!(
        total_rows > 300,
        "expected substantial content volume, got {total_rows}"
    );
}
