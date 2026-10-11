// #1644-#1647 persistence tests: each AreaSpec persists via the generic authored-
// content pipeline and reads back byte-identical through a SEPARATE store lookup, with
// the acquired-map doctrine flags (#944) never true.
use super::towns_thera_support::{all_town_specs, content_store};
use crate::{
    calyx_content::EngineContentKind,
    rpg::{
        area_spec_content_records, area_spec_readback, persist_and_lower_rpg_authored_content,
        validate_rpg_authored_content_records,
    },
};

#[test]
fn towns_thera_specs_persist_and_read_back_stored_equals_authored() {
    for (spec, exp) in all_town_specs() {
        let where_ = exp.area_id;
        let readback = area_spec_readback(&spec).unwrap();
        let records = area_spec_content_records(&spec).unwrap();
        let validation = validate_rpg_authored_content_records(&records).unwrap();

        let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
        assert_eq!(records.len(), expected_rows, "record count at {where_}");
        assert_eq!(
            validation.record_count, expected_rows,
            "validation count at {where_}"
        );
        assert!(
            validation.all_editor_authored_sources,
            "authored sources at {where_}"
        );
        assert!(
            validation.no_builtin_map_runtime,
            "no builtin map at {where_}"
        );

        let label = spec.area_id.replace('.', "-");
        let (store, _vault) = content_store(&label);
        let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
        assert_eq!(lowered.row_count, expected_rows, "lowered rows at {where_}");

        // Separate read of the summary row proves stored == authored.
        let summary = store
            .get_content_for_editor(records[0].kind, &records[0].id)
            .unwrap();
        assert_eq!(summary.kind, EngineContentKind::Terrain, "kind at {where_}");
        assert_eq!(summary.id, spec.area_id, "summary id at {where_}");
        assert_eq!(
            summary.payload["surface_kind"], "area_spec",
            "surface_kind at {where_}"
        );
        assert_eq!(
            summary.payload["area_id"], spec.area_id,
            "payload area_id at {where_}"
        );
        assert_eq!(
            summary.payload["area_spec_sha256"],
            serde_json::Value::String(readback.area_spec_sha256.clone()),
            "sha256 stored==authored at {where_}"
        );
        assert_ne!(
            summary.payload["built_in_map_required"], true,
            "map flag at {where_}"
        );
        assert_ne!(
            summary.payload["grants_map_or_minimap"], true,
            "minimap flag at {where_}"
        );
        assert_ne!(
            summary.payload["grants_gps_markers"], true,
            "gps flag at {where_}"
        );
        assert!(
            summary
                .source_path
                .starts_with("editor://rpg/thera/area_specs/"),
            "source path at {where_}"
        );

        // A room row also round-trips identically.
        let room = &spec.rooms[0];
        let room_row = store
            .get_content_for_editor(EngineContentKind::Terrain, &room.room_id)
            .unwrap();
        assert_eq!(
            room_row.payload["surface_kind"], "area_room",
            "room surface at {where_}"
        );
        assert_eq!(
            room_row.payload["room_id"], room.room_id,
            "room id at {where_}"
        );
        assert_eq!(
            room_row.payload["display_name"], room.display_name,
            "room name at {where_}"
        );
        assert_ne!(
            room_row.payload["grants_gps_markers"], true,
            "room gps flag at {where_}"
        );

        assert!(
            lowered.entities[0].lowered_field_count > 0,
            "lowered fields at {where_}"
        );
    }
}
