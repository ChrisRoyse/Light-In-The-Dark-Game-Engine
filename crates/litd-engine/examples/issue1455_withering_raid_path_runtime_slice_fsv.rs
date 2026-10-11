//! #1455 / #1876 Withering raid-path runtime-slice FSV -- sim admits parties using frozen
//! per-entity raid thresholds (not package-summary scalars).

use std::error::Error;

#[path = "fixtures/issue1061_undeath_fixture.rs"]
mod issue1061_undeath_fixture;
#[path = "fixtures/issue1067_infernal_zones_fixture.rs"]
mod issue1067_infernal_zones_fixture;
#[path = "fixtures/issue1070_infernal_attrition_fixture.rs"]
mod issue1070_infernal_attrition_fixture;

use issue1061_undeath_fixture::ashen_reach_undeath_policy;
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
        InfernalBossAgentPolicy, WITHERING_RAID_SURFACE_KIND, WitheringRaidPathAdmission,
        WitheringRaidPathPolicy, WitheringRaidPathRuntime, admit_withering_raid_from_path,
        persist_and_lower_rpg_authored_content, withering_raid_content_records,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1455_withering_raid_path_runtime_slice")?;
    let vault_dir = resolve_engine_content_panel_vault()?;
    println!(
        "ISSUE1455_WITHERING canonical_vault={}",
        vault_dir.display()
    );

    let policy = WitheringRaidPathPolicy::ashen_reach_defaults();
    let undeath = ashen_reach_undeath_policy();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let attrition = ashen_reach_infernal_attrition_policy();

    let store = open_canonical_engine_content_store()?;
    let authored_records =
        withering_raid_content_records(&policy, &undeath, &zones, &bosses, &attrition)?;
    let authored = persist_and_lower_rpg_authored_content(&store, &authored_records)?;

    let frozen_ids: Vec<(EngineContentKind, String)> = authored_records
        .iter()
        .map(|record| (record.kind, record.id.clone()))
        .collect();
    let frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let runtime = WitheringRaidPathRuntime::from_frozen(&frozen)?;

    let parity_party = runtime.raid.minimum_party_size == i64::from(policy.raid.minimum_party_size);
    let parity_level =
        runtime.raid.minimum_character_level == i64::from(policy.raid.minimum_character_level);
    let parity_phases = runtime.phase_count() == policy.raid.phases.len();
    let parity_all = parity_party && parity_level && parity_phases;

    let min_party = runtime.raid.minimum_party_size;
    let min_level = runtime.raid.minimum_character_level;
    let admit_ok = admit_withering_raid_from_path(&runtime, min_party, min_level)
        == WitheringRaidPathAdmission::Admitted;
    let deny_size = matches!(
        admit_withering_raid_from_path(&runtime, min_party - 1, min_level),
        WitheringRaidPathAdmission::Denied { .. }
    );
    let deny_level = matches!(
        admit_withering_raid_from_path(&runtime, min_party, min_level - 1),
        WitheringRaidPathAdmission::Denied { .. }
    );

    let raid_id = policy.raid.raid_id.clone();
    let before_fp = frozen
        .fingerprint(EngineContentKind::Trigger, &raid_id)
        .ok_or("frozen raid missing")?
        .to_string();
    let edited = override_min_party(&authored_records, &raid_id, min_party + 2)?;
    persist_and_lower_rpg_authored_content(&store, &edited)?;
    let edited_frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let edited_runtime = WitheringRaidPathRuntime::from_frozen(&edited_frozen)?;
    let after_fp = edited_frozen
        .fingerprint(EngineContentKind::Trigger, &raid_id)
        .ok_or("edited raid missing")?
        .to_string();
    let after_min = edited_runtime.raid.minimum_party_size;
    let denied_after = matches!(
        admit_withering_raid_from_path(&edited_runtime, min_party, min_level),
        WitheringRaidPathAdmission::Denied { .. }
    );

    persist_and_lower_rpg_authored_content(&store, &authored_records)?;
    let restored =
        WitheringRaidPathRuntime::from_frozen(&FrozenContentTable::freeze(&store, &frozen_ids)?)?
            .raid
            .minimum_party_size;

    let count_before = resolvable_count(&store, &frozen_ids);
    let bad_rejected =
        persist_and_lower_rpg_authored_content(&store, &author_invalid_source_record()).is_err();
    let count_after = resolvable_count(&store, &frozen_ids);
    let missing_loud = store
        .get_content_for_editor(EngineContentKind::Trigger, "raid.withering.nope")
        .is_err();

    let report = json!({
        "schema": "litd.issue1455.withering_raid_path_runtime_slice_fsv.v1",
        "issue": 1876,
        "parent_issue": 1455,
        "scope": "Withering raid-path per-entity runtime slice; admission floors from frozen raid Trigger",
        "source_of_truth": {
            "canonical_vault": vault_dir.display().to_string(),
            "report_path": run.report_path().display().to_string(),
        },
        "expected_actuals": {
            "authored_row_count": authored.row_count,
            "frozen_row_count": frozen.len(),
            "phase_count": runtime.phase_count(),
            "parity_all": parity_all,
            "min_party": min_party,
            "min_level": min_level,
            "admit_at_floors": admit_ok,
            "deny_undersize": deny_size,
            "deny_underlevel": deny_level,
            "edit_before_min_party": min_party,
            "edit_after_min_party": after_min,
            "edit_changed_floor": after_min != min_party,
            "edit_changed_fingerprint": before_fp != after_fp,
            "party_denied_after_edit": denied_after,
            "restored_matches_before": restored == min_party,
            "invalid_source_rejected": bad_rejected,
            "row_count_before_bad": count_before,
            "row_count_after_bad": count_after,
            "missing_id_errors_loud": missing_loud,
        },
    });

    require_bool(&report, "parity_all", true)?;
    require_bool(&report, "admit_at_floors", true)?;
    require_bool(&report, "deny_undersize", true)?;
    require_bool(&report, "deny_underlevel", true)?;
    require_bool(&report, "edit_changed_floor", true)?;
    require_bool(&report, "edit_changed_fingerprint", true)?;
    require_bool(&report, "party_denied_after_edit", true)?;
    require_bool(&report, "restored_matches_before", true)?;
    require_bool(&report, "invalid_source_rejected", true)?;
    require_bool(&report, "missing_id_errors_loud", true)?;
    require_i64(&report, "authored_row_count", 9)?;
    require_i64(&report, "frozen_row_count", 9)?;
    require_i64(&report, "phase_count", 6)?;
    require_i64(&report, "min_party", 8)?;
    require_i64(&report, "min_level", 47)?;
    if count_before != count_after {
        return Err(format!(
            "row count changed on rejected write: {count_before} vs {count_after}"
        )
        .into());
    }

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1455_WITHERING_RAID_PATH_RUNTIME_SLICE_FSV report={} sha256={} rows={} phases={} admit/deny_size/deny_lvl={}/{}/{} edit {}->{} denied_after={} restored={}",
        read.path.display(),
        read.sha256,
        authored.row_count,
        runtime.phase_count(),
        admit_ok,
        deny_size,
        deny_level,
        min_party,
        after_min,
        denied_after,
        restored == min_party,
    );
    Ok(())
}

fn resolvable_count(store: &EngineCalyxContentStore, ids: &[(EngineContentKind, String)]) -> u32 {
    ids.iter()
        .filter(|(kind, id)| store.get_content_for_editor(*kind, id).is_ok())
        .count() as u32
}

fn override_min_party(
    records: &[EngineContentRecord],
    raid_id: &str,
    new_min: i64,
) -> Result<Vec<EngineContentRecord>, Box<dyn Error>> {
    let mut edited = records.to_vec();
    let record = edited
        .iter_mut()
        .find(|record| record.id == raid_id)
        .ok_or("raid record missing")?;
    record.payload["raid"]["minimum_party_size"] = json!(new_min);
    record.source.source_hash = sha256_hex(&serde_json::to_vec(&record.payload)?);
    Ok(edited)
}

fn author_invalid_source_record() -> Vec<EngineContentRecord> {
    let payload = json!({
        "schema": "litd.rpg.withering_raid_path.v1",
        "surface_kind": WITHERING_RAID_SURFACE_KIND,
        "raid": {
            "raid_id": "raid.withering.bad",
            "minimum_party_size": 8,
            "minimum_character_level": 47,
            "pressure_tier_milli": 900,
            "final_guard_agent_id": "agent.bad",
            "target_item_of_power_id": "item.bad",
        },
        "requires_faction_leader_order": true,
    });
    vec![EngineContentRecord {
        kind: EngineContentKind::Trigger,
        id: "raid.withering.bad".to_string(),
        display_name: "Bad Withering Source".to_string(),
        source: EngineContentSource {
            path: "rpg://withering_raid_path/raid.withering.bad".to_string(),
            author: "editor_authoring.issue1876.negative".to_string(),
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
