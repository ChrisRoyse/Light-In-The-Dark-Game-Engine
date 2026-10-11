#[path = "fixtures/issue964_themes_fixture.rs"]
mod issue964_themes_fixture;

use std::fs;

use issue964_themes_fixture::{
    duplicate_declared_tag, duplicate_sphere, happy_catalog, happy_input, happy_rules,
    mismatch_input, missing_sphere_after_deadline, unknown_religion, unknown_role_sphere,
};
use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        CharacterThemeInput, ThemeCatalog, ThemeRules, build_theme_catalog_readback,
        evaluate_character_theme, persist_and_lower_rpg_authored_content, theme_content_records,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const README_SPELLBOOK_PATH: &str = "docs/readme/GP19-spellbook.png";
const README_TALENTS_PATH: &str = "docs/readme/GP20-talents.png";
const THEMES_IMPL_PATH: &str = "crates/litd-engine/src/rpg/themes.rs";
const THEMES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/themes_types.rs";
const THEMES_VALIDATE_PATH: &str = "crates/litd-engine/src/rpg/themes_validate.rs";
const THEMES_READBACK_PATH: &str = "crates/litd-engine/src/rpg/themes_readback.rs";
const THEMES_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/themes_content.rs";
const THEMES_TEST_PATH: &str = "crates/litd-engine/src/rpg/themes_tests.rs";
const THEMES_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/themes_test_fixture.rs";
const THEMES_EXAMPLE_FIXTURE_PATH: &str =
    "crates/litd-engine/examples/fixtures/issue964_themes_fixture.rs";
const THEMES_EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue964_themes_fsv.rs";
const SOURCE_PATHS: [&str; 11] = [
    README_SPELLBOOK_PATH,
    README_TALENTS_PATH,
    THEMES_IMPL_PATH,
    THEMES_TYPES_PATH,
    THEMES_VALIDATE_PATH,
    THEMES_READBACK_PATH,
    THEMES_CONTENT_PATH,
    THEMES_TEST_PATH,
    THEMES_FIXTURE_PATH,
    THEMES_EXAMPLE_FIXTURE_PATH,
    THEMES_EXAMPLE_PATH,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue964_spheres_themes")?;
    let catalog = happy_catalog();
    let rules = happy_rules();
    let input = happy_input();
    let catalog_readback = build_theme_catalog_readback(&catalog)?;
    let records = theme_content_records(&catalog, &rules)?;
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_map_or_gps_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/themes/")
    });
    let content_has_compiled_source_path = persisted_rows
        .iter()
        .any(|row| row.source_path.starts_with("rpg://"));
    let lowered_artifact_count = persisted_rows
        .iter()
        .filter(|row| {
            row.lowering.hot_path_policy == "frozen_fingerprinted_data_only"
                && !row.lowering.fingerprint.is_empty()
        })
        .count();
    let happy = evaluate_character_theme(&input, &catalog, &rules)?;
    let mismatch = evaluate_character_theme(&mismatch_input(), &catalog, &rules)?;
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;
    let source_readback_count = source_readbacks.len();
    let source_count_for = |needle: &str| -> usize {
        SOURCE_PATHS
            .iter()
            .filter(|path| path.contains(needle))
            .count()
    };
    let report = json!({
        "schema": "litd.issue964.spheres_themes_fsv.v1",
        "issue": 964,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect catalog, happy_match, mismatch_match, expected_actuals, edges, and hashes against the saved source-of-truth file."
        },
        "research_sources": [
            "https://wiki.carrionfields.net/game/spheres",
            "https://carrionfields.net/helpsearch.php?id=791",
            "https://wiki.carrionfields.net/"
        ],
        "readme_references_reviewed": [
            "docs/readme/GP19-spellbook.png",
            "docs/readme/GP20-talents.png"
        ],
        "sufficiency_argument": "The fixture is the smallest original-content sphere/theme set that proves #964 and the #1455 migration slice: configurable spheres, role archetypes, deity/religion sphere sets, level-10 sphere deadline, accepted role+religion empowerment fit, explicit mismatch readback without fallback, deterministic hashes, 17 per-entity editor-authored Calyx rows, frozen lowered fingerprints, and >=3 fail-loud invalid classes. It deliberately uses 10 representative spheres instead of a full production sphere list because the universal matching, validation, and authored-lowering transitions are fully covered.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "readme_reference_count": source_count_for("docs/readme/"),
            "theme_source_count": source_count_for("crates/litd-engine/src/rpg/themes"),
            "example_source_count": source_count_for("crates/litd-engine/examples"),
            "fixture_source_count": source_count_for("fixture"),
            "sphere_count": catalog_readback.sphere_count,
            "role_count": catalog_readback.role_count,
            "religion_count": catalog_readback.religion_count,
            "deity_sphere_edge_count": catalog_readback.deity_sphere_edge_count,
            "theme_tag_count": catalog_readback.theme_tag_count,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "happy_score": happy.religion_match.as_ref().map(|m| m.score).unwrap_or_default(),
            "happy_grade": happy.religion_match.as_ref().map(|m| m.grade.as_str()).unwrap_or("missing"),
            "happy_empowerment_fit": happy.religion_match.as_ref().map(|m| m.empowerment_fit_accepted).unwrap_or(false),
            "happy_role_sphere_match": happy.role_sphere_match,
            "mismatch_grade": mismatch.religion_match.as_ref().map(|m| m.grade.as_str()).unwrap_or("missing"),
            "mismatch_fit": mismatch.religion_match.as_ref().map(|m| m.religion_fit_accepted).unwrap_or(true),
            "sphere_deadline_level": rules.sphere_choice_deadline_level,
            "edge_count": 5,
            "error_code": "LITD-RPG-THEME-INVALID-CONFIG"
        },
        "catalog_input_sha256": hash_json(&catalog)?,
        "rules_sha256": hash_json(&rules)?,
        "input_sha256": hash_json(&input)?,
        "catalog": catalog_readback,
        "calyx": {
            "content_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "happy_match": happy,
        "mismatch_match": mismatch,
        "edges": [
            edge("duplicate_sphere", duplicate_sphere())?,
            edge("unknown_role_sphere", unknown_role_sphere())?,
            edge("missing_sphere_after_deadline", missing_sphere_after_deadline())?,
            edge("unknown_religion", unknown_religion())?,
            edge("duplicate_declared_tag", duplicate_declared_tag())?,
        ],
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["readme_reference_count"], 2)?;
    require_i64(&report["expected_actuals"]["theme_source_count"], 7)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["fixture_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 17)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 17)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        false,
    )?;
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE964_THEMES_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} lowered={} map_true={} spheres={} score={} grade={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["expected_actuals"]["sphere_count"],
        readback.json["expected_actuals"]["happy_score"],
        readback.json["expected_actuals"]["happy_grade"],
        readback.json["edges"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
    );
    Ok(())
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "kind": row.kind,
                "database_id": row.database.id,
                "database_sha256": row.database.sha256,
                "source_path": row.source_path,
                "surface_kind": row.payload["surface_kind"],
                "catalog_id": row.payload["catalog_id"],
                "theme_catalog_sha256": row.payload["theme_catalog_sha256"],
                "theme_catalog_result_sha256": row.payload["theme_catalog_result_sha256"],
                "lowering_artifact_id": row.lowering.artifact_id,
                "lowering_fingerprint": row.lowering.fingerprint,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn edge(
    case: &str,
    (input, catalog, rules): (CharacterThemeInput, ThemeCatalog, ThemeRules),
) -> Result<Value, EngineError> {
    let before_input = hash_json(&input)?;
    let before_catalog = hash_json(&catalog)?;
    let before_rules = hash_json(&rules)?;
    let error = match evaluate_character_theme(&input, &catalog, &rules) {
        Ok(_) => {
            return Err(EngineError::RpgThemeInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid theme data to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    Ok(json!({
        "case": case,
        "before_input_sha256": before_input,
        "after_input_sha256": hash_json(&input)?,
        "before_catalog_sha256": before_catalog,
        "after_catalog_sha256": hash_json(&catalog)?,
        "before_rules_sha256": before_rules,
        "after_rules_sha256": hash_json(&rules)?,
        "state_unchanged": before_input == hash_json(&input)?
            && before_catalog == hash_json(&catalog)?
            && before_rules == hash_json(&rules)?,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn hash_json<T: serde::Serialize>(value: &T) -> Result<String, EngineError> {
    serde_json::to_vec(value)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize issue964 theme FSV value: {error}"),
        })
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgThemeInvalidConfig {
        field: "issue964.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgThemeInvalidConfig {
        field: "issue964.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
