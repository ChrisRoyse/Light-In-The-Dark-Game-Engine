use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        ASHEN_HUMAN_RACE_ID, AttributeBuildInput, AttributeScores, DerivedStatFormulaTable,
        RPG_RACE_SPEC_SCHEMA, RaceAccessRule, RaceAttributeCaps, RaceClassAccess, RaceHometown,
        RaceResistanceProfile, RaceSize, RaceSpec, RaceVision, RaceVisualContract,
        derive_character_stats, persist_and_lower_rpg_authored_content, race_attribute_caps,
        race_spec_content_records, race_spec_readback, validate_race_spec,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WORLD_BIBLE_PATH: &str = "docs/lore/ashen-reach-world-bible.md";
const PEOPLE_LORE_PATH: &str = "docs/lore/ashen-reach-peoples-mourning-legends.md";
const CHARACTER_REF_PATH: &str = "docs/readme/12-character-creation.png";
const RACE_IMPL_PATH: &str = "crates/litd-engine/src/rpg/race_specs.rs";
const RACE_TYPES_PATH: &str = "crates/litd-engine/src/rpg/race_specs_types.rs";
const RACE_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/race_specs_content.rs";
const RACE_TEST_PATH: &str = "crates/litd-engine/src/rpg/race_specs_tests.rs";
const ATTRIBUTES_IMPL_PATH: &str = "crates/litd-engine/src/rpg/attributes.rs";
const ATTRIBUTES_TYPES_PATH: &str = "crates/litd-engine/src/rpg/attributes_types.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue999_human_race_fsv.rs";
const SOURCE_PATHS: [&str; 10] = [
    WORLD_BIBLE_PATH,
    PEOPLE_LORE_PATH,
    CHARACTER_REF_PATH,
    RACE_IMPL_PATH,
    RACE_TYPES_PATH,
    RACE_CONTENT_PATH,
    RACE_TEST_PATH,
    ATTRIBUTES_IMPL_PATH,
    ATTRIBUTES_TYPES_PATH,
    EXAMPLE_PATH,
];

fn human_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ASHEN_HUMAN_RACE_ID.to_string(),
        display_name: "Human".to_string(),
        player_facing_canon_name: "Human".to_string(),
        source_issue: 999,
        canon_issues: vec![985, 999, 1055, 1059],
        mechanics_source_url: "https://wiki.carrionfields.net/race/human".to_string(),
        source_history_label: "CF Human factual mechanics source only".to_string(),
        culture_summary:
            "City-born houses of Galdmere, road families, old noble remnants, and frontier settlers."
                .to_string(),
        stat_caps: AttributeScores::new(20, 20, 20, 20, 20, 20),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: Vec::new(),
        innate_abilities: Vec::new(),
        vision: RaceVision {
            infravision: false,
            infravision_tiles: 0,
        },
        class_access: RaceClassAccess::All,
        experience_penalty_percent: 0,
        hometowns: vec![
            home("origin.galdmere.human_house", "Galdmere House", "Galdmere"),
            home("origin.vhael.road_family", "Road Family", "Galdmere roads"),
            home(
                "origin.ashen_reach.frontier",
                "Reach Frontier",
                "The Ashen Reach",
            ),
        ],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.human.vhael_adventurer".to_string(),
            skeleton_id: "skeleton.biped.medium".to_string(),
            required_animations: vec![
                "idle".to_string(),
                "walk".to_string(),
                "run".to_string(),
                "attack_1h".to_string(),
                "attack_2h".to_string(),
                "cast".to_string(),
                "interact".to_string(),
                "death".to_string(),
            ],
            silhouette: "medium biped, versatile adventurer proportions".to_string(),
            material_notes: "Vhael-original clothing, armor, skin, hair, and road-worn variants; no copied race iconography.".to_string(),
        },
        implementation_notes: vec![
            "Humans may be any class.".to_string(),
            "Humans have no source-listed resistance, vulnerability, innate, or infravision block."
                .to_string(),
            "Player-facing local culture is Vhael-original per #1059.".to_string(),
        ],
    }
}

fn human_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&human_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue999_human_race_spec")?;
    let spec = human_race_spec();
    let readback = race_spec_readback(&spec)?;
    let records = race_spec_content_records(&spec)?;
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
            .starts_with("editor://rpg/ashen_reach/race_specs/")
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
    let stats_probe = attribute_pipeline_probe()?;
    let edges = vec![
        edge("wrong_schema", wrong_schema())?,
        edge("stale_renderer", stale_renderer())?,
        edge("invalid_vision", invalid_vision())?,
        edge("duplicate_resistance", duplicate_resistance())?,
        edge("missing_hometown", missing_hometown())?,
    ];
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
        "schema": "litd.issue999.human_race_fsv.v1",
        "issue": 999,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "mechanics_source_url": "https://wiki.carrionfields.net/race/human",
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect happy_path, stats_probe, Calyx rows, lowered entities, and edge cases against expected_actuals."
        },
        "sufficiency_argument": "The #999 slice is the Human race data row. A single exact row is sufficient because #999 does not require the other #985 race rows. The happy path proves Human source mechanics and Ashen Reach canon fields; the Calyx readback proves the row is editor-authored, persisted, and lowered; the stats probe proves the row feeds the existing attribute pipeline; edge probes prove stale renderer, bad schema, invalid vision, duplicate resistance, and missing hometown fail closed.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "lore_reference_count": source_count_for("docs/lore/"),
            "readme_reference_count": source_count_for("docs/readme/"),
            "race_source_count": source_count_for("crates/litd-engine/src/rpg/race_specs"),
            "attribute_source_count": source_count_for("crates/litd-engine/src/rpg/attributes"),
            "example_source_count": source_count_for("crates/litd-engine/examples/issue999_human_race_fsv.rs"),
            "race_id": ASHEN_HUMAN_RACE_ID,
            "source_issue": 999,
            "stat_caps": {"strength": 20, "intelligence": 20, "wisdom": 20, "dexterity": 20, "constitution": 20, "charisma": 20},
            "alignment": "any",
            "ethos": "any",
            "size": "medium",
            "resistance_count": 0,
            "vulnerability_count": 0,
            "innate_ability_count": 0,
            "infravision": false,
            "class_access": "all",
            "experience_penalty_percent": 0,
            "hometown_count": 3,
            "visual_renderer": "Rust/wgpu",
            "animation_count": 8,
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
            "edge_count": 5,
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "happy_path": readback,
        "calyx": {
            "content_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "stats_probe": stats_probe,
        "edges": edges,
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["lore_reference_count"], 2)?;
    require_i64(&report["expected_actuals"]["readme_reference_count"], 1)?;
    require_i64(&report["expected_actuals"]["race_source_count"], 4)?;
    require_i64(&report["expected_actuals"]["attribute_source_count"], 2)?;
    require_i64(&report["expected_actuals"]["example_source_count"], 1)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 1)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 1)?;
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
        "ISSUE999_HUMAN_RACE_FSV report={} bytes={} sha256={} race={} rows={} anchors={} editor_sources={} lowered={} map_true={} caps={} xp={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["happy_path"]["race_id"],
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["source_anchor_count"],
        readback.json["expected_actuals"]["source_paths_are_editor_authored"],
        readback.json["expected_actuals"]["lowered_artifact_count"],
        readback.json["expected_actuals"]["content_has_map_or_gps_true"],
        readback.json["happy_path"]["stat_caps"],
        readback.json["happy_path"]["experience_penalty_percent"],
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
                "race_id": row.payload["race_id"],
                "race_spec_sha256": row.payload["race_spec_sha256"],
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

fn attribute_pipeline_probe() -> Result<Value, EngineError> {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.galdmere.wayfarer".to_string(),
        race_id: ASHEN_HUMAN_RACE_ID.to_string(),
        level: 5,
        base: AttributeScores::new(20, 20, 20, 20, 20, 20),
        race_caps: vec![human_race_attribute_caps()],
        modifiers: Vec::new(),
    };
    let readback = derive_character_stats(&input, &table)?;
    Ok(json!({
        "race_id": readback.race_id,
        "race_caps": readback.race_caps,
        "effective_attributes": readback.effective_attributes,
        "cap_clamps": readback.cap_clamps,
        "result_sha256": readback.result_sha256,
    }))
}

fn edge(case: &str, spec: litd_engine::rpg::RaceSpec) -> Result<Value, EngineError> {
    let before = spec_hash(&spec)?;
    let error = match validate_race_spec(&spec) {
        Ok(_) => {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid race spec to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = spec_hash(&spec)?;
    Ok(json!({
        "case": case,
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn wrong_schema() -> litd_engine::rpg::RaceSpec {
    let mut spec = human_race_spec();
    spec.schema = "litd.rpg.race_spec.bad".to_string();
    spec
}

fn stale_renderer() -> litd_engine::rpg::RaceSpec {
    let mut spec = human_race_spec();
    spec.visual.renderer = "g3n-native".to_string();
    spec
}

fn invalid_vision() -> litd_engine::rpg::RaceSpec {
    let mut spec = human_race_spec();
    spec.vision.infravision_tiles = 6;
    spec
}

fn duplicate_resistance() -> litd_engine::rpg::RaceSpec {
    let mut spec = human_race_spec();
    spec.resistances = vec![
        RaceResistanceProfile {
            resistance_id: "fire".to_string(),
            amount: 5,
        },
        RaceResistanceProfile {
            resistance_id: "fire".to_string(),
            amount: 7,
        },
    ];
    spec
}

fn missing_hometown() -> litd_engine::rpg::RaceSpec {
    let mut spec = human_race_spec();
    spec.hometowns.clear();
    spec
}

fn spec_hash(spec: &litd_engine::rpg::RaceSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| litd_hash::sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize race spec edge: {error}"),
        })
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue999.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue999.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
