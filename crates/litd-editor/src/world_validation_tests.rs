use litd_engine::math::{Transform, Vec3};

use super::*;
use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
};

const ROCK_MODEL: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

#[test]
fn clean_real_asset_world_reports_no_issues() {
    let document = clean_world();
    let before = "unvalidated";
    let report = validate_editor_world(&document, WorldValidationUseCase::Play);
    println!(
        "issue1218 clean before={before} after blocking={} errors={} summary={} objects={} products={}",
        report.blocking,
        report.error_count,
        report.summary,
        document.objects.len(),
        document.asset_products.len()
    );
    assert!(!report.blocking);
    assert_eq!(report.issue_count, 0);
    require_editor_world_valid(&document, WorldValidationUseCase::Play).unwrap();
}

#[test]
fn multiple_defects_are_all_listed_in_one_report() {
    let mut document = clean_world();
    document.objects.remove("world");
    document.objects.insert(
        "dangling_child".to_string(),
        EditorObject::new(
            "dangling_child",
            "Doodad",
            Some("missing_parent".to_string()),
            Transform::IDENTITY,
        )
        .with_property("target_region", "missing_region"),
    );
    document
        .objects
        .get_mut("unit_playtest_hero")
        .unwrap()
        .properties
        .clear();

    let before = "world removed; dangling parent+region; unit model cleared";
    let report = validate_editor_world(&document, WorldValidationUseCase::Play);
    println!(
        "issue1218 multi before={before} after blocking={} errors={} summary={}",
        report.blocking, report.error_count, report.summary
    );
    let codes = issue_codes(&report);
    assert!(report.blocking);
    assert!(has_code(&codes, "LITD-VAL-WORLD-MISSING"));
    assert!(has_code(&codes, "LITD-VAL-DANGLING-PARENT"));
    assert!(has_code(&codes, "LITD-VAL-DANGLING-REGION-REF"));
    assert!(has_code(&codes, "LITD-VAL-UNIT-MISSING-MODEL"));
    let error = require_editor_world_valid(&document, WorldValidationUseCase::Play)
        .unwrap_err()
        .to_string();
    assert!(error.contains("editor.validation.blocking"));
}

#[test]
fn quest_soft_lock_reports_fix_oriented_issue() {
    let mut document = clean_world();
    document.objects.insert(
        "quest_ash_gate".to_string(),
        EditorObject::new(
            "quest_ash_gate",
            "Quest",
            Some("world".to_string()),
            Transform::IDENTITY,
        )
        .with_property("start_object", "unit_playtest_hero")
        .with_property("complete_object", "missing_npc")
        .with_property("required_item", "missing_key"),
    );

    let before = "quest has missing complete_object and required_item";
    let report = validate_editor_world(&document, WorldValidationUseCase::Publish);
    println!(
        "issue1218 softlock before={before} after blocking={} errors={} summary={}",
        report.blocking, report.error_count, report.summary
    );
    let codes = issue_codes(&report);
    assert!(has_code(&codes, "LITD-VAL-QUEST-DANGLING-ENDPOINT"));
    assert!(has_code(&codes, "LITD-VAL-QUEST-UNOBTAINABLE-ITEM"));
    assert!(report.summary.contains("create object missing_npc"));
}

#[test]
fn publish_requires_playable_spawn_state() {
    let mut document = clean_world();
    document.objects.remove("unit_playtest_hero");

    let before = "no Unit objects";
    let report = validate_editor_world(&document, WorldValidationUseCase::Publish);
    println!(
        "issue1218 publish_block before={before} after blocking={} errors={} summary={}",
        report.blocking, report.error_count, report.summary
    );
    let codes = issue_codes(&report);
    assert!(has_code(&codes, "LITD-VAL-PLAY-NO-UNITS"));
    assert!(require_editor_world_valid(&document, WorldValidationUseCase::Publish).is_err());
}

#[test]
fn invalid_unit_damage_blocks_play_with_clear_reason() {
    let mut document = clean_world();
    document
        .objects
        .get_mut("unit_playtest_hero")
        .unwrap()
        .properties
        .insert("stat.damage".to_string(), "0".to_string());

    let report = validate_editor_world(&document, WorldValidationUseCase::Play);
    println!(
        "issue738 invalid_damage blocking={} errors={} summary={}",
        report.blocking, report.error_count, report.summary
    );

    let codes = issue_codes(&report);
    assert!(report.blocking);
    assert!(has_code(&codes, "LITD-VAL-UNIT-DAMAGE-INVALID"));
    let error = require_editor_world_valid(&document, WorldValidationUseCase::Play)
        .unwrap_err()
        .to_string();
    assert!(error.contains("stat.damage"));
    assert!(error.contains("positive integer"));
}

fn clean_world() -> EditorDocument {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let record = catalog
        .records()
        .iter()
        .find(|record| record.relative_path == ROCK_MODEL)
        .expect("real asset vault must contain Quaternius Rock1 for validation tests");
    let mut document =
        EditorDocument::new("calyx://world/validation-clean", "validation-clean-rev").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_playtest_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", record).unwrap();
    document
}

fn issue_codes(report: &WorldValidationReport) -> Vec<&str> {
    report
        .issues
        .iter()
        .map(|issue| issue.code.as_str())
        .collect()
}

fn has_code(codes: &[&str], expected: &str) -> bool {
    codes.contains(&expected)
}
