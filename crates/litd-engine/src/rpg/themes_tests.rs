use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        RPG_THEME_SCHEMA, build_theme_catalog_readback, evaluate_character_theme,
        persist_and_lower_rpg_authored_content, theme_content_records,
        validate_rpg_authored_content_records,
    },
    rpg_themes_test_fixture::{
        duplicate_sphere, forbidden_duplicate_tag, happy_catalog, happy_input, happy_rules,
        mismatch_input, missing_sphere_after_deadline, unknown_religion, unknown_role_sphere,
    },
};

#[test]
fn issue964_theme_catalog_counts_spheres_roles_and_religions() {
    let readback = build_theme_catalog_readback(&happy_catalog()).unwrap();

    assert_eq!(readback.schema, RPG_THEME_SCHEMA);
    assert_eq!(readback.sphere_count, 10);
    assert_eq!(readback.role_count, 3);
    assert_eq!(readback.religion_count, 3);
    assert_eq!(readback.deity_sphere_edge_count, 10);
    assert!(readback.theme_tag_count >= 16);
    assert_eq!(readback.input_sha256.len(), 64);
    assert_eq!(readback.result_sha256.len(), 64);
}

#[test]
fn issue964_theme_catalog_persists_as_authored_calyx_content() {
    let catalog = happy_catalog();
    let rules = happy_rules();
    let records = theme_content_records(&catalog, &rules).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("theme-catalog");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let sphere = store
        .get_content_for_editor(EngineContentKind::Lore, "sphere.healing")
        .unwrap();
    let role = store
        .get_content_for_editor(EngineContentKind::Story, "role.sentinel_mender")
        .unwrap();
    let rules_row = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            "server.ashen_reach.themes.rules",
        )
        .unwrap();

    assert_eq!(records.len(), 17);
    assert_eq!(validation.record_count, 17);
    assert_eq!(lowered.row_count, 17);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|entity| {
        entity
            .source_path
            .starts_with("editor://rpg/ashen_reach/themes/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|entity| entity.lowered_field_count > 0)
    );
    assert_eq!(sphere.payload["surface_kind"], "theme_sphere");
    assert_eq!(sphere.payload["sphere_id"], "sphere.healing");
    assert_eq!(role.payload["surface_kind"], "role_theme");
    assert_eq!(rules_row.payload["surface_kind"], "theme_rules");
    for record in records {
        assert_ne!(record.payload["built_in_map_required"], true);
        assert_ne!(record.payload["grants_map_or_minimap"], true);
        assert_ne!(record.payload["grants_gps_markers"], true);
    }
}

#[test]
fn issue964_character_theme_accepts_matching_role_and_religion() {
    let readback =
        evaluate_character_theme(&happy_input(), &happy_catalog(), &happy_rules()).unwrap();
    let religion = readback.religion_match.as_ref().unwrap();

    assert_eq!(readback.schema, RPG_THEME_SCHEMA);
    assert_eq!(readback.sphere_deadline_level, 10);
    assert!(readback.sphere_selection_required);
    assert_eq!(readback.role_sphere_match, "primary");
    assert_eq!(religion.religion_id, "religion.dawn_court");
    assert!(religion.class_requires_empowerment);
    assert!(religion.direct_sphere_match);
    assert_eq!(religion.score, 85);
    assert_eq!(religion.grade, "accepted");
    assert!(religion.religion_fit_accepted);
    assert!(religion.empowerment_fit_accepted);
    assert!(religion.missing_required_tags.is_empty());
    assert!(religion.forbidden_tags_present.is_empty());
}

#[test]
fn issue964_theme_match_reports_religion_mismatch_without_fallback() {
    let readback =
        evaluate_character_theme(&mismatch_input(), &happy_catalog(), &happy_rules()).unwrap();
    let religion = readback.religion_match.as_ref().unwrap();

    assert_eq!(religion.religion_id, "religion.ember_crown");
    assert!(!religion.direct_sphere_match);
    assert_eq!(religion.grade, "sphere_mismatch");
    assert!(!religion.religion_fit_accepted);
    assert!(!religion.empowerment_fit_accepted);
    assert!(religion.score < 60);
}

#[test]
fn issue964_themes_fail_closed_on_invalid_edges() {
    assert_code(duplicate_sphere(), "LITD-RPG-THEME-INVALID-CONFIG");
    assert_code(unknown_role_sphere(), "LITD-RPG-THEME-INVALID-CONFIG");
    assert_code(
        missing_sphere_after_deadline(),
        "LITD-RPG-THEME-INVALID-CONFIG",
    );
    assert_code(unknown_religion(), "LITD-RPG-THEME-INVALID-CONFIG");
    assert_code(forbidden_duplicate_tag(), "LITD-RPG-THEME-INVALID-CONFIG");
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
    crate::test_temp::TestTempDir::new(&format!("issue964-{label}"))
}
