//! Full State Verification for #1853 -- permanent hero death (roster tombstone).
//!
//! Headless: drives the real campaign flow over a real 3-chapter lowered campaign
//! (m2 -> m3 -> m5). Proves Miren rehydrates into m3, is script-tombstoned mid-m3,
//! is absent from m5 seeding, and the on-disk save KV holds `hero_roster` FALLEN.
//! Edge cases (revival refuse, rehydrate refuse, inactive progress) fail loud.
//! Source of truth: independent re-read of the save file bytes from disk.

use std::fs;

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_package::validate_campaign_manifest,
    game_runtime::{
        CampaignFlow, GameSaveFile, HERO_ROSTER_CATEGORY, HERO_ROSTER_STATE_FALLEN, HeroProgress,
        campaign_flow_fixture::{
            HERO_KEY, HERO_SLOT_ID, MIREN_HERO_KEY, MIREN_SLOT_ID, VICTORY_CLEARED,
            three_chapter_permanent_death_campaign,
        },
    },
};
use serde_json::json;

const SAVE_SLOT: u32 = 7;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1853_permanent_hero_death")?;

    let fixture = three_chapter_permanent_death_campaign();
    validate_campaign_manifest(
        &fixture.manifest,
        &fixture.entry_document,
        &fixture.entry_document_fingerprint,
    )?;

    let mut flow = CampaignFlow::boot(fixture.manifest, SAVE_SLOT)?;
    assert_eq!(flow.current_chapter_id(), "m2");

    let vanguard = HeroProgress {
        hero_key: HERO_KEY.to_string(),
        level: 5,
        experience: 1_200,
        hp_milli: 800_000,
        item_keys: vec!["item.emberblade".to_string()],
    };
    let miren = HeroProgress {
        hero_key: MIREN_HERO_KEY.to_string(),
        level: 4,
        experience: 900,
        hp_milli: 700_000,
        item_keys: vec!["item.miren_charm".to_string()],
    };

    // m2: both heroes active; complete to m3.
    let m2_seed = flow.active_hero_slot_ids_for_current_chapter()?;
    assert_eq!(m2_seed, [HERO_SLOT_ID, MIREN_SLOT_ID]);
    flow.complete_chapter(
        Some(VICTORY_CLEARED),
        &[
            (HERO_SLOT_ID.to_string(), vanguard.clone()),
            (MIREN_SLOT_ID.to_string(), miren.clone()),
        ],
    )?;
    assert_eq!(flow.current_chapter_id(), "m3");

    // m3 start: Miren rehydrates with carried level + items.
    let miren_at_m3 = flow.rehydrate_hero(MIREN_SLOT_ID)?;
    let miren_rehydrated_ok = miren_at_m3.level == 4 && miren_at_m3.item_keys == miren.item_keys;

    // Scripted permanent death mid-m3.
    flow.tombstone_hero(MIREN_SLOT_ID)?;
    let fallen = flow.is_hero_fallen(MIREN_SLOT_ID)?;
    let revival_refused = flow.assert_revival_allowed(MIREN_SLOT_ID).is_err();
    let rehydrate_refused = flow.rehydrate_hero(MIREN_SLOT_ID).is_err();

    // m3 end: only vanguard progress required; advance to m5.
    flow.complete_chapter(
        Some(VICTORY_CLEARED),
        &[(HERO_SLOT_ID.to_string(), vanguard.clone())],
    )?;
    assert_eq!(flow.current_chapter_id(), "m5");
    let m5_seed = flow.active_hero_slot_ids_for_current_chapter()?;
    let miren_absent_from_m5 = m5_seed == [HERO_SLOT_ID.to_string()];

    // Persist + independent disk readback of the FALLEN tombstone (source of truth).
    let save_path = run.run_dir().join("campaign-progress.save.ron");
    let in_memory = flow.save_to_path(&save_path)?;
    let raw_bytes = fs::read(&save_path)?;
    let raw_sha256 = litd_hash::sha256_hex(&raw_bytes);
    let reloaded = GameSaveFile::from_ron_str(&String::from_utf8(raw_bytes)?)?;
    reloaded.validate()?;
    let roster_state = reloaded
        .kv
        .get_integer(HERO_ROSTER_CATEGORY, MIREN_SLOT_ID)?;
    let tombstone_on_disk = roster_state == HERO_ROSTER_STATE_FALLEN;
    let fingerprint_ok = reloaded.save_fingerprint == in_memory.save_fingerprint;
    let progress_ok = reloaded
        .kv
        .get_text("campaign_progress", "current_chapter")?
        == "m5"
        && reloaded
            .kv
            .get_integer("campaign_progress", "completed_count")?
            == 2;

    // Edge: empty progress for active hero fails; progress for fallen fails.
    let mut edge_flow = CampaignFlow::boot(
        three_chapter_permanent_death_campaign().manifest,
        SAVE_SLOT + 1,
    )?;
    edge_flow.complete_chapter(
        Some(VICTORY_CLEARED),
        &[
            (HERO_SLOT_ID.to_string(), vanguard.clone()),
            (MIREN_SLOT_ID.to_string(), miren),
        ],
    )?;
    edge_flow.tombstone_hero(MIREN_SLOT_ID)?;
    let edge_fallen_progress_fails = edge_flow
        .complete_chapter(
            Some(VICTORY_CLEARED),
            &[
                (HERO_SLOT_ID.to_string(), vanguard.clone()),
                (MIREN_SLOT_ID.to_string(), vanguard.clone()),
            ],
        )
        .is_err();
    let edge_missing_active_fails = edge_flow
        .complete_chapter(Some(VICTORY_CLEARED), &[])
        .is_err();

    assert!(miren_rehydrated_ok, "Miren must rehydrate into m3");
    assert!(fallen, "Miren must be FALLEN after tombstone");
    assert!(revival_refused, "revival must refuse FALLEN");
    assert!(rehydrate_refused, "rehydrate must refuse FALLEN");
    assert!(miren_absent_from_m5, "Miren must be absent from m5 seed");
    assert!(tombstone_on_disk, "save must hold hero_roster FALLEN");
    assert!(fingerprint_ok && progress_ok, "independent save readback");
    assert!(
        edge_fallen_progress_fails,
        "progress for FALLEN slot must fail"
    );
    assert!(
        edge_missing_active_fails,
        "missing active hero progress must fail"
    );

    let report = json!({
        "issue": 1853,
        "verdict": "PASS",
        "source_of_truth": {
            "save_path": save_path.to_string_lossy(),
            "save_sha256": raw_sha256,
            "hero_roster_miren": roster_state,
            "hero_roster_fallen_const": HERO_ROSTER_STATE_FALLEN,
            "current_chapter": "m5",
            "m5_seed": m5_seed,
        },
        "happy_path": {
            "miren_rehydrated_into_m3": miren_rehydrated_ok,
            "tombstoned_mid_m3": fallen,
            "revival_refused": revival_refused,
            "rehydrate_refused": rehydrate_refused,
            "miren_absent_from_m5": miren_absent_from_m5,
            "tombstone_on_disk_readback": tombstone_on_disk,
        },
        "edge_cases": {
            "fallen_progress_fails": edge_fallen_progress_fails,
            "missing_active_progress_fails": edge_missing_active_fails,
        },
    });
    let report_path = run.run_dir().join("manual-fsv-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let report_bytes = fs::read(&report_path)?;
    let report_sha = litd_hash::sha256_hex(&report_bytes);
    println!(
        "FSV_PASS issue=1853 report={} sha256={} save_sha256={}",
        report_path.display(),
        report_sha,
        raw_sha256
    );
    Ok(())
}
