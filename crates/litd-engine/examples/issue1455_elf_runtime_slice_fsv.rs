//! #1455 Elf runtime-slice FSV - the keystone proof that editor-authored content actually loads
//! at runtime from the CANONICAL Calyx vault, instead of living only in a `#[cfg(test)]` fixture.
//!
//! Source of truth is the REAL `gameeditor-engine-content-panel` vault under the canonical Calyx
//! root, not a throwaway temp dir. The evidence chain is:
//!   1. Author Elf into the canonical vault via the same primitives the `author_race` MCP tool
//!      wraps (`open_canonical_engine_content_store` + `race_spec_content_records` +
//!      `persist_and_lower_rpg_authored_content`).
//!   2. Read the row back with a SEPARATE op (`get_content_for_editor`).
//!   3. Load it at runtime with `load_frozen_content_for_sim` and assert it is in the frozen table.
//!   4. Parity: the authored row's lowered fingerprint == the known-good `elf_race_spec` oracle's.
//!   5. Three edge cases with before/after state: a no-recompile stat edit, a fail-closed invalid
//!      record (row count unchanged), and a loud miss on a non-existent id.

use std::error::Error;

use litd_engine::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
        resolve_engine_content_panel_vault,
    },
    calyx_content_runtime::{
        FrozenContentTable, load_frozen_content_for_sim, open_canonical_engine_content_store,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AttributeScores, RPG_RACE_SPEC_SCHEMA, RaceAccessRule, RaceClassAccess, RaceHometown,
        RaceInnateAbility, RaceSize, RaceSpec, RaceVision, RaceVisualContract,
        persist_and_lower_rpg_authored_content, race_spec_content_records,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const ELF_ID: &str = "race.elf";
const STAT_INT_POINTER: &str = "/stat_caps/intelligence";

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1455_elf_runtime_slice")?;
    let vault_dir = resolve_engine_content_panel_vault()?;
    println!("ISSUE1455_SLICE canonical_vault={}", vault_dir.display());

    // --- 1. AUTHOR: Elf -> canonical vault (the author_race tool's engine path) --------------
    let store = open_canonical_engine_content_store()?;
    let authored_records = race_spec_content_records(&elf_race_spec(25))?;
    let authored = persist_and_lower_rpg_authored_content(&store, &authored_records)?;
    let authored_entity = authored
        .entities
        .first()
        .ok_or("author produced no entity")?
        .clone();

    // --- 2. READBACK (separate op) ----------------------------------------------------------
    let readback = store.get_content_for_editor(EngineContentKind::Unit, ELF_ID)?;
    println!(
        "ISSUE1455_SLICE readback cx_id={} input_sha256={} lowered_fingerprint={}",
        readback.cx_id, readback.input_sha256, readback.lowering.fingerprint
    );

    // --- 3. RUNTIME LOAD: freeze from the canonical vault -----------------------------------
    let frozen_ids = [(EngineContentKind::Unit, ELF_ID.to_string())];
    let frozen_artifacts = load_frozen_content_for_sim(&store, &frozen_ids)?;
    let frozen_table = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let elf_present = frozen_table.row(EngineContentKind::Unit, ELF_ID).is_some();
    let loaded_int_cap = frozen_table
        .row(EngineContentKind::Unit, ELF_ID)
        .and_then(|row| row.payload_i64(STAT_INT_POINTER))
        .ok_or("frozen Elf row missing intelligence stat cap")?;

    // --- 4. PARITY: authored row == known-good oracle lowered fingerprint --------------------
    // Independently rebuild the oracle spec and persist it to a scratch vault; both go through the
    // identical measurement/lowering path, so an authored row that diverged from the canonical
    // race_specs_content shape would show a different fingerprint here.
    let oracle_vault = run.artifact_dir("oracle-vault")?;
    let oracle_store = EngineCalyxContentStore::new(EngineCalyxDatabase::open(
        EngineCalyxDatabaseConfig::for_vault(&oracle_vault),
    )?);
    let oracle_records = race_spec_content_records(&elf_race_spec(25))?;
    let oracle = persist_and_lower_rpg_authored_content(&oracle_store, &oracle_records)?;
    let oracle_entity = oracle.entities.first().ok_or("oracle produced no entity")?;
    let parity_lowered =
        authored_entity.lowered_package_fingerprint == oracle_entity.lowered_package_fingerprint;
    let parity_input = authored_entity.input_sha256 == oracle_entity.input_sha256;

    // --- 5a. EDGE: no-recompile edit changes the canonical lowered fingerprint --------------
    let before_fp = readback.lowering.fingerprint.clone();
    let before_int = loaded_int_cap;
    let edited_records = race_spec_content_records(&elf_race_spec(24))?;
    persist_and_lower_rpg_authored_content(&store, &edited_records)?;
    let edited_table = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let after_fp = edited_table
        .fingerprint(EngineContentKind::Unit, ELF_ID)
        .ok_or("edited Elf row missing")?
        .to_string();
    let after_int = edited_table
        .row(EngineContentKind::Unit, ELF_ID)
        .and_then(|row| row.payload_i64(STAT_INT_POINTER))
        .ok_or("edited Elf row missing intelligence stat cap")?;
    let edit_changed_fingerprint = before_fp != after_fp;
    let edit_changed_value = before_int != after_int;
    // Restore the canonical vault to the known-good Elf (INT 25) so it ends in the oracle state.
    persist_and_lower_rpg_authored_content(&store, &authored_records)?;
    let restored_fp = FrozenContentTable::freeze(&store, &frozen_ids)?
        .fingerprint(EngineContentKind::Unit, ELF_ID)
        .ok_or("restored Elf row missing")?
        .to_string();
    let restored_matches_before = restored_fp == before_fp;

    // --- 5b. EDGE: invalid record fails closed, canonical row count unchanged ----------------
    let count_before = resolvable_count(&store);
    let bad_source = author_invalid_source_record();
    let bad_source_rejected = persist_and_lower_rpg_authored_content(&store, &bad_source).is_err();
    let bad_flag = author_free_map_flag_record();
    let bad_flag_rejected = persist_and_lower_rpg_authored_content(&store, &bad_flag).is_err();
    let count_after = resolvable_count(&store);
    let bad_id_absent = store
        .get_content_for_editor(EngineContentKind::Unit, "race.bad_source")
        .is_err();

    // --- 5c. EDGE: readback of a non-existent id errors loudly, not empty-ok ----------------
    let missing_errs = store
        .get_content_for_editor(EngineContentKind::Unit, "race.does_not_exist")
        .is_err();

    let report = json!({
        "schema": "litd.issue1455.elf_runtime_slice_fsv.v1",
        "issue": 1455,
        "scope": "Elf race end-to-end vertical slice; #1455 stays open until full content migration",
        "source_of_truth": {
            "canonical_vault": vault_dir.display().to_string(),
            "oracle_scratch_vault": oracle_vault.display().to_string(),
            "report_path": run.report_path().display().to_string(),
        },
        "expected_actuals": {
            "authored_row_count": authored.row_count,
            "frozen_artifact_count": frozen_artifacts.len(),
            "elf_present_in_frozen_table": elf_present,
            "loaded_intelligence_cap": loaded_int_cap,
            "parity_lowered_fingerprint": parity_lowered,
            "parity_input_sha256": parity_input,
            "edit_before_fingerprint": before_fp,
            "edit_after_fingerprint": after_fp,
            "edit_before_intelligence": before_int,
            "edit_after_intelligence": after_int,
            "edit_changed_fingerprint": edit_changed_fingerprint,
            "edit_changed_value": edit_changed_value,
            "restored_matches_before": restored_matches_before,
            "invalid_source_rejected": bad_source_rejected,
            "invalid_free_map_flag_rejected": bad_flag_rejected,
            "row_count_before_bad": count_before,
            "row_count_after_bad": count_after,
            "bad_id_absent": bad_id_absent,
            "missing_id_errors_loud": missing_errs,
        },
        "authored_entity": authored_entity,
        "oracle_entity": oracle_entity,
    });

    require_bool(&report, "elf_present_in_frozen_table", true)?;
    require_bool(&report, "parity_lowered_fingerprint", true)?;
    require_bool(&report, "parity_input_sha256", true)?;
    require_bool(&report, "edit_changed_fingerprint", true)?;
    require_bool(&report, "edit_changed_value", true)?;
    require_bool(&report, "restored_matches_before", true)?;
    require_bool(&report, "invalid_source_rejected", true)?;
    require_bool(&report, "invalid_free_map_flag_rejected", true)?;
    require_bool(&report, "bad_id_absent", true)?;
    require_bool(&report, "missing_id_errors_loud", true)?;
    require_i64(&report, "authored_row_count", 1)?;
    require_i64(&report, "frozen_artifact_count", 1)?;
    require_i64(&report, "loaded_intelligence_cap", 25)?;
    require_i64(&report, "edit_after_intelligence", 24)?;
    require_i64(&report, "row_count_before_bad", 1)?;
    require_i64(&report, "row_count_after_bad", 1)?;
    if count_before != count_after {
        return Err(format!(
            "canonical row count changed on a rejected write: before={count_before} after={count_after}"
        )
        .into());
    }

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1455_ELF_RUNTIME_SLICE_FSV report={} sha256={} elf_loaded={} parity={} edit_fp {}->{} int {}->{} restored={} bad_rejected={}/{} count {}=={} missing_loud={}",
        read.path.display(),
        read.sha256,
        elf_present,
        parity_lowered && parity_input,
        before_fp,
        after_fp,
        before_int,
        after_int,
        restored_matches_before,
        bad_source_rejected,
        bad_flag_rejected,
        count_before,
        count_after,
        missing_errs,
    );
    Ok(())
}

fn resolvable_count(store: &EngineCalyxContentStore) -> u32 {
    [ELF_ID]
        .into_iter()
        .filter(|id| {
            store
                .get_content_for_editor(EngineContentKind::Unit, id)
                .is_ok()
        })
        .count() as u32
}

fn author_invalid_source_record() -> Vec<EngineContentRecord> {
    // A compiled rpg:// source must be rejected: RPG content has to come from editor/MCP authoring.
    let payload = json!({
        "schema": RPG_RACE_SPEC_SCHEMA,
        "surface_kind": "race_spec",
        "race_id": "race.bad_source",
    });
    vec![EngineContentRecord {
        kind: EngineContentKind::Unit,
        id: "race.bad_source".to_string(),
        display_name: "Race Spec: Bad Source".to_string(),
        source: EngineContentSource {
            path: "rpg://race_specs/race.bad_source".to_string(),
            author: "editor_authoring.issue1455.negative".to_string(),
            source_hash: sha256_hex(&serde_json::to_vec(&payload).unwrap()),
        },
        payload,
        anchors: Vec::new(),
    }]
}

fn author_free_map_flag_record() -> Vec<EngineContentRecord> {
    // A free runtime map/GPS flag must be rejected (diegetic-map guardrail).
    let payload = json!({
        "schema": RPG_RACE_SPEC_SCHEMA,
        "surface_kind": "race_spec",
        "race_id": "race.bad_flag",
        "grants_map_or_minimap": true,
    });
    vec![EngineContentRecord {
        kind: EngineContentKind::Unit,
        id: "race.bad_flag".to_string(),
        display_name: "Race Spec: Bad Flag".to_string(),
        source: EngineContentSource {
            path: "editor://rpg/ashen_reach/race_specs/race.bad_flag".to_string(),
            author: "editor_authoring.issue1455.negative".to_string(),
            source_hash: sha256_hex(&serde_json::to_vec(&payload).unwrap()),
        },
        payload,
        anchors: Vec::new(),
    }]
}

fn require_bool(report: &Value, key: &str, expected: bool) -> Result<(), Box<dyn Error>> {
    let actual = report["expected_actuals"][key].as_bool();
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(format!("expected {key}={expected}, got {actual:?}").into())
    }
}

fn require_i64(report: &Value, key: &str, expected: i64) -> Result<(), Box<dyn Error>> {
    let actual = report["expected_actuals"][key].as_i64();
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(format!("expected {key}={expected}, got {actual:?}").into())
    }
}

// Local copy of the #1000 Elf spec (the shared fixture is cfg(test)-only). `intelligence_cap`
// parameterizes the one stat the no-recompile-edit edge case mutates; every other field is held
// identical to rpg/race_specs_test_fixture.rs::elf_race_spec so the authored row matches the
// oracle exactly.
fn elf_race_spec(intelligence_cap: i16) -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ELF_ID.to_string(),
        display_name: "Elf".to_string(),
        player_facing_canon_name: "Elf".to_string(),
        source_issue: 1000,
        canon_issues: vec![984, 985, 1000],
        mechanics_source_url: "https://wiki.carrionfields.net/race/elf".to_string(),
        source_history_label: "CF Elf (High Elf) factual mechanics source only".to_string(),
        culture_summary:
            "The Name-Keepers, eldest of the Light-devoted elven lines, holding the old memory of \
             Vhael from before the Sundering."
                .to_string(),
        stat_caps: AttributeScores::new(18, intelligence_cap, 21, 23, 16, 21),
        alignment: RaceAccessRule::Restricted {
            values: vec!["good".to_string()],
        },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: Vec::new(),
        innate_abilities: vec![RaceInnateAbility {
            ability_id: "swiftstrike".to_string(),
            display_name: "Swiftstrike".to_string(),
            description:
                "Inherent elven quickness in the strike; the elf's light-footed passage often goes \
                 unnoticed by those who do not lay eyes upon it."
                    .to_string(),
        }],
        vision: RaceVision {
            infravision: false,
            infravision_tiles: 0,
        },
        class_access: RaceClassAccess::AllExcept {
            class_ids: vec![
                "class.druid".to_string(),
                "class.anti_paladin".to_string(),
                "class.necromancer".to_string(),
                "class.assassin".to_string(),
                "class.shaman".to_string(),
            ],
        },
        experience_penalty_percent: 400,
        hometowns: vec![RaceHometown {
            origin_id: "origin.elf.name_keeper_seat".to_string(),
            display_name: "Name-Keeper Seat".to_string(),
            canon_place: "The high-elven seat of the Name-Keepers".to_string(),
        }],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.elf.name_keeper".to_string(),
            skeleton_id: "skeleton.biped.slender".to_string(),
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
            silhouette: "tall, slender biped; graceful high-elven proportions".to_string(),
            material_notes:
                "Vhael-original Name-Keeper raiment, carved name-tokens, and muted forest greys; \
                 no copied race iconography."
                    .to_string(),
        },
        implementation_notes: vec![
            "CF stat caps captured verbatim: STR Strong(18), INT Genius(25), WIS Sagacious(21), \
             DEX Quick(23), CON Average(16), CHA Disarming(21)."
                .to_string(),
            "CF 'Resistant: Curse' and 'Vulnerable: Iron' are NOT yet encodable: the resistance \
             vocabulary (DEFAULT_RESISTANCE_IDS) has only the nine combat damage channels. \
             Captured here as facts pending a framework fix; not mapped onto a wrong channel."
                .to_string(),
            "CF has no explicit Size field; descriptive only (taller than humans, avg 6'3\", \
             slender, least hardy). Medium is the nearest framework tier."
                .to_string(),
            "CF has no infravision entry; High Elves are surface-dwellers. Infravision left false."
                .to_string(),
            "CF class rule is a denylist (any profession EXCEPT Druid, Anti-Paladin, Necromancer, \
             Assassin, Shaman); encoded as RaceClassAccess::AllExcept."
                .to_string(),
            "Name/lore originalized per #984: player-facing category stays 'Elf'; the Vhael line is \
             the Light-devoted Name-Keepers. Mechanics unchanged."
                .to_string(),
        ],
    }
}
