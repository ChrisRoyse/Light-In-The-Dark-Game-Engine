// Parameterized readback tests across every AreaSpec authored by the
// #1649-#1660 batch (~57 places). Exercises the happy-path invariants once per
// spec rather than hand-writing ~57 near-identical test functions.

use std::collections::BTreeSet;

use crate::rpg::{area_spec_readback, validate_area_spec};

use super::regions_thera_fixture_underdark::ARIN_GIRD_AREA_ID;
use super::regions_thera_fixture_underdark_b::SVIRFNEBLIN_ENCLAVE_AREA_ID;
use super::regions_thera_registry::{all_region_specs, batch_source_issues};

#[test]
fn issue1649_1660_every_spec_validates_and_reads_back_cleanly() {
    let specs = all_region_specs();
    assert!(
        specs.len() >= 50,
        "expected ~57 authored places, got {}",
        specs.len()
    );

    let issues = batch_source_issues();
    let mut seen_area_ids = BTreeSet::new();

    for spec in &specs {
        validate_area_spec(spec).unwrap_or_else(|e| {
            panic!("spec {} failed validation: {e}", spec.area_id);
        });
        let readback = area_spec_readback(spec).unwrap();

        assert!(
            seen_area_ids.insert(spec.area_id.clone()),
            "duplicate area_id across batch: {}",
            spec.area_id
        );
        assert!(
            issues.contains(&spec.source_issue),
            "spec {} has source_issue {} outside this batch's #1649-#1660 range",
            spec.area_id,
            spec.source_issue
        );
        assert!(
            spec.canon_issues.contains(&987) && spec.canon_issues.contains(&1632),
            "spec {} canon_issues must include 987 and 1632",
            spec.area_id
        );
        assert!(
            spec.canon_issues.contains(&spec.source_issue),
            "spec {} canon_issues must include its own source_issue",
            spec.area_id
        );
        assert!(
            spec.mechanics_source_url
                .starts_with("https://wiki.carrionfields.net/"),
            "spec {} mechanics_source_url must be CF-sourced",
            spec.area_id
        );
        assert!(
            readback.room_count >= 1,
            "spec {} has no rooms",
            spec.area_id
        );
        assert_eq!(readback.visual_renderer, "Rust/wgpu");
        assert!(
            readback.prop_count >= 3,
            "spec {} has fewer than 3 required props",
            spec.area_id
        );
        assert_eq!(readback.area_spec_sha256.len(), 64);
        // Wilderness/region doctrine: none of these places are a hometown option
        // except the two CF-factual hometown exceptions (Arin-gird duergar #1006,
        // Svirfneblin enclave #1008).
        let is_hometown_exception =
            spec.area_id == ARIN_GIRD_AREA_ID || spec.area_id == SVIRFNEBLIN_ENCLAVE_AREA_ID;
        assert_eq!(
            readback.is_hometown_option, is_hometown_exception,
            "spec {} hometown flag mismatch",
            spec.area_id
        );
        if is_hometown_exception {
            assert!(
                readback.has_recall_point,
                "hometown {} needs a recall point",
                spec.area_id
            );
        }
    }
}

#[test]
fn issue1649_1660_every_room_id_and_connection_id_is_globally_unique() {
    let specs = all_region_specs();
    let mut room_ids = BTreeSet::new();
    let mut connection_ids = BTreeSet::new();
    for spec in &specs {
        for room in &spec.rooms {
            assert!(
                room_ids.insert(room.room_id.clone()),
                "duplicate room_id across batch: {}",
                room.room_id
            );
        }
        for conn in &spec.connections {
            assert!(
                connection_ids.insert(conn.connection_id.clone()),
                "duplicate connection_id across batch: {}",
                conn.connection_id
            );
        }
    }
    assert!(room_ids.len() >= 100);
    assert!(connection_ids.len() >= 100);
}
