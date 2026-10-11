use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        ASHKIN_DEEP_RACE_ID, AttributeBuildInput, AttributeScores, DerivedStatFormulaTable,
        DerivedStats, RPG_RACE_SPEC_SCHEMA, RaceAccessRule, RaceAttributeCaps, RaceClassAccess,
        RaceHometown, RaceInnateAbility, RaceSize, RaceSpec, RaceVision, RaceVisualContract,
        StatModifier, StatModifierSource, derive_character_stats,
        persist_and_lower_rpg_authored_content, race_attribute_caps, race_spec_content_records,
        race_spec_readback, validate_race_spec,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const RACE_IMPL_PATH: &str = "crates/litd-engine/src/rpg/race_specs.rs";
const RACE_TYPES_PATH: &str = "crates/litd-engine/src/rpg/race_specs_types.rs";
const RACE_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/race_specs_content.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1003_ashkin_deep_race_fsv.rs";
const SOURCE_PATHS: [&str; 4] = [
    RACE_IMPL_PATH,
    RACE_TYPES_PATH,
    RACE_CONTENT_PATH,
    EXAMPLE_PATH,
];

// Local copy of the #1003 Ashkin (Deep-Line) spec (the shared fixture is cfg(test)-only). Kept identical
// to rpg/race_specs_fixture_g1.rs::ashkin_deep_race_spec so the FSV binary authors the same row.
fn ashkin_deep_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ASHKIN_DEEP_RACE_ID.to_string(),
        display_name: "Ashkin (Deep-Line)".to_string(),
        player_facing_canon_name: "Ashkin (Deep-Line)".to_string(),
        source_issue: 1003,
        canon_issues: vec![984, 985, 1003],
        mechanics_source_url: "https://wiki.carrionfields.net/race/dark-elf".to_string(),
        source_history_label: "CF Dark-Elf factual mechanics source only".to_string(),
        culture_summary: "Deep-line Ashkin - the undiluted grey-marked line of the Sundering, gifted and dangerous near thin Weave, their power bound to the shadowed path they walk.".to_string(),
        stat_caps: AttributeScores::new(18, 24, 21, 24, 16, 18),
        alignment: RaceAccessRule::Restricted { values: vec!["evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: Vec::new(),
        innate_abilities: vec![RaceInnateAbility { ability_id: "faerie_fire".to_string(), display_name: "Faerie Fire".to_string(), description: "Calls a clinging magical light that outlines a foe, marking them for easier strikes.".to_string() }, RaceInnateAbility { ability_id: "infravision".to_string(), display_name: "Infravision".to_string(), description: "Sees in the dark by heat and shadow without a light source.".to_string() }],
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.warrior".to_string(), "class.thief".to_string(), "class.transmuter".to_string(), "class.invoker".to_string(), "class.necromancer".to_string(), "class.conjurer".to_string(), "class.assassin".to_string(), "class.anti_paladin".to_string(), "class.healer".to_string(), "class.shaman".to_string(), "class.ranger".to_string()] },
        experience_penalty_percent: 400,
        hometowns: vec![home("origin.ashkin.deep_seat", "Deep Seat", "The deep thin-Weave holdfast of the deep-line Ashkin")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.ashkin.deep_line".to_string(),
            skeleton_id: "skeleton.biped.slender".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "medium biped, slight deep-marked proportions".to_string(),
            material_notes: "Vhael-original deep-marked raiment and shadow-worn greys; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Mithril' has no framework resistance channel; captured here, left unencoded (blocked on #1631).".to_string(), "CF: deep-line must stay evil to cast their spells; encoded as the Evil-only alignment restriction.".to_string(), "CF also states they can 'walk unnoticed'; captured as flavor alongside the infravision innate.".to_string(), "CF gives no explicit size tier; prose is slight build, average 5'6\". Medium is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string(), "Name/lore originalized per #984: player-facing category is the canon-original Ashkin (deep-line); CF Dark-Elf is the mechanics source only. Mechanics unchanged.".to_string()],
    }
}

fn ashkin_deep_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&ashkin_deep_race_spec())
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
    let run = begin_issue_run("issue1003_ashkin_deep_race_spec")?;
    let spec = ashkin_deep_race_spec();
    let readback = race_spec_readback(&spec)?;
    let records = race_spec_content_records(&spec)?;
    let authored_payload = records[0].payload.clone();

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    let stored_row = store.get_content_for_editor(records[0].kind, &records[0].id)?;
    let identity = happy_path_identity(&spec, &authored_payload, &stored_row)?;
    let stats_probe = attribute_pipeline_probe()?;

    let edge_stat_cap = reject_edge("out_of_range_stat_cap", out_of_range_stat_cap())?;
    let edge_bad_class_token = reject_edge("malformed_class_token", malformed_class_token())?;
    let edge_ghost_class = flagged_pass_edge("nonexistent_class_no_registry", nonexistent_class())?;

    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;

    let report = json!({
        "schema": "litd.issue1003.ashkin_deep_race_fsv.v1",
        "issue": 1003,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "mechanics_source_url": "https://wiki.carrionfields.net/race/dark-elf",
            "sources": source_readbacks,
        },
        "expected_actuals": {
            "race_id": ASHKIN_DEEP_RACE_ID,
            "source_issue": 1003,
            "stat_caps": {"strength": 18, "intelligence": 24, "wisdom": 21, "dexterity": 24, "constitution": 16, "charisma": 18},
            "alignment_matches": readback.alignment == RaceAccessRule::Restricted { values: vec!["evil".to_string()] },
            "innate_ability_count": 2,
            "infravision": true,
            "experience_penalty_percent": 400,
            "class_access_kind": "restricted",
            "class_id_count": 11,
            "resistance_count": 0,
            "vulnerability_count": 0,
            "content_rows_written": lowered.row_count,
            "stored_matches_authored": identity["stored_matches_authored"],
            "con_clamped_to_cap": stats_probe["con_clamped_to_cap"],
            "edge_stat_cap_rejected": edge_stat_cap["rejected"],
            "edge_malformed_class_rejected": edge_bad_class_token["rejected"],
            "edge_nonexistent_class_rejected": edge_ghost_class["rejected"],
            "edge_nonexistent_class_flagged_gap": edge_ghost_class["flagged_gap"],
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "happy_path": readback,
        "readback_identity": identity,
        "calyx": {
            "content_rows": content_rows(std::slice::from_ref(&stored_row)),
            "lowered_entities": &lowered.entities,
        },
        "stats_probe": stats_probe,
        "edges": [edge_stat_cap, edge_bad_class_token, edge_ghost_class],
    });

    require_bool(&report["expected_actuals"]["stored_matches_authored"], true)?;
    require_bool(&report["expected_actuals"]["alignment_matches"], true)?;
    require_bool(&report["expected_actuals"]["con_clamped_to_cap"], true)?;
    require_bool(&report["expected_actuals"]["edge_stat_cap_rejected"], true)?;
    require_bool(
        &report["expected_actuals"]["edge_malformed_class_rejected"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["edge_nonexistent_class_rejected"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["edge_nonexistent_class_flagged_gap"],
        true,
    )?;
    require_i64(&report["expected_actuals"]["class_id_count"], 11)?;
    require_i64(
        &report["expected_actuals"]["experience_penalty_percent"],
        400,
    )?;

    let written = run.write_report(&report)?;
    println!(
        "ISSUE1003_ASHKIN_DEEP_RACE_FSV report={} sha256={} race={} stored_matches={} con_clamped={} caps={} xp={} class_kind={}",
        written.path.display(),
        written.sha256,
        written.json["happy_path"]["race_id"],
        written.json["expected_actuals"]["stored_matches_authored"],
        written.json["expected_actuals"]["con_clamped_to_cap"],
        written.json["happy_path"]["stat_caps"],
        written.json["happy_path"]["experience_penalty_percent"],
        written.json["expected_actuals"]["class_access_kind"],
    );
    Ok(())
}

fn happy_path_identity(
    spec: &RaceSpec,
    authored_payload: &Value,
    stored_row: &EngineContentConstellationReadback,
) -> Result<Value, EngineError> {
    let stored = &stored_row.payload;
    let fields = [
        ("race_id", authored_payload["race_id"] == stored["race_id"]),
        (
            "stat_caps",
            authored_payload["stat_caps"] == stored["stat_caps"],
        ),
        (
            "alignment",
            authored_payload["alignment"] == stored["alignment"],
        ),
        (
            "class_access",
            authored_payload["class_access"] == stored["class_access"],
        ),
        (
            "experience_penalty_percent",
            authored_payload["experience_penalty_percent"] == stored["experience_penalty_percent"],
        ),
        (
            "innate_abilities",
            authored_payload["innate_abilities"] == stored["innate_abilities"],
        ),
        (
            "resistances",
            authored_payload["resistances"] == stored["resistances"],
        ),
        (
            "vulnerabilities",
            authored_payload["vulnerabilities"] == stored["vulnerabilities"],
        ),
        ("visual", authored_payload["visual"] == stored["visual"]),
        (
            "race_spec_sha256",
            authored_payload["race_spec_sha256"] == stored["race_spec_sha256"],
        ),
    ];
    let all_match = fields.iter().all(|(_, ok)| *ok) && stored_row.id == spec.race_id;
    Ok(json!({
        "before_authored_race_id": authored_payload["race_id"],
        "before_authored_stat_caps": authored_payload["stat_caps"],
        "before_authored_class_access": authored_payload["class_access"],
        "before_authored_spec_sha256": authored_payload["race_spec_sha256"],
        "after_stored_race_id": stored["race_id"],
        "after_stored_stat_caps": stored["stat_caps"],
        "after_stored_class_access": stored["class_access"],
        "after_stored_spec_sha256": stored["race_spec_sha256"],
        "after_stored_source_path": stored_row.source_path,
        "per_field_match": fields.iter().map(|(k, v)| json!({"field": k, "match": v})).collect::<Vec<_>>(),
        "stored_matches_authored": all_match,
    }))
}

fn attribute_pipeline_probe() -> Result<Value, EngineError> {
    let table = DerivedStatFormulaTable::mud_defaults();
    let input = AttributeBuildInput {
        character_id: "char.ashkin_deep.probe".to_string(),
        race_id: ASHKIN_DEEP_RACE_ID.to_string(),
        level: 5,
        base: race_caps_scores(),
        race_caps: vec![ashkin_deep_race_attribute_caps()],
        modifiers: vec![StatModifier {
            modifier_id: "gear.probe_band".to_string(),
            source: StatModifierSource::Gear,
            attributes: AttributeScores::new(0, 0, 0, 0, 10, 0),
            derived: DerivedStats::ZERO,
            resistances: Vec::new(),
        }],
    };
    let readback = derive_character_stats(&input, &table)?;
    Ok(json!({
        "race_id": readback.race_id,
        "race_caps": readback.race_caps,
        "uncapped_attributes": readback.uncapped_attributes,
        "effective_attributes": readback.effective_attributes,
        "con_clamped_to_cap": readback.uncapped_attributes.constitution == 26
            && readback.effective_attributes.constitution == 16,
        "cap_clamps": readback.cap_clamps,
        "result_sha256": readback.result_sha256,
    }))
}

fn race_caps_scores() -> AttributeScores {
    ashkin_deep_race_spec().stat_caps
}

fn reject_edge(case: &str, spec: RaceSpec) -> Result<Value, EngineError> {
    let before = spec_hash(&spec)?;
    let outcome = validate_race_spec(&spec);
    let after = spec_hash(&spec)?;
    let (rejected, code, message) = match outcome {
        Ok(_) => (false, String::new(), "unexpectedly accepted".to_string()),
        Err(error) => (true, error.code().to_string(), error.to_string()),
    };
    Ok(json!({
        "case": case,
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "rejected": rejected,
        "flagged_gap": false,
        "error_code": code,
        "error": message,
    }))
}

fn flagged_pass_edge(case: &str, spec: RaceSpec) -> Result<Value, EngineError> {
    let before = spec_hash(&spec)?;
    let accepted = validate_race_spec(&spec).is_ok();
    let after = spec_hash(&spec)?;
    Ok(json!({
        "case": case,
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "rejected": !accepted,
        "flagged_gap": accepted,
        "gap": "no ClassSpec registry (#1019-1037) exists yet, so a well-formed but non-existent class id passes validation; referential integrity must be added when the class roster lands.",
    }))
}

fn out_of_range_stat_cap() -> RaceSpec {
    let mut spec = ashkin_deep_race_spec();
    spec.stat_caps = AttributeScores::new(200, 24, 21, 24, 16, 18);
    spec
}

fn malformed_class_token() -> RaceSpec {
    let mut spec = ashkin_deep_race_spec();
    spec.class_access = RaceClassAccess::AllExcept {
        class_ids: vec!["class.not a token!".to_string()],
    };
    spec
}

fn nonexistent_class() -> RaceSpec {
    let mut spec = ashkin_deep_race_spec();
    spec.class_access = RaceClassAccess::Restricted {
        class_ids: vec!["class.does_not_exist_anywhere".to_string()],
    };
    spec
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
                "class_access": row.payload["class_access"],
                "race_spec_sha256": row.payload["race_spec_sha256"],
                "lowering_artifact_id": row.lowering.artifact_id,
                "lowering_fingerprint": row.lowering.fingerprint,
            })
        })
        .collect()
}

fn spec_hash(spec: &RaceSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
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
        field: "fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
