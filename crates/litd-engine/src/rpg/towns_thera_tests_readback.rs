// #1644-#1647 readback tests: every authored AreaSpec's readback matches its
// authored facts and canon (data-driven over the whole batch).
use super::towns_thera_support::all_town_specs;
use crate::rpg::{area_spec_readback, validate_area_spec};

#[test]
fn towns_thera_readback_matches_authored_facts_and_canon() {
    for (spec, exp) in all_town_specs() {
        let readback = area_spec_readback(&spec).unwrap();
        let where_ = exp.area_id;
        assert_eq!(readback.area_id, exp.area_id, "area_id at {where_}");
        assert_eq!(
            readback.display_name, spec.display_name,
            "display_name at {where_}"
        );
        assert_eq!(
            readback.source_issue, exp.source_issue,
            "source_issue at {where_}"
        );
        assert_eq!(readback.area_kind, exp.area_kind, "area_kind at {where_}");
        assert!(
            readback.room_count >= exp.min_rooms,
            "room_count {} < {} at {where_}",
            readback.room_count,
            exp.min_rooms
        );
        assert_eq!(
            readback.is_hometown_option, exp.is_hometown_option,
            "is_hometown_option at {where_}"
        );
        assert_eq!(
            readback.has_recall_point, exp.has_recall_point,
            "has_recall_point at {where_}"
        );
        assert_eq!(
            readback.visual_renderer, "Rust/wgpu",
            "renderer at {where_}"
        );
        assert!(readback.prop_count >= 3, "prop_count at {where_}");
        assert_eq!(
            readback.area_spec_sha256.len(),
            64,
            "sha256 len at {where_}"
        );
        assert!(
            readback.connection_count > 0,
            "connection_count at {where_}"
        );

        // Canon must include the parent, the world graph, and the source issue.
        assert!(spec.canon_issues.contains(&987), "canon 987 at {where_}");
        assert!(spec.canon_issues.contains(&1632), "canon 1632 at {where_}");
        assert!(
            spec.canon_issues.contains(&spec.source_issue),
            "canon source at {where_}"
        );
        assert!(
            spec.mechanics_source_url
                .starts_with("https://wiki.carrionfields.net/"),
            "mechanics url at {where_}"
        );

        // The whole spec passes the generic validator.
        validate_area_spec(&spec).unwrap();
    }
}

#[test]
fn towns_thera_batch_has_27_distinct_places_covering_all_four_issues() {
    let specs = all_town_specs();
    assert_eq!(specs.len(), 27, "expected 27 authored places");

    // Reused ids that resolve Galadon's forward-stub connections (#1636).
    for reused in [
        "area.thera.galadon.copeham_inn",
        "area.thera.vale_of_arendyl",
        "area.thera.galadon.goblin_village",
    ] {
        assert!(
            specs.iter().any(|(spec, _)| spec.area_id == reused),
            "missing reused id {reused}"
        );
    }

    // Every area id is distinct.
    let mut ids: Vec<String> = specs.iter().map(|(spec, _)| spec.area_id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 27, "area ids must be unique");

    // All four scope issues are represented.
    for issue in [1644u32, 1645, 1646, 1647] {
        assert!(
            specs.iter().any(|(spec, _)| spec.source_issue == issue),
            "no place authored for issue {issue}"
        );
    }
}
