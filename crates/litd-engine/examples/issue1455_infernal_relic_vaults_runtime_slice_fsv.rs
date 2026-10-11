//! #1455 / #1875 Infernal relic-vaults runtime-slice FSV -- sim reads each hell-forged relic's
//! power floor from frozen Item rows, not Rust literals (mirrors #1874 boss-agents pattern).

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
        INFERNAL_RELIC_ITEM_SURFACE_KIND, InfernalBossAgentPolicy, InfernalRelicRuntimeCatalog,
        InfernalRelicVaultPolicy, RelicQualification, infernal_relic_item_content_records,
        persist_and_lower_rpg_authored_content, relic_power_qualifies,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const BRANDBLADE_ID: &str = "item.infernal.hellforged.brandblade";

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1455_infernal_relic_vaults_runtime_slice")?;
    let vault_dir = resolve_engine_content_panel_vault()?;
    println!("ISSUE1455_RELICS canonical_vault={}", vault_dir.display());

    let relic_policy = InfernalRelicVaultPolicy::ashen_reach_defaults();
    let zones = ashen_reach_infernal_zone_policy();
    let bosses = InfernalBossAgentPolicy::ashen_reach_defaults();
    let attrition = ashen_reach_infernal_attrition_policy();

    let store = open_canonical_engine_content_store()?;
    let authored_records =
        infernal_relic_item_content_records(&relic_policy, &zones, &bosses, &attrition)?;
    let authored = persist_and_lower_rpg_authored_content(&store, &authored_records)?;

    let brandblade_oracle = relic_policy
        .item_set
        .iter()
        .find(|item| item.item_id == BRANDBLADE_ID)
        .ok_or("oracle missing brandblade")?;
    let apex_readback = store.get_content_for_editor(EngineContentKind::Item, BRANDBLADE_ID)?;
    println!(
        "ISSUE1455_RELICS brandblade_readback cx_id={} fp={}",
        apex_readback.cx_id, apex_readback.lowering.fingerprint
    );

    let frozen_ids: Vec<(EngineContentKind, String)> = relic_policy
        .item_set
        .iter()
        .map(|item| (EngineContentKind::Item, item.item_id.clone()))
        .collect();
    let frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let catalog = InfernalRelicRuntimeCatalog::from_frozen(&frozen)?;

    let mut parity_all = catalog.len() == relic_policy.item_set.len();
    for item in &relic_policy.item_set {
        match catalog.item(&item.item_id) {
            Some(frozen_item) => {
                let expected_slot = serde_json::to_value(item.slot)?
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                parity_all &= frozen_item.power_rating_milli == i64::from(item.power_rating_milli)
                    && frozen_item.world_finite_count == i64::from(item.world_finite_count)
                    && frozen_item.source_vault_id == item.source_vault_id
                    && frozen_item.slot == expected_slot;
            }
            None => parity_all = false,
        }
    }

    let brandblade = catalog
        .item(BRANDBLADE_ID)
        .ok_or("frozen catalog missing brandblade")?;
    let floor = brandblade.power_rating_milli;
    let qualify_at_floor =
        relic_power_qualifies(brandblade, floor) == RelicQualification::Qualifies;
    let withhold_below = matches!(
        relic_power_qualifies(brandblade, floor - 1),
        RelicQualification::Withholds { .. }
    );

    let before_fp = frozen
        .fingerprint(EngineContentKind::Item, BRANDBLADE_ID)
        .ok_or("frozen brandblade missing")?
        .to_string();
    let edited_records = override_relic_power(&authored_records, BRANDBLADE_ID, floor + 15)?;
    persist_and_lower_rpg_authored_content(&store, &edited_records)?;
    let edited_frozen = FrozenContentTable::freeze(&store, &frozen_ids)?;
    let edited_catalog = InfernalRelicRuntimeCatalog::from_frozen(&edited_frozen)?;
    let edited_blade = edited_catalog
        .item(BRANDBLADE_ID)
        .ok_or("edited catalog missing brandblade")?;
    let after_fp = edited_frozen
        .fingerprint(EngineContentKind::Item, BRANDBLADE_ID)
        .ok_or("edited brandblade missing")?
        .to_string();
    let after_floor = edited_blade.power_rating_milli;
    let party_withheld_after = matches!(
        relic_power_qualifies(edited_blade, floor),
        RelicQualification::Withholds { .. }
    );

    persist_and_lower_rpg_authored_content(&store, &authored_records)?;
    let restored_floor = InfernalRelicRuntimeCatalog::from_frozen(&FrozenContentTable::freeze(
        &store,
        &frozen_ids,
    )?)?
    .item(BRANDBLADE_ID)
    .map(|item| item.power_rating_milli);

    let count_before = resolvable_count(&store, &frozen_ids);
    let bad_source_rejected =
        persist_and_lower_rpg_authored_content(&store, &author_invalid_source_record()).is_err();
    let count_after = resolvable_count(&store, &frozen_ids);
    let missing_errs = store
        .get_content_for_editor(EngineContentKind::Item, "item.infernal.nope")
        .is_err();

    let report = json!({
        "schema": "litd.issue1455.infernal_relic_vaults_runtime_slice_fsv.v1",
        "issue": 1875,
        "parent_issue": 1455,
        "scope": "Infernal relic-item per-entity runtime slice; sim reads power_rating_milli from frozen Item rows",
        "source_of_truth": {
            "canonical_vault": vault_dir.display().to_string(),
            "report_path": run.report_path().display().to_string(),
        },
        "expected_actuals": {
            "authored_row_count": authored.row_count,
            "frozen_row_count": frozen.len(),
            "catalog_len": catalog.len(),
            "parity_all": parity_all,
            "brandblade_floor": floor,
            "oracle_brandblade_power": brandblade_oracle.power_rating_milli,
            "qualify_at_floor": qualify_at_floor,
            "withhold_below_floor": withhold_below,
            "edit_before_floor": floor,
            "edit_after_floor": after_floor,
            "edit_changed_floor": after_floor != floor,
            "edit_changed_fingerprint": before_fp != after_fp,
            "party_withheld_after_edit": party_withheld_after,
            "restored_matches_before": restored_floor == Some(floor),
            "invalid_source_rejected": bad_source_rejected,
            "row_count_before_bad": count_before,
            "row_count_after_bad": count_after,
            "missing_id_errors_loud": missing_errs,
        },
    });

    require_bool(&report, "parity_all", true)?;
    require_bool(&report, "qualify_at_floor", true)?;
    require_bool(&report, "withhold_below_floor", true)?;
    require_bool(&report, "edit_changed_floor", true)?;
    require_bool(&report, "edit_changed_fingerprint", true)?;
    require_bool(&report, "party_withheld_after_edit", true)?;
    require_bool(&report, "restored_matches_before", true)?;
    require_bool(&report, "invalid_source_rejected", true)?;
    require_bool(&report, "missing_id_errors_loud", true)?;
    require_i64(&report, "authored_row_count", 6)?;
    require_i64(&report, "frozen_row_count", 6)?;
    require_i64(&report, "catalog_len", 6)?;
    if count_before != count_after {
        return Err(format!(
            "row count changed on rejected write: {count_before} vs {count_after}"
        )
        .into());
    }

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1455_INFERNAL_RELIC_VAULTS_RUNTIME_SLICE_FSV report={} sha256={} catalog={} parity={} floor={} qualify/withhold={}/{} edit {}->{} withheld_after={} restored={} bad={} missing={}",
        read.path.display(),
        read.sha256,
        catalog.len(),
        parity_all,
        floor,
        qualify_at_floor,
        withhold_below,
        floor,
        after_floor,
        party_withheld_after,
        restored_floor == Some(floor),
        bad_source_rejected,
        missing_errs,
    );
    Ok(())
}

fn resolvable_count(store: &EngineCalyxContentStore, ids: &[(EngineContentKind, String)]) -> u32 {
    ids.iter()
        .filter(|(kind, id)| store.get_content_for_editor(*kind, id).is_ok())
        .count() as u32
}

fn override_relic_power(
    records: &[EngineContentRecord],
    item_id: &str,
    new_power: i64,
) -> Result<Vec<EngineContentRecord>, Box<dyn Error>> {
    let mut edited = records.to_vec();
    let record = edited
        .iter_mut()
        .find(|record| record.id == item_id)
        .ok_or("relic record missing from builder output")?;
    record.payload["power_rating_milli"] = json!(new_power);
    record.source.source_hash = sha256_hex(&serde_json::to_vec(&record.payload)?);
    Ok(edited)
}

fn author_invalid_source_record() -> Vec<EngineContentRecord> {
    let payload = json!({
        "schema": "litd.rpg.infernal_relic_vaults.v1",
        "surface_kind": INFERNAL_RELIC_ITEM_SURFACE_KIND,
        "item_id": "item.infernal.bad",
        "slot": "weapon",
        "rarity": "legendary",
        "source_vault_id": "vault.bad",
        "guarded_by_ruler_ref": "agent.bad",
        "world_finite_count": 1,
        "power_rating_milli": 980,
    });
    vec![EngineContentRecord {
        kind: EngineContentKind::Item,
        id: "item.infernal.bad".to_string(),
        display_name: "Bad Relic Source".to_string(),
        source: EngineContentSource {
            path: "rpg://infernal_relic_vaults/item.infernal.bad".to_string(),
            author: "editor_authoring.issue1875.negative".to_string(),
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
