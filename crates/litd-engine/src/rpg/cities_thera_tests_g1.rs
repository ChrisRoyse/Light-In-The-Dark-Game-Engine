// Tests for #1637 Voralian City, #1638 Hamsah Mu-tazz, #1639 Udgaard, and
// #1640 Seantryn Modan. Each combined test mirrors the galadon precedent
// (area_specs_tests.rs): readback facts against source, persist ->
// SEPARATE readback via get_content_for_editor with stored==authored, and
// registry referential integrity. The fail-closed-edges coverage for all
// seven cities is consolidated into one shared parameterized test in
// cities_thera_tests_g2.rs.
use std::collections::BTreeSet;

use crate::{
    calyx_content::EngineContentKind,
    rpg::{
        AreaKind, area_spec_content_records, area_spec_readback,
        persist_and_lower_rpg_authored_content, validate_area_connections_against_registry,
        validate_rpg_authored_content_records,
    },
};

use super::rpg_cities_thera_fixture_hamsah::{
    HAMSAH_AREA_ID, hamsah_area_spec, hamsah_known_area_ids,
};
use super::rpg_cities_thera_fixture_seantryn::{
    SEANTRYN_MODAN_AREA_ID, seantryn_area_spec, seantryn_known_area_ids,
};
use super::rpg_cities_thera_fixture_support::content_store;
use super::rpg_cities_thera_fixture_udgaard::{
    UDGAARD_AREA_ID, udgaard_area_spec, udgaard_known_area_ids,
};
use super::rpg_cities_thera_fixture_voralian::{
    VORALIAN_AREA_ID, voralian_area_spec, voralian_known_area_ids,
};

#[test]
fn issue1637_voralian_area_spec_matches_facts_persists_and_is_registry_sound() {
    let spec = voralian_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, VORALIAN_AREA_ID);
    assert_eq!(readback.display_name, "Voralian City");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1637);
    assert!(readback.guildhall_count >= 3);
    assert!(readback.is_hometown_option);
    assert!(readback.has_academy);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);

    let (records, lowered, summary) = persist_and_readback(&spec, "voralian-area-spec");
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(summary.id, VORALIAN_AREA_ID);
    assert_eq!(lowered.row_count, expected_rows);

    let known: BTreeSet<&str> = voralian_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

#[test]
fn issue1638_hamsah_area_spec_matches_facts_persists_and_is_registry_sound() {
    let spec = hamsah_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, HAMSAH_AREA_ID);
    assert_eq!(readback.display_name, "Hamsah Mu-tazz");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1638);
    assert!(readback.guildhall_count >= 3);
    assert!(readback.is_hometown_option);
    assert!(readback.has_academy);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);

    let (records, lowered, summary) = persist_and_readback(&spec, "hamsah-area-spec");
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(summary.id, HAMSAH_AREA_ID);
    assert_eq!(lowered.row_count, expected_rows);

    let known: BTreeSet<&str> = hamsah_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

#[test]
fn issue1639_udgaard_area_spec_matches_facts_persists_and_is_registry_sound() {
    let spec = udgaard_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, UDGAARD_AREA_ID);
    assert_eq!(readback.display_name, "Udgaard");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1639);
    assert!(readback.guildhall_count >= 2);
    assert!(readback.is_hometown_option);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);

    let (records, lowered, summary) = persist_and_readback(&spec, "udgaard-area-spec");
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(summary.id, UDGAARD_AREA_ID);
    assert_eq!(lowered.row_count, expected_rows);

    let known: BTreeSet<&str> = udgaard_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

#[test]
fn issue1640_seantryn_area_spec_matches_facts_persists_and_is_registry_sound() {
    let spec = seantryn_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, SEANTRYN_MODAN_AREA_ID);
    assert_eq!(readback.display_name, "Seantryn Modan");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1640);
    assert!(readback.guildhall_count >= 2);
    assert!(readback.has_sewers);
    assert!(readback.is_hometown_option);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);

    let (records, lowered, summary) = persist_and_readback(&spec, "seantryn-area-spec");
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(summary.id, SEANTRYN_MODAN_AREA_ID);
    assert_eq!(lowered.row_count, expected_rows);

    let known: BTreeSet<&str> = seantryn_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

/// Shared persist -> SEPARATE readback via `get_content_for_editor`, asserting the
/// acquired-map doctrine (#944) flags and editor-authored source path, mirroring the
/// galadon precedent (`issue1636_galadon_area_spec_persists_as_authored_calyx_content`).
pub(crate) fn persist_and_readback(
    spec: &crate::rpg::AreaSpec,
    label: &str,
) -> (
    Vec<crate::calyx_content::EngineContentRecord>,
    crate::rpg::RpgAuthoredContentLoweringReadback,
    crate::calyx_content::EngineContentConstellationReadback,
) {
    let records = area_spec_content_records(spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    let (store, _vault) = content_store(label);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let summary = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(summary.kind, EngineContentKind::Terrain);
    assert_eq!(summary.payload["surface_kind"], "area_spec");
    assert_ne!(summary.payload["built_in_map_required"], true);
    assert_ne!(summary.payload["grants_map_or_minimap"], true);
    assert_ne!(summary.payload["grants_gps_markers"], true);
    assert!(
        summary
            .source_path
            .starts_with("editor://rpg/thera/area_specs/")
    );
    assert!(lowered.entities[0].lowered_field_count > 0);
    // Leak the vault guard's lifetime into the caller by dropping it here is fine: the
    // store itself only needs the temp dir to remain until this fn returns, matching the
    // one-shot persist+readback pattern used throughout these tests.
    (records, lowered, summary)
}
