//! Real Calyx FSV for #1755: one rank-51 Berserker Clan-Chief per clan.

use std::sync::Arc;

use litd_engine::{
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig, EngineCalyxRecordKind},
    fsv_artifacts::begin_issue_run,
    rpg::{
        BERSERKER_CLAN_CHIEF_POLICY_ID, BERSERKER_CLAN_CHIEF_RANK, FactionRosterPolicy,
        admit_clan_chief, persist_faction_roster_policy, read_faction_roster_state,
        release_clan_chief, transfer_clan_chief,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

#[path = "../src/rpg/class_specs_fixture_berserker.rs"]
mod berserker_fixture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1755_faction_roster")?;
    let vault_dir = run.artifact_dir("calyx-vault")?;
    let config = EngineCalyxDatabaseConfig::for_vault(&vault_dir);
    let database = EngineCalyxDatabase::open(config.clone())?;
    let class_spec = berserker_fixture::berserker_class_spec();
    let policy = FactionRosterPolicy {
        schema: "litd.rpg.faction_roster_policy.v1".to_string(),
        policy_id: BERSERKER_CLAN_CHIEF_POLICY_ID.to_string(),
        class_spec_id: class_spec.class_id.clone(),
        role_id: "role.berserker.clan_chief".to_string(),
        rank: BERSERKER_CLAN_CHIEF_RANK,
    };
    persist_faction_roster_policy(&database, &policy)?;
    let clan_id = "clan.golden-ash";
    let before = read_faction_roster_state(&database, clan_id, &policy.policy_id)?;

    let shared = Arc::new(config);
    let candidates = ["candidate.alpha", "candidate.beta"];
    let workers = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| {
            let config = Arc::clone(&shared);
            std::thread::spawn(move || {
                let database = EngineCalyxDatabase::open((*config).clone())?;
                admit_clan_chief(
                    &database,
                    BERSERKER_CLAN_CHIEF_POLICY_ID,
                    "clan.golden-ash",
                    candidate,
                    if index == 0 {
                        "event.concurrent.alpha"
                    } else {
                        "event.concurrent.beta"
                    },
                )
            })
        });
    let mut decisions = Vec::new();
    for worker in workers {
        decisions.push(
            worker
                .join()
                .map_err(|_| std::io::Error::other("roster worker panicked"))??,
        );
    }
    let after_concurrent = EngineCalyxDatabase::open((*shared).clone())?;
    let concurrent_state =
        read_faction_roster_state(&after_concurrent, clan_id, BERSERKER_CLAN_CHIEF_POLICY_ID)?;
    let accepted = decisions
        .iter()
        .filter(|decision| decision.accepted)
        .count();
    let refused = decisions
        .iter()
        .filter(|decision| !decision.accepted)
        .count();
    let winner = concurrent_state
        .holder
        .as_ref()
        .ok_or("concurrent admission did not persist a holder")?
        .candidate_id
        .clone();

    let stale_release = release_clan_chief(
        &after_concurrent,
        BERSERKER_CLAN_CHIEF_POLICY_ID,
        clan_id,
        "candidate.stale",
        "event.stale.release",
    )?;
    let transferred_to = "candidate.transfer";
    let transfer = transfer_clan_chief(
        &after_concurrent,
        BERSERKER_CLAN_CHIEF_POLICY_ID,
        clan_id,
        &winner,
        transferred_to,
        "event.explicit.transfer",
    )?;
    let after_transfer =
        read_faction_roster_state(&after_concurrent, clan_id, BERSERKER_CLAN_CHIEF_POLICY_ID)?;
    let empty_clan_error = admit_clan_chief(
        &after_concurrent,
        BERSERKER_CLAN_CHIEF_POLICY_ID,
        "",
        "candidate.empty-clan",
        "event.empty-clan",
    )
    .is_err();
    let stale_transfer = transfer_clan_chief(
        &after_concurrent,
        BERSERKER_CLAN_CHIEF_POLICY_ID,
        clan_id,
        "candidate.stale",
        "candidate.invalid-transfer",
        "event.stale.transfer",
    )?;
    let stale_transfer_rejected = !stale_transfer.accepted;
    let policy_row = after_concurrent.get_record(
        EngineCalyxRecordKind::FactionRosterPolicy,
        BERSERKER_CLAN_CHIEF_POLICY_ID,
    )?;
    let audit_rows = [
        "event.concurrent.alpha",
        "event.concurrent.beta",
        "event.stale.release",
        "event.explicit.transfer",
        "event.stale.transfer",
    ]
    .into_iter()
    .map(|id| after_concurrent.get_record(EngineCalyxRecordKind::FactionRosterAudit, id))
    .collect::<Result<Vec<_>, _>>()?;
    let report = json!({
        "schema": "litd.issue1755.faction_roster_fsv.v1",
        "source_of_truth": "reloaded Calyx policy/state/audit rows",
        "policy_class_spec_id": policy.class_spec_id,
        "policy_rank": policy.rank,
        "before": before,
        "concurrent": {"accepted": accepted, "refused": refused, "state": concurrent_state, "winner": winner},
        "stale_release_refused": !stale_release.accepted,
        "transfer_accepted": transfer.accepted,
        "stale_transfer": stale_transfer,
        "after_transfer": after_transfer,
        "empty_clan_rejected": empty_clan_error,
        "stale_transfer_rejected": stale_transfer_rejected,
        "policy_row": policy_row.payload,
        "audit_rows": audit_rows.into_iter().map(|row| row.payload).collect::<Vec<_>>(),
    });
    let report_readback = run.write_report(&report)?;
    let report_path = report_readback.path;
    let bytes = std::fs::read(&report_path)?;
    let report_json: serde_json::Value = serde_json::from_slice(&bytes)?;
    if report_json["concurrent"]["accepted"] != 1
        || report_json["concurrent"]["refused"] != 1
        || report_json["stale_release_refused"] != true
        || report_json["transfer_accepted"] != true
        || report_json["empty_clan_rejected"] != true
        || report_json["stale_transfer_rejected"] != true
        || report_json["after_transfer"]["holder"]["candidate_id"] != transferred_to
    {
        return Err("#1755 FSV verdict failed".into());
    }
    println!(
        "ISSUE1755_FACTION_ROSTER_FSV report={} bytes={} sha256={} accepted={} refused={} final_holder={}",
        report_path.display(),
        bytes.len(),
        sha256_hex(&bytes),
        accepted,
        refused,
        transferred_to
    );
    Ok(())
}
