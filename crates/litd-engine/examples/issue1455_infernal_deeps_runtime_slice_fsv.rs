//! #1455 Infernal Deeps runtime-slice FSV - the flagship-corpus proof that the SIM consumes
//! editor-authored, Calyx-lowered data at runtime instead of Rust literals.
//!
//! The Elf slice (`issue1455_elf_runtime_slice_fsv`) proved the runtime-load pattern for one race.
//! This slice extends it to the largest hardcoded Ashen Reach corpus - the Infernal Deeps - and
//! goes one step deeper: a Rust sim mechanic (`admit_withering_raid`) reads its tuning thresholds
//! out of the FROZEN lowered rows, so editing an authored number in the editor moves an in-game
//! gate with no recompile. That is the exact #1348/#1455 requirement for "mechanics that stay in
//! Rust must read their tuning from lowered data, not literals".
//!
//! Source of truth is the REAL `gameeditor-engine-content-panel` vault under the canonical Calyx
//! root, not a throwaway temp dir. Evidence chain:
//!   1. Author the 6 Infernal Deeps entities into the canonical vault (same builder the migration
//!      guardrail slice uses: `infernal_deeps_content_records`).
//!   2. Read one row back with a SEPARATE op (`get_content_for_editor`).
//!   3. Freeze all 6 rows with `FrozenContentTable::freeze` and build `InfernalDeepsRuntimeTuning`
//!      entirely from the frozen payloads.
//!   4. Parity: every frozen tuning value == the Rust oracle (`derive_infernal_deeps_package`).
//!   5. Sim behavior: `admit_withering_raid` denies a 7-person party and admits an 8-person party
//!      using the FROZEN minimum party size.
//!   6. Edges with before/after state: a no-recompile edit (min party 8 -> 10) flips the admission
//!      of the same 8-person party and changes the lowered fingerprint; a fail-closed invalid
//!      record leaves the canonical row count unchanged; a missing id errors loud.

use std::error::Error;

#[path = "fixtures/issue1061_undeath_fixture.rs"]
mod issue1061_undeath_fixture;
#[path = "fixtures/issue1066_infernal_access_fixture.rs"]
mod issue1066_infernal_access_fixture;
#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

use issue1061_undeath_fixture::ashen_reach_undeath_policy;
use issue1066_infernal_access_fixture::ashen_reach_infernal_access_policy;
use issue1067_infernal_zones_fixture::ashen_reach_infernal_zone_policy;
use issue1070_infernal_attrition_fixture::ashen_reach_infernal_attrition_policy;

use litd_engine::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
        resolve_engine_content_panel_vault,
    },
    calyx_content_runtime::{FrozenContentTable, open_canonical_engine_content_store},
    fsv_artifacts::begin_issue_run,
    rpg::{
        DeriveInfernalDeepsPackageArgs, INFERNAL_DEEPS_RELIC_VAULTS_ID,
        INFERNAL_DEEPS_WITHERING_ID, InfernalBossAgentPolicy, InfernalDeepsPackagePolicy,
        InfernalDeepsPackageReadback, InfernalDeepsRuntimeTuning, InfernalRelicVaultPolicy,
        WitheringRaidAdmission, WitheringRaidPathPolicy, admit_withering_raid,
        derive_infernal_deeps_package, infernal_deeps_content_records,
        persist_and_lower_rpg_authored_content,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const WITHERING_ID: &str = INFERNAL_DEEPS_WITHERING_ID;

// The 6 authored Infernal Deeps entities, with the content kind each was authored under.
const DEEPS_ROWS: [(EngineContentKind, &str); 6] = [
    (
        EngineContentKind::Lore,
        "server.ashen_reach.infernal_deeps.package",
    ),
    (
        EngineContentKind::Trigger,
        "server.ashen_reach.infernal_deeps.access_contract",
    ),
    (
        EngineContentKind::Terrain,
        "server.ashen_reach.infernal_deeps.zone_topology",
    ),
    (
        EngineContentKind::Agent,
        "server.ashen_reach.infernal_deeps.boss_hierarchy",
    ),
    (EngineContentKind::Item, INFERNAL_DEEPS_RELIC_VAULTS_ID),
    (EngineContentKind::Story, INFERNAL_DEEPS_WITHERING_ID),
];

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1455_infernal_deeps_runtime_slice")?;
    let vault_dir = resolve_engine_content_panel_vault()?;
    println!("ISSUE1455_DEEPS canonical_vault={}", vault_dir.display());

    // --- 0. ORACLE: the pre-migration Rust derivation (the parity target) --------------------
    let oracle = infernal_deeps_oracle()?;

    // --- 1. AUTHOR: 6 Infernal Deeps rows -> canonical vault ---------------------------------
    let store = open_canonical_engine_content_store()?;
    let authored_records = infernal_deeps_records(8)?;
    let authored = persist_and_lower_rpg_authored_content(&store, &authored_records)?;

    // --- 2. READBACK (separate op) on the withering row --------------------------------------
    let withering_readback =
        store.get_content_for_editor(EngineContentKind::Story, WITHERING_ID)?;
    println!(
        "ISSUE1455_DEEPS readback cx_id={} input_sha256={} lowered_fingerprint={}",
        withering_readback.cx_id,
        withering_readback.input_sha256,
        withering_readback.lowering.fingerprint
    );

    // --- 3. RUNTIME LOAD: freeze all 6 rows, build tuning from frozen payloads ----------------
    let frozen_ids = frozen_ids();
    let frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let tuning = InfernalDeepsRuntimeTuning::from_frozen(&frozen)?;

    // --- 4. PARITY: frozen tuning == Rust oracle ----------------------------------------------
    let parity_party =
        tuning.withering_minimum_party_size == oracle.withering_minimum_party_size as i64;
    let parity_level =
        tuning.withering_minimum_character_level == oracle.withering_minimum_character_level as i64;
    let parity_phases =
        tuning.withering_raid_phase_count == oracle.withering_raid_phase_count as i64;
    let parity_zones = tuning.zone_count == oracle.zone_count as i64;
    let parity_bosses = tuning.boss_count == oracle.boss_count as i64;
    let parity_relics = tuning.relic_vault_count == oracle.relic_vault_count as i64;
    let parity_all = parity_party
        && parity_level
        && parity_phases
        && parity_zones
        && parity_bosses
        && parity_relics;

    // --- 5. SIM BEHAVIOR: the gate uses the FROZEN minimum party size -------------------------
    let deny_seven = admit_withering_raid(&tuning, 7, 60);
    let admit_eight = admit_withering_raid(&tuning, 8, 60);
    let seven_denied = matches!(
        deny_seven,
        WitheringRaidAdmission::DeniedPartyTooSmall { .. }
    );
    let eight_admitted = admit_eight == WitheringRaidAdmission::Admitted;

    // --- 6a. EDGE: no-recompile edit (min party 8 -> 10) flips the same 8-party ---------------
    let before_fp = frozen
        .fingerprint(EngineContentKind::Story, WITHERING_ID)
        .ok_or("frozen withering row missing")?
        .to_string();
    let before_min = tuning.withering_minimum_party_size;
    let edited_records = infernal_deeps_records(10)?;
    persist_and_lower_rpg_authored_content(&store, &edited_records)?;
    let edited_frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let edited_tuning = InfernalDeepsRuntimeTuning::from_frozen(&edited_frozen)?;
    let after_fp = edited_frozen
        .fingerprint(EngineContentKind::Story, WITHERING_ID)
        .ok_or("edited withering row missing")?
        .to_string();
    let after_min = edited_tuning.withering_minimum_party_size;
    let admit_eight_after = admit_withering_raid(&edited_tuning, 8, 60);
    let eight_denied_after = matches!(
        admit_eight_after,
        WitheringRaidAdmission::DeniedPartyTooSmall { .. }
    );
    let edit_changed_fingerprint = before_fp != after_fp;
    let edit_changed_min = before_min != after_min;

    // Restore the canonical vault to the known-good min-8 state so it ends in the oracle shape.
    persist_and_lower_rpg_authored_content(&store, &authored_records)?;
    let restored_min =
        InfernalDeepsRuntimeTuning::from_frozen(&FrozenContentTable::freeze(&store, &frozen_ids)?)?
            .withering_minimum_party_size;
    let restored_matches = restored_min == before_min;

    // --- 6b. EDGE: invalid record fails closed, canonical row count unchanged ------------------
    let count_before = resolvable_count(&store);
    let bad_source_rejected =
        persist_and_lower_rpg_authored_content(&store, &author_invalid_source_record()).is_err();
    let bad_flag_rejected =
        persist_and_lower_rpg_authored_content(&store, &author_free_map_flag_record()).is_err();
    let count_after = resolvable_count(&store);
    let bad_id_absent = store
        .get_content_for_editor(
            EngineContentKind::Story,
            "server.ashen_reach.infernal_deeps.bad",
        )
        .is_err();

    // --- 6c. EDGE: readback / freeze of a non-existent id errors loud, not empty-ok -----------
    let missing_errs = store
        .get_content_for_editor(
            EngineContentKind::Story,
            "server.ashen_reach.infernal_deeps.nope",
        )
        .is_err();

    let report = json!({
        "schema": "litd.issue1455.infernal_deeps_runtime_slice_fsv.v1",
        "issue": 1455,
        "scope": "Infernal Deeps corpus end-to-end runtime slice; the sim reads tuning from lowered data, not Rust literals",
        "source_of_truth": {
            "canonical_vault": vault_dir.display().to_string(),
            "report_path": run.report_path().display().to_string(),
        },
        "expected_actuals": {
            "authored_row_count": authored.row_count,
            "frozen_row_count": frozen.len(),
            "tuning_min_party_size": tuning.withering_minimum_party_size,
            "tuning_min_character_level": tuning.withering_minimum_character_level,
            "tuning_raid_phase_count": tuning.withering_raid_phase_count,
            "tuning_zone_count": tuning.zone_count,
            "tuning_boss_count": tuning.boss_count,
            "tuning_relic_vault_count": tuning.relic_vault_count,
            "parity_all": parity_all,
            "seven_party_denied": seven_denied,
            "eight_party_admitted": eight_admitted,
            "edit_before_min_party": before_min,
            "edit_after_min_party": after_min,
            "edit_changed_min": edit_changed_min,
            "edit_changed_fingerprint": edit_changed_fingerprint,
            "eight_party_denied_after_edit": eight_denied_after,
            "restored_matches_before": restored_matches,
            "invalid_source_rejected": bad_source_rejected,
            "invalid_free_map_flag_rejected": bad_flag_rejected,
            "row_count_before_bad": count_before,
            "row_count_after_bad": count_after,
            "bad_id_absent": bad_id_absent,
            "missing_id_errors_loud": missing_errs,
        },
        "oracle_readback": oracle,
    });

    require_bool(&report, "parity_all", true)?;
    require_bool(&report, "seven_party_denied", true)?;
    require_bool(&report, "eight_party_admitted", true)?;
    require_bool(&report, "edit_changed_min", true)?;
    require_bool(&report, "edit_changed_fingerprint", true)?;
    require_bool(&report, "eight_party_denied_after_edit", true)?;
    require_bool(&report, "restored_matches_before", true)?;
    require_bool(&report, "invalid_source_rejected", true)?;
    require_bool(&report, "invalid_free_map_flag_rejected", true)?;
    require_bool(&report, "bad_id_absent", true)?;
    require_bool(&report, "missing_id_errors_loud", true)?;
    require_i64(&report, "authored_row_count", 6)?;
    require_i64(&report, "frozen_row_count", 6)?;
    require_i64(&report, "tuning_min_party_size", 8)?;
    require_i64(&report, "tuning_zone_count", 7)?;
    require_i64(&report, "tuning_boss_count", 7)?;
    require_i64(&report, "edit_after_min_party", 10)?;
    require_i64(&report, "row_count_before_bad", 6)?;
    require_i64(&report, "row_count_after_bad", 6)?;
    if count_before != count_after {
        return Err(format!(
            "canonical row count changed on a rejected write: before={count_before} after={count_after}"
        )
        .into());
    }

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1455_INFERNAL_DEEPS_RUNTIME_SLICE_FSV report={} sha256={} frozen_rows={} parity={} gate 7=deny/8=admit {}/{} edit min {}->{} fp {}->{} 8_denied_after={} restored={} bad_rejected={}/{} count {}=={} missing_loud={}",
        read.path.display(),
        read.sha256,
        frozen.len(),
        parity_all,
        seven_denied,
        eight_admitted,
        before_min,
        after_min,
        before_fp,
        after_fp,
        eight_denied_after,
        restored_matches,
        bad_source_rejected,
        bad_flag_rejected,
        count_before,
        count_after,
        missing_errs,
    );
    Ok(())
}

fn frozen_ids() -> Vec<(EngineContentKind, String)> {
    DEEPS_ROWS
        .iter()
        .map(|(kind, id)| (*kind, (*id).to_string()))
        .collect()
}

fn resolvable_count(store: &EngineCalyxContentStore) -> u32 {
    DEEPS_ROWS
        .iter()
        .filter(|(kind, id)| store.get_content_for_editor(*kind, id).is_ok())
        .count() as u32
}

fn infernal_deeps_oracle() -> Result<InfernalDeepsPackageReadback, Box<dyn Error>> {
    let package = InfernalDeepsPackagePolicy::ashen_reach_defaults();
    Ok(derive_infernal_deeps_package(
        DeriveInfernalDeepsPackageArgs {
            package: &package,
            access: &ashen_reach_infernal_access_policy(),
            attrition: &ashen_reach_infernal_attrition_policy(),
            zones: &ashen_reach_infernal_zone_policy(),
            bosses: &InfernalBossAgentPolicy::ashen_reach_defaults(),
            relics: &InfernalRelicVaultPolicy::ashen_reach_defaults(),
            withering: &WitheringRaidPathPolicy::ashen_reach_defaults(),
            undeath: &ashen_reach_undeath_policy(),
        },
    )?)
}

/// The 6 authored records, with the withering minimum party size overridden to `min_party` so the
/// no-recompile-edit edge case can move the gate boundary without touching any Rust literal in the
/// sim path.
fn infernal_deeps_records(min_party: u64) -> Result<Vec<EngineContentRecord>, Box<dyn Error>> {
    let package = InfernalDeepsPackagePolicy::ashen_reach_defaults();
    let readback = infernal_deeps_oracle()?;
    let mut records = infernal_deeps_content_records(&package, &readback)?;
    let withering = records
        .iter_mut()
        .find(|record| record.id == WITHERING_ID)
        .ok_or("withering record missing from builder output")?;
    withering.payload["withering_minimum_party_size"] = json!(min_party);
    withering.source.source_hash = sha256_hex(&serde_json::to_vec(&withering.payload)?);
    Ok(records)
}

fn author_invalid_source_record() -> Vec<EngineContentRecord> {
    // A compiled rpg:// source must be rejected: RPG content has to come from editor/MCP authoring.
    let payload = json!({
        "schema": "litd.rpg.infernal_deeps.v1",
        "surface_kind": "infernal_deeps_zone_topology",
        "zone_count": 7,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
    });
    vec![EngineContentRecord {
        kind: EngineContentKind::Story,
        id: "server.ashen_reach.infernal_deeps.bad".to_string(),
        display_name: "Infernal Deeps: Bad Source".to_string(),
        source: EngineContentSource {
            path: "rpg://infernal_deeps/server.ashen_reach.infernal_deeps.bad".to_string(),
            author: "editor_authoring.issue1455.negative".to_string(),
            source_hash: sha256_hex(&serde_json::to_vec(&payload).unwrap()),
        },
        payload,
        anchors: Vec::new(),
    }]
}

fn author_free_map_flag_record() -> Vec<EngineContentRecord> {
    // A free runtime map/GPS flag must be rejected (diegetic-map guardrail, #944).
    let payload = json!({
        "schema": "litd.rpg.infernal_deeps.v1",
        "surface_kind": "infernal_deeps_zone_topology",
        "zone_count": 7,
        "grants_map_or_minimap": true,
    });
    vec![EngineContentRecord {
        kind: EngineContentKind::Story,
        id: "server.ashen_reach.infernal_deeps.bad_flag".to_string(),
        display_name: "Infernal Deeps: Bad Flag".to_string(),
        source: EngineContentSource {
            path: "editor://rpg/ashen_reach/infernal_deeps/bad_flag".to_string(),
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
