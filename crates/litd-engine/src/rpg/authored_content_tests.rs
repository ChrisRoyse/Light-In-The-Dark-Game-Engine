use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        DeriveInfernalDeepsPackageArgs, InfernalBossAgentPolicy, InfernalDeepsPackagePolicy,
        InfernalRelicVaultPolicy, WitheringRaidPathPolicy, ability_content_records,
        derive_infernal_deeps_package, infernal_access_content_records,
        infernal_attrition_content_records, infernal_deeps_content_records,
        infernal_zone_content_records, persist_and_lower_rpg_authored_content,
        race_spec_content_records, rpg_authored_payload_without_nulls, theme_content_records,
        unhallowed_knight_content_records, validate_rpg_authored_content_records,
    },
    rpg_abilities_test_fixture::happy_catalog as happy_ability_catalog,
    rpg_infernal_access_test_fixture::ashen_reach_infernal_access_policy,
    rpg_infernal_attrition_test_fixture::ashen_reach_infernal_attrition_policy,
    rpg_infernal_zones_test_fixture::ashen_reach_infernal_zone_policy,
    rpg_race_specs_test_fixture::human_race_spec,
    rpg_themes_test_fixture::{happy_catalog, happy_rules},
    rpg_undeath_test_fixture::ashen_reach_undeath_policy,
    rpg_unhallowed_knight_test_fixture::ashen_reach_unhallowed_knight_policy,
};

#[test]
fn issue1455_infernal_deeps_authored_records_persist_and_lower_per_surface() {
    let records = infernal_deeps_records();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    assert_eq!(validation.record_count, 6);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(
        records
            .iter()
            .all(|record| record.payload.get("readback").is_none())
    );

    let (store, _vault) = content_store("infernal-deeps-authored");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(lowered.row_count, 6);
    assert!(!lowered.package_fingerprint.is_empty());
    assert!(!lowered.calyx_live_query_allowed_in_hot_path);
    assert_eq!(lowered.validation.record_count, 6);
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_deeps/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    assert!(lowered.entities.iter().any(|row| {
        row.kind == EngineContentKind::Terrain && row.surface_kind == "infernal_deeps_zone_topology"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.kind == EngineContentKind::Agent && row.surface_kind == "infernal_deeps_boss_hierarchy"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.kind == EngineContentKind::Item && row.surface_kind == "infernal_deeps_relic_vaults"
    }));
}

#[test]
fn issue1455_infernal_access_zones_and_attrition_records_are_authored() {
    let access = infernal_access_content_records(&ashen_reach_infernal_access_policy()).unwrap();
    let attrition_policy = ashen_reach_infernal_attrition_policy();
    let attrition = infernal_attrition_content_records(&attrition_policy).unwrap();
    let zones =
        infernal_zone_content_records(&ashen_reach_infernal_zone_policy(), &attrition_policy)
            .unwrap();
    let mut records = Vec::new();
    records.extend(access);
    records.extend(attrition);
    records.extend(zones);
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("infernal-access-zones-attrition");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 23);
    assert_eq!(lowered.row_count, 23);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_access/")
            && row.surface_kind == "infernal_access_entry_rule"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_zones/")
            && row.surface_kind == "infernal_zone_node"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_attrition/")
            && row.surface_kind == "infernal_attrition_circle_profile"
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
}

#[test]
fn issue1455_human_race_spec_record_is_authored() {
    let records = race_spec_content_records(&human_race_spec()).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("human-race-spec");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 1);
    assert_eq!(lowered.row_count, 1);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records[0].anchors.is_empty());
    assert!(
        lowered.entities[0]
            .source_path
            .starts_with("editor://rpg/ashen_reach/race_specs/")
    );
    assert_eq!(lowered.entities[0].surface_kind, "race_spec");
    assert_eq!(lowered.entities[0].kind, EngineContentKind::Unit);
    assert!(lowered.entities[0].lowered_field_count > 0);
}

#[test]
fn issue1455_theme_catalog_records_are_authored() {
    let records = theme_content_records(&happy_catalog(), &happy_rules()).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("theme-catalog");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 17);
    assert_eq!(lowered.row_count, 17);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/themes/rules/")
            && row.surface_kind == "theme_rules"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/themes/spheres/")
            && row.surface_kind == "theme_sphere"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/themes/roles/")
            && row.surface_kind == "role_theme"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/themes/religions/")
            && row.surface_kind == "religion_theme"
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
}

#[test]
fn issue1455_ability_catalog_records_are_authored() {
    let records = ability_content_records(&happy_ability_catalog()).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("ability-catalog");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 8);
    assert_eq!(lowered.row_count, 8);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/abilities/catalog/")
            && row.surface_kind == "ability_catalog"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/abilities/abilities/")
            && row.surface_kind == "ability_definition"
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
}

#[test]
fn issue1455_unhallowed_knight_records_are_authored() {
    let records =
        unhallowed_knight_content_records(&ashen_reach_unhallowed_knight_policy()).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _vault) = content_store("unhallowed-knight");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 8);
    assert_eq!(lowered.row_count, 8);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert_eq!(
        lowered
            .entities
            .iter()
            .filter(|row| row.kind == EngineContentKind::Ability)
            .count(),
        6
    );
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/unhallowed_knight/")
            && row.surface_kind == "class_spec"
    }));
    assert!(lowered.entities.iter().any(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/unhallowed_knight/")
            && row.surface_kind == "soul_weapon_rules"
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
}

#[test]
fn issue1455_authored_content_rejects_compiled_source_and_testpass_anchor() {
    let mut record = authored_record("lore.bad.source", json!({"known_value": 1}));
    record.source.path = "rpg://infernal_deeps/lore.bad.source".to_string();
    assert_invalid(
        validate_rpg_authored_content_records(&[record]),
        "source.path",
    );

    let mut record = authored_record("lore.bad.anchor", json!({"known_value": 1}));
    record.anchors.push(Anchor {
        kind: AnchorKind::TestPass,
        value: AnchorValue::Bool(true),
        source: "self".to_string(),
        observed_at: 1_786_000_000,
        confidence: 1.0,
    });
    assert_invalid(validate_rpg_authored_content_records(&[record]), "anchors");
}

#[test]
fn issue1455_authored_content_rejects_free_map_but_allows_acquired_map_source() {
    let free_map = authored_record(
        "lore.bad.free-map",
        json!({
            "known_value": 1,
            "built_in_map_required": true,
        }),
    );
    assert_invalid(
        validate_rpg_authored_content_records(&[free_map]),
        "/built_in_map_required",
    );

    let acquired = authored_record(
        "item.map.vhael.vellum_of_ember_roads",
        json!({
            "known_value": 1,
            "acquired_map_source": {
                "source_kind": "cartographer_purchase",
                "scope_id": "region.vhael.galdmere_east_road",
                "requires_possession_or_knowledge": true,
                "default_available": false,
                "player_position_indicator": false,
                "quest_markers": false,
                "route_lines": false,
                "minimap_hud": false
            },
        }),
    );
    let validation = validate_rpg_authored_content_records(&[acquired]).unwrap();
    assert_eq!(validation.acquired_map_record_count, 1);
    assert!(validation.no_builtin_map_runtime);
}

#[test]
fn issue1455_authored_content_rejects_null_and_sanitizes_absent_optional_fields() {
    let raw_null = authored_record(
        "lore.bad.null",
        json!({
            "known_value": 1,
            "optional_absent": null,
        }),
    );
    assert_invalid(
        validate_rpg_authored_content_records(&[raw_null]),
        "/optional_absent",
    );

    let mut sanitized = authored_record(
        "lore.good.sanitized-null",
        json!({
            "known_value": 1,
            "optional_absent": null,
        }),
    );
    sanitized.payload = rpg_authored_payload_without_nulls(sanitized.payload);
    sanitized.source.source_hash = payload_sha256(&sanitized.payload);

    assert!(sanitized.payload.get("optional_absent").is_none());
    validate_rpg_authored_content_records(&[sanitized]).unwrap();
}

#[test]
fn issue1455_authored_field_edit_changes_lowered_fingerprint_without_recompile() {
    let records = infernal_deeps_records();
    let (store, _vault) = content_store("infernal-deeps-original");
    let original = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    let mut edited_records = records.clone();
    let relic = edited_records
        .iter_mut()
        .find(|record| record.id == "server.ashen_reach.infernal_deeps.relic_vaults")
        .unwrap();
    relic.payload["highest_power_rating_milli"] = json!(975);
    relic.source.source_hash = payload_sha256(&relic.payload);

    let (store, _vault) = content_store("infernal-deeps-edited");
    let edited = persist_and_lower_rpg_authored_content(&store, &edited_records).unwrap();

    let original_relic =
        fingerprint_for(&original, "server.ashen_reach.infernal_deeps.relic_vaults");
    let edited_relic = fingerprint_for(&edited, "server.ashen_reach.infernal_deeps.relic_vaults");
    assert_ne!(original.package_fingerprint, edited.package_fingerprint);
    assert_ne!(original_relic, edited_relic);
}

fn infernal_deeps_records() -> Vec<EngineContentRecord> {
    let package = InfernalDeepsPackagePolicy::ashen_reach_defaults();
    let readback = derive_infernal_deeps_package(DeriveInfernalDeepsPackageArgs {
        package: &package,
        access: &ashen_reach_infernal_access_policy(),
        attrition: &ashen_reach_infernal_attrition_policy(),
        zones: &ashen_reach_infernal_zone_policy(),
        bosses: &InfernalBossAgentPolicy::ashen_reach_defaults(),
        relics: &InfernalRelicVaultPolicy::ashen_reach_defaults(),
        withering: &WitheringRaidPathPolicy::ashen_reach_defaults(),
        undeath: &ashen_reach_undeath_policy(),
    })
    .unwrap();
    infernal_deeps_content_records(&package, &readback).unwrap()
}

fn authored_record(id: &str, payload_fields: Value) -> EngineContentRecord {
    let mut payload = json!({
        "schema": "litd.rpg.test_authored_content.v1",
        "surface_kind": "test_authored_entity",
        "source_issue": 1455,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
    });
    let object = payload.as_object_mut().unwrap();
    for (key, value) in payload_fields.as_object().unwrap() {
        object.insert(key.clone(), value.clone());
    }
    EngineContentRecord {
        kind: EngineContentKind::Lore,
        id: id.to_string(),
        display_name: id.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/test/{id}"),
            author: "editor_authoring.issue1455.test".to_string(),
            source_hash: payload_sha256(&payload),
        },
        payload,
        anchors: Vec::new(),
    }
}

fn fingerprint_for(readback: &crate::rpg::RpgAuthoredContentLoweringReadback, id: &str) -> String {
    readback
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .unwrap()
        .lowered_package_fingerprint
        .clone()
}

fn assert_invalid<T: std::fmt::Debug>(
    result: Result<T, crate::rpg::RpgAuthoredContentError>,
    expected_field: &str,
) {
    match result {
        Err(crate::rpg::RpgAuthoredContentError::Invalid { field, .. }) => {
            assert!(field.contains(expected_field), "{field}");
        }
        other => panic!("expected invalid authored content at {expected_field}, got {other:?}"),
    }
}

fn payload_sha256(payload: &Value) -> String {
    sha256_hex(&serde_json::to_vec(payload).unwrap())
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
    crate::test_temp::TestTempDir::new(&format!("issue1455-{label}"))
}
