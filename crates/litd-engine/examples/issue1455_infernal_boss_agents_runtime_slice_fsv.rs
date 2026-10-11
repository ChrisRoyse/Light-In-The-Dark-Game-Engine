//! #1455 / #1874 Infernal boss-agents runtime-slice FSV - the proof that the SIM consumes
//! editor-authored, Calyx-lowered boss stats at runtime instead of Rust literals.
//!
//! The Infernal Deeps slice (`issue1455_infernal_deeps_runtime_slice_fsv`) proved the runtime-load
//! pattern for the package-level scalars (zone/boss/relic counts, withering thresholds). This slice
//! goes per-entity: it authors the 7 Infernal boss Agent rows, freezes them, and builds an
//! `InfernalBossRuntimeRoster` entirely from the frozen payloads, then a Rust sim mechanic
//! (`boss_engages`) reads each boss's engagement-strength floor out of the FROZEN rows. Re-authoring
//! a boss's minimum strength in the editor moves the in-game engagement gate with no recompile -
//! the exact #1348/#1455 requirement for "mechanics that stay in Rust must read their tuning from
//! lowered data, not literals".
//!
//! Source of truth is the REAL `gameeditor-engine-content-panel` vault under the canonical Calyx
//! root, not a throwaway temp dir. Evidence chain:
//!   1. Author the 7 boss Agent rows into the canonical vault (`infernal_boss_agent_content_records`).
//!   2. Read the apex row back with a SEPARATE op (`get_content_for_editor`).
//!   3. Freeze all 7 rows and build `InfernalBossRuntimeRoster` from the frozen payloads.
//!   4. Parity: every frozen boss's engagement strength / lethality tier / role == the Rust oracle
//!      (`InfernalBossAgentPolicy::ashen_reach_defaults`).
//!   5. Sim behavior: `boss_engages` withholds a below-floor party and engages an at-floor party
//!      using the FROZEN minimum; the apex is the authored `apex_tyrant`.
//!   6. Edges: a no-recompile edit (Cinder Baron min 700 -> 760) flips a 700-strength party from
//!      engaged to withheld and changes the lowered fingerprint; an invalid record fails closed and
//!      leaves the canonical row count unchanged; a missing id errors loud.

use std::error::Error;

#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

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
        BossEngagement, INFERNAL_BOSS_AGENT_SURFACE_KIND, InfernalBossAgentPolicy,
        InfernalBossAgentRole, InfernalBossRuntimeRoster, boss_engages,
        infernal_boss_agent_content_records, persist_and_lower_rpg_authored_content,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

// The Cinder Baron guards the Ember Wastes with an authored engagement floor of 700 milli.
const CINDER_BARON_ID: &str = "agent.infernal.ember_wastes.cinder_baron";

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1455_infernal_boss_agents_runtime_slice")?;
    let vault_dir = resolve_engine_content_panel_vault()?;
    println!("ISSUE1455_BOSSES canonical_vault={}", vault_dir.display());

    // --- 0. ORACLE: the pre-migration Rust policy (the parity target) ------------------------
    let policy = InfernalBossAgentPolicy::ashen_reach_defaults();
    let zones = ashen_reach_infernal_zone_policy();
    let attrition = ashen_reach_infernal_attrition_policy();

    // --- 1. AUTHOR: 7 boss Agent rows -> canonical vault -------------------------------------
    let store = open_canonical_engine_content_store()?;
    let authored_records = infernal_boss_agent_content_records(&policy, &zones, &attrition)?;
    let authored = persist_and_lower_rpg_authored_content(&store, &authored_records)?;

    // --- 2. READBACK (separate op) on the apex row -------------------------------------------
    let apex_id = apex_agent_id(&policy)?;
    let apex_readback = store.get_content_for_editor(EngineContentKind::Agent, &apex_id)?;
    println!(
        "ISSUE1455_BOSSES apex_readback cx_id={} input_sha256={} lowered_fingerprint={}",
        apex_readback.cx_id, apex_readback.input_sha256, apex_readback.lowering.fingerprint
    );

    // --- 3. RUNTIME LOAD: freeze all 7 rows, build the roster from frozen payloads -----------
    let frozen_ids = frozen_ids(&policy);
    let frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let roster = InfernalBossRuntimeRoster::from_frozen(&frozen)?;

    // --- 4. PARITY: every frozen boss == the Rust oracle -------------------------------------
    let mut parity_all = roster.len() == policy.boss_agents.len();
    for boss in &policy.boss_agents {
        let expected_role = serde_json::to_value(boss.role)?
            .as_str()
            .unwrap_or_default()
            .to_string();
        match roster.boss(&boss.calyx_agent_id) {
            Some(frozen_boss) => {
                parity_all &= frozen_boss.engagement_strength_milli
                    == i64::from(boss.tactic_profile.min_strength_milli)
                    && frozen_boss.lethality_tier == i64::from(boss.lethality_tier)
                    && frozen_boss.role == expected_role;
            }
            None => parity_all = false,
        }
    }

    // --- 5. SIM BEHAVIOR: the gate uses the FROZEN engagement floor --------------------------
    let cinder = roster
        .boss(CINDER_BARON_ID)
        .ok_or("frozen roster missing the Cinder Baron")?;
    let floor = cinder.engagement_strength_milli; // authored 700
    let engage_at_floor = boss_engages(cinder, floor) == BossEngagement::Engages;
    let withhold_below = matches!(
        boss_engages(cinder, floor - 1),
        BossEngagement::Withholds { .. }
    );
    let apex_is_tyrant = roster.apex().map(|boss| boss.agent_id.clone()) == Some(apex_id.clone());

    // --- 6a. EDGE: no-recompile edit (Cinder Baron floor 700 -> 760) withholds the same party --
    let before_fp = frozen
        .fingerprint(EngineContentKind::Agent, CINDER_BARON_ID)
        .ok_or("frozen Cinder Baron row missing")?
        .to_string();
    let edited_records = override_boss_floor(&authored_records, CINDER_BARON_ID, 760)?;
    persist_and_lower_rpg_authored_content(&store, &edited_records)?;
    let edited_frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let edited_roster = InfernalBossRuntimeRoster::from_frozen(&edited_frozen)?;
    let edited_cinder = edited_roster
        .boss(CINDER_BARON_ID)
        .ok_or("edited roster missing the Cinder Baron")?;
    let after_fp = edited_frozen
        .fingerprint(EngineContentKind::Agent, CINDER_BARON_ID)
        .ok_or("edited Cinder Baron row missing")?
        .to_string();
    let after_floor = edited_cinder.engagement_strength_milli;
    let party_withheld_after = matches!(
        boss_engages(edited_cinder, floor),
        BossEngagement::Withholds { .. }
    );
    let edit_changed_floor = after_floor != floor;
    let edit_changed_fingerprint = before_fp != after_fp;

    // Restore the canonical vault to the known-good authored state.
    persist_and_lower_rpg_authored_content(&store, &authored_records)?;
    let restored_floor =
        InfernalBossRuntimeRoster::from_frozen(&FrozenContentTable::freeze(&store, &frozen_ids)?)?
            .boss(CINDER_BARON_ID)
            .map(|boss| boss.engagement_strength_milli);
    let restored_matches = restored_floor == Some(floor);

    // --- 6b. EDGE: invalid record fails closed, canonical row count unchanged -----------------
    let count_before = resolvable_count(&store, &frozen_ids);
    let bad_source_rejected =
        persist_and_lower_rpg_authored_content(&store, &author_invalid_source_record()).is_err();
    let count_after = resolvable_count(&store, &frozen_ids);
    let missing_errs = store
        .get_content_for_editor(EngineContentKind::Agent, "agent.infernal.nope")
        .is_err();

    let report = json!({
        "schema": "litd.issue1455.infernal_boss_agents_runtime_slice_fsv.v1",
        "issue": 1874,
        "parent_issue": 1455,
        "scope": "Infernal boss-agents per-entity runtime slice; the sim reads each boss's engagement floor from lowered data, not Rust literals",
        "source_of_truth": {
            "canonical_vault": vault_dir.display().to_string(),
            "report_path": run.report_path().display().to_string(),
        },
        "expected_actuals": {
            "authored_row_count": authored.row_count,
            "frozen_row_count": frozen.len(),
            "roster_len": roster.len(),
            "parity_all": parity_all,
            "cinder_floor": floor,
            "engage_at_floor": engage_at_floor,
            "withhold_below_floor": withhold_below,
            "apex_is_authored_tyrant": apex_is_tyrant,
            "edit_before_floor": floor,
            "edit_after_floor": after_floor,
            "edit_changed_floor": edit_changed_floor,
            "edit_changed_fingerprint": edit_changed_fingerprint,
            "party_withheld_after_edit": party_withheld_after,
            "restored_matches_before": restored_matches,
            "invalid_source_rejected": bad_source_rejected,
            "row_count_before_bad": count_before,
            "row_count_after_bad": count_after,
            "missing_id_errors_loud": missing_errs,
        },
    });

    require_bool(&report, "parity_all", true)?;
    require_bool(&report, "engage_at_floor", true)?;
    require_bool(&report, "withhold_below_floor", true)?;
    require_bool(&report, "apex_is_authored_tyrant", true)?;
    require_bool(&report, "edit_changed_floor", true)?;
    require_bool(&report, "edit_changed_fingerprint", true)?;
    require_bool(&report, "party_withheld_after_edit", true)?;
    require_bool(&report, "restored_matches_before", true)?;
    require_bool(&report, "invalid_source_rejected", true)?;
    require_bool(&report, "missing_id_errors_loud", true)?;
    require_i64(&report, "authored_row_count", 7)?;
    require_i64(&report, "frozen_row_count", 7)?;
    require_i64(&report, "roster_len", 7)?;
    require_i64(&report, "cinder_floor", 700)?;
    require_i64(&report, "edit_after_floor", 760)?;
    if count_before != count_after {
        return Err(format!(
            "canonical row count changed on a rejected write: before={count_before} after={count_after}"
        )
        .into());
    }

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1455_INFERNAL_BOSS_AGENTS_RUNTIME_SLICE_FSV report={} sha256={} roster={} parity={} gate floor {}=engage/{}=withhold {}/{} apex={} edit {}->{} fp {}->{} withheld_after={} restored={} bad_rejected={} count {}=={} missing_loud={}",
        read.path.display(),
        read.sha256,
        roster.len(),
        parity_all,
        floor,
        floor - 1,
        engage_at_floor,
        withhold_below,
        apex_is_tyrant,
        floor,
        after_floor,
        before_fp,
        after_fp,
        party_withheld_after,
        restored_matches,
        bad_source_rejected,
        count_before,
        count_after,
        missing_errs,
    );
    Ok(())
}

fn frozen_ids(policy: &InfernalBossAgentPolicy) -> Vec<(EngineContentKind, String)> {
    policy
        .boss_agents
        .iter()
        .map(|boss| (EngineContentKind::Agent, boss.calyx_agent_id.clone()))
        .collect()
}

fn apex_agent_id(policy: &InfernalBossAgentPolicy) -> Result<String, Box<dyn Error>> {
    policy
        .boss_agents
        .iter()
        .find(|boss| matches!(boss.role, InfernalBossAgentRole::ApexTyrant))
        .map(|boss| boss.calyx_agent_id.clone())
        .ok_or_else(|| "policy has no apex tyrant".into())
}

fn resolvable_count(store: &EngineCalyxContentStore, ids: &[(EngineContentKind, String)]) -> u32 {
    ids.iter()
        .filter(|(kind, id)| store.get_content_for_editor(*kind, id).is_ok())
        .count() as u32
}

/// Clone the authored records and override one boss's `min_strength_milli`, recomputing its source
/// hash so the edited record persists - the no-recompile-edit edge case moves the gate boundary
/// without touching any Rust literal in the sim path.
fn override_boss_floor(
    records: &[EngineContentRecord],
    agent_id: &str,
    new_floor: u64,
) -> Result<Vec<EngineContentRecord>, Box<dyn Error>> {
    let mut edited = records.to_vec();
    let record = edited
        .iter_mut()
        .find(|record| record.id == agent_id)
        .ok_or("boss record missing from builder output")?;
    record.payload["min_strength_milli"] = json!(new_floor);
    record.source.source_hash = sha256_hex(&serde_json::to_vec(&record.payload)?);
    Ok(edited)
}

fn author_invalid_source_record() -> Vec<EngineContentRecord> {
    // A compiled rpg:// source must be rejected: RPG content has to come from editor/MCP authoring.
    let payload = json!({
        "schema": "litd.rpg.infernal_boss_agents.v1",
        "surface_kind": INFERNAL_BOSS_AGENT_SURFACE_KIND,
        "agent_id": "agent.infernal.bad",
        "boss_id": "boss.infernal.bad",
        "circle_id": "circle.infernal.bad",
        "role": "circle_lord",
        "lethality_tier": 7,
        "min_strength_milli": 650,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
    });
    vec![EngineContentRecord {
        kind: EngineContentKind::Agent,
        id: "agent.infernal.bad".to_string(),
        display_name: "Infernal Boss: Bad Source".to_string(),
        source: EngineContentSource {
            path: "rpg://infernal_boss_agents/agent.infernal.bad".to_string(),
            author: "editor_authoring.issue1874.negative".to_string(),
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
