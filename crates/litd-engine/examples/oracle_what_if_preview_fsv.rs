use std::fs;
use std::path::Path;

use litd_engine::EngineError;
use serde_json::json;

#[path = "oracle_what_if_preview_fsv/support.rs"]
mod support;

use support::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = fsv_root();
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;

    let cases = vec![
        run_happy(&root)?,
        run_insufficient(&root)?,
        run_cycle(&root)?,
        run_low_confidence(&root)?,
    ];
    let final_readback = json!({
        "artifact_bytes": 0,
        "online_files": files_under(&root, "online")?,
        "recurrence_files": files_under(&root, "recurrence")?,
        "assay_files": files_under(&root, "assay")?,
        "ledger_files": files_under(&root, "ledger")?,
        "persisted_outcomes": cases.iter().map(|case| {
            format!("{}:{:?}", case.name, case.after.outcome)
        }).collect::<Vec<_>>(),
    });
    let artifact = json!({
        "issue": 1197,
        "source_of_truth": "Aster Base CF, Recurrence CF, Assay CF, Online CF preview row, and Ledger CF",
        "cases": cases,
        "final_readback": final_readback,
    });
    let path = root.join("manual-fsv.json");
    fs::write(&path, serde_json::to_vec_pretty(&artifact)?)?;
    let bytes = fs::metadata(&path)?.len();
    let mut readback: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    readback["final_readback"]["artifact_bytes"] = json!(bytes);
    fs::write(&path, serde_json::to_vec_pretty(&readback)?)?;
    println!(
        "FSV_SOURCE_OF_TRUTH_READBACK path={} bytes={} cases={} online_files={} recurrence_files={} assay_files={} ledger_files={}",
        path.display(),
        bytes,
        readback["cases"].as_array().map_or(0, Vec::len),
        readback["final_readback"]["online_files"]
            .as_array()
            .map_or(0, Vec::len),
        readback["final_readback"]["recurrence_files"]
            .as_array()
            .map_or(0, Vec::len),
        readback["final_readback"]["assay_files"]
            .as_array()
            .map_or(0, Vec::len),
        readback["final_readback"]["ledger_files"]
            .as_array()
            .map_or(0, Vec::len),
    );
    Ok(())
}

fn run_happy(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "happy")?;
    seed_oracle_fixture(service.vault(), 1.0, 0.8);
    seed_prediction_rows(
        service.vault(),
        "buff_knight_damage",
        "Overpowered",
        12,
        "enemy_ttk_drop",
    );
    seed_edge(
        service.vault(),
        "enemy_ttk_drop",
        "economy_gold_inflation",
        "GoldInflation",
        true,
    );
    seed_edge(
        service.vault(),
        "economy_gold_inflation",
        "raid_reward_rebalance",
        "NeedsRebalance",
        true,
    );
    let before = service.read_source_truth(DOMAIN, "buff_knight_damage")?;
    let report = service.preview_change(request("buff_knight_damage"))?;
    let after = service.read_source_truth(DOMAIN, "buff_knight_damage")?;
    assert_preview(&report.record.outcome, "enemy_ttk_drop", false, false);
    print_case("happy_known_impact", &before, &after, "ok");
    Ok(case(
        "happy_known_impact",
        "tree contains enemy_ttk_drop -> economy_gold_inflation",
        before,
        after,
        "ok",
    ))
}

fn run_insufficient(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "insufficient")?;
    seed_oracle_fixture(service.vault(), 0.25, 0.8);
    seed_prediction_rows(
        service.vault(),
        "buff_archer_range",
        "Overpowered",
        12,
        "range_lock",
    );
    let before = service.read_source_truth(DOMAIN, "buff_archer_range")?;
    let error = service
        .preview_change(request("buff_archer_range"))
        .expect_err("insufficient must fail closed");
    assert!(matches!(
        error,
        EngineError::OraclePreviewInsufficient { .. }
    ));
    let after = service.read_source_truth(DOMAIN, "buff_archer_range")?;
    print_case("edge_insufficient_bits", &before, &after, error.code());
    Ok(case(
        "edge_insufficient_bits",
        "persisted rejected record with CALYX_ORACLE_INSUFFICIENT",
        before,
        after,
        error.code(),
    ))
}

fn run_cycle(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "cycle")?;
    seed_oracle_fixture(service.vault(), 1.0, 0.8);
    seed_prediction_rows(
        service.vault(),
        "buff_mage_burst",
        "Overpowered",
        12,
        "cooldown_pressure",
    );
    seed_edge(
        service.vault(),
        "cooldown_pressure",
        "buff_mage_burst",
        "BackEdge",
        true,
    );
    let before = service.read_source_truth(DOMAIN, "buff_mage_burst")?;
    let report = service.preview_change(request("buff_mage_burst"))?;
    let after = service.read_source_truth(DOMAIN, "buff_mage_burst")?;
    assert_preview(&report.record.outcome, "cooldown_pressure", true, false);
    print_case("edge_cycle_guarded", &before, &after, "ok");
    Ok(case(
        "edge_cycle_guarded",
        "back edge is omitted from tree",
        before,
        after,
        "ok",
    ))
}

fn run_low_confidence(root: &Path) -> Result<CaseEvidence, Box<dyn std::error::Error>> {
    let mut service = service(root, "low-confidence")?;
    seed_oracle_fixture(service.vault(), 1.0, 0.8);
    seed_prediction_rows(
        service.vault(),
        "buff_rogue_crit",
        "Overpowered",
        1,
        "crit_spike",
    );
    seed_prediction_rows(service.vault(), "buff_rogue_crit", "Fine", 1, "no_change");
    seed_edge(
        service.vault(),
        "crit_spike",
        "loot_spike",
        "LootSpike",
        true,
    );
    let before = service.read_source_truth(DOMAIN, "buff_rogue_crit")?;
    let report = service.preview_change(request("buff_rogue_crit"))?;
    let after = service.read_source_truth(DOMAIN, "buff_rogue_crit")?;
    assert_preview(&report.record.outcome, "", false, true);
    print_case("edge_low_confidence_pruned", &before, &after, "ok");
    Ok(case(
        "edge_low_confidence_pruned",
        "low confidence tree has no children",
        before,
        after,
        "ok",
    ))
}
