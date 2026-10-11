//! Full State Verification for #1852 -- the single-player campaign FLOW RUNNER.
//!
//! Headless (no GPU / window), the appropriate tier for a deterministic host state
//! machine. It drives the REAL flow over a REAL 2-chapter lowered campaign (built through
//! genuine engine lowering primitives, the same `.lower()` calls the editor valve drives)
//! and proves, with the verdict gating the process (assert-before-report, #800):
//!   1. the 2-chapter campaign package validates (real lowered campaign, not mocked);
//!   2. chapter 1 BOOTS a real melee session and is driven to a genuine victory
//!      (`GameOverEvent` for player 0), and completing it advances to chapter 2;
//!   3. chapter 2 BOOTS its own datasets-backed session (distinct combined_state_hash)
//!      WITH the hero's carried-over level + items rehydrated;
//!   4. the campaign progress + hero payload persist to a save slot (reusing #850
//!      `GameSaveFile`), and an INDEPENDENT readback -- the raw save BYTES re-read from
//!      disk and parsed FRESH -- re-validates the fingerprint and shows the progress head,
//!      the chapter unlock, and the hero payload;
//!   5. two typed negative controls fail LOUD: a circular unlock_after topology, and a
//!      tampered save file.

use std::fs;

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_package::validate_campaign_manifest,
    game_runtime::{
        CampaignFlow, GameSaveFile, HeroProgress, PlayerResult, SessionOrder,
        campaign_flow_fixture::{
            ENEMY_STRUCTURE_UNIT, HERO_KEY, HERO_SLOT_ID, VICTORY_CLEARED, two_chapter_campaign,
        },
    },
};
use serde_json::json;

const SAVE_SLOT: u32 = 1;
const HERO_LEVEL: u32 = 5;
const HERO_XP: u64 = 1_200;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1852_campaign_flow")?;

    // (1) Real 2-chapter lowered campaign; validate it exactly as litd-game does on load.
    let fixture = two_chapter_campaign();
    validate_campaign_manifest(
        &fixture.manifest,
        &fixture.entry_document,
        &fixture.entry_document_fingerprint,
    )?;
    let chapter_count = fixture.manifest.chapters.len();
    let campaign_fingerprint = fixture.manifest.manifest_fingerprint.clone();

    // (2) Boot at ch1, boot its session, drive to a real victory, then advance.
    let mut flow = CampaignFlow::boot(fixture.manifest, SAVE_SLOT)?;
    let boot_chapter = flow.current_chapter_id().to_string();
    let mut ch1_session = flow.instantiate_current_session()?;
    let ch1_hash = ch1_session.combined_state_hash();
    ch1_session.apply(SessionOrder::DestroyUnit {
        unit: ENEMY_STRUCTURE_UNIT,
    })?;
    let ch1_victory = ch1_session
        .match_state()
        .game_over_log()
        .iter()
        .any(|event| event.player == 0 && event.result == PlayerResult::Victory);

    let hero_end = HeroProgress {
        hero_key: HERO_KEY.to_string(),
        level: HERO_LEVEL,
        experience: HERO_XP,
        hp_milli: 800_000,
        item_keys: vec!["item.emberblade".to_string(), "item.ward".to_string()],
    };
    let advance = flow.complete_chapter(
        Some(VICTORY_CLEARED),
        &[(HERO_SLOT_ID.to_string(), hero_end.clone())],
    )?;
    let advanced_to_ch2 = advance.next_chapter_id.as_deref() == Some("ch2")
        && flow.current_chapter_id() == "ch2"
        && !advance.campaign_complete;

    // (3) Chapter 2 boots its OWN session (distinct hash) WITH carried-over hero state.
    let ch2_session = flow.instantiate_current_session()?;
    let ch2_hash = ch2_session.combined_state_hash();
    let carried = flow.rehydrate_hero(HERO_SLOT_ID)?;
    let hero_carried_ok = carried.level == HERO_LEVEL
        && carried.experience == HERO_XP
        && carried.item_keys == hero_end.item_keys
        && carried.hero_key == HERO_KEY;
    let ch2_boots_distinct =
        ch2_hash != ch1_hash && ch2_session.match_state().roster().player_count() == 2;

    // (4) Persist progress + hero, then INDEPENDENTLY re-read the raw save bytes.
    let save_path = run.run_dir().join("campaign-progress.save.ron");
    let in_memory_save = flow.save_to_path(&save_path)?;
    let raw_bytes = fs::read(&save_path)?;
    let raw_sha256 = litd_hash::sha256_hex(&raw_bytes);
    let raw_text = String::from_utf8(raw_bytes.clone())?;
    // Fresh parse from the on-disk bytes (independent of the in-memory object), then the
    // fingerprint gate re-derives from the parsed content -- not a recompute of one buffer.
    let reloaded = GameSaveFile::from_ron_str(&raw_text)?;
    reloaded.validate()?;
    let readback_current = reloaded
        .kv
        .get_text("campaign_progress", "current_chapter")?
        .to_string();
    let readback_completed = reloaded
        .kv
        .get_integer("campaign_progress", "completed_count")?;
    let readback_unlock_ch1 = reloaded.kv.get_integer("chapter_unlock", "ch1")?;
    let readback_hero = reloaded
        .kv
        .get_unit("hero_carryover", HERO_SLOT_ID)?
        .clone();
    let fingerprint_round_trips = reloaded.save_fingerprint == in_memory_save.save_fingerprint;
    let save_readback_ok = readback_current == "ch2"
        && readback_completed == 1
        && readback_unlock_ch1 == 0
        && readback_hero.level == HERO_LEVEL
        && readback_hero.item_keys == hero_end.item_keys
        && fingerprint_round_trips;

    // (5a) Negative control: a circular unlock_after topology fails LOUD at boot.
    let mut cyclic = two_chapter_campaign();
    cyclic.manifest.chapters[0].unlock_after = vec!["ch2".to_string()];
    cyclic.manifest.chapters[1].unlock_after = vec!["ch1".to_string()];
    let cycle_failed_closed = CampaignFlow::boot(cyclic.manifest, SAVE_SLOT).is_err();

    // (5b) Negative control: a tampered save file fails LOUD on load.
    let tampered_path = run.run_dir().join("campaign-progress.tampered.ron");
    fs::write(&tampered_path, raw_text.replacen("ch2", "ch9", 1))?;
    let tamper_failed_closed = GameSaveFile::load_from_path(&tampered_path).is_err();

    // Every verdict gates the process (assert-before-report; #1458/#1483 false-green ban).
    assert!(ch1_victory, "chapter 1 must reach a real player-0 victory");
    assert!(
        advanced_to_ch2,
        "victory must advance the flow to chapter 2"
    );
    assert!(
        hero_carried_ok,
        "chapter 2 must rehydrate the carried hero state"
    );
    assert!(
        ch2_boots_distinct,
        "chapter 2 must boot its own distinct session"
    );
    assert!(
        save_readback_ok,
        "independent raw-save readback must show progress + hero payload"
    );
    assert!(
        cycle_failed_closed,
        "circular unlock topology must fail loud"
    );
    assert!(tamper_failed_closed, "tampered save must fail loud on load");

    let report = json!({
        "schema": "litd.issue1852.campaign_flow_fsv.v1",
        "issue": 1852,
        "sufficiency_argument":
            "A 2-chapter campaign (ch1 -> ch2 by both an explicit victory branch and \
             unlock_after) is the smallest topology that exercises every advance path: a real \
             session boots and is driven to a real victory, branch resolution advances, the \
             next chapter boots its OWN datasets (distinct state hash), one hero slot carries \
             level+items across the boundary, and the whole progress persists + re-reads from \
             raw save bytes. Two typed negative controls (cycle, tampered save) prove the \
             fail-loud gates.",
        "part_1_campaign_valid": {
            "chapter_count": chapter_count,
            "manifest_fingerprint": campaign_fingerprint,
            "boot_chapter": boot_chapter,
        },
        "part_2_chapter1_victory": {
            "ch1_combined_state_hash": format!("{ch1_hash:016x}"),
            "player0_victory": ch1_victory,
            "advanced_to_ch2": advanced_to_ch2,
            "completed_chapter": advance.completed_chapter_id,
            "next_chapter": advance.next_chapter_id,
        },
        "part_3_chapter2_boot_with_hero": {
            "ch2_combined_state_hash": format!("{ch2_hash:016x}"),
            "distinct_from_ch1": ch2_hash != ch1_hash,
            "carried_level": carried.level,
            "carried_experience": carried.experience,
            "carried_items": carried.item_keys,
            "hero_carried_ok": hero_carried_ok,
        },
        "part_4_save_independent_readback": {
            "source_of_truth": {
                "save_path": save_path.display().to_string(),
                "raw_bytes": raw_bytes.len(),
                "raw_sha256": raw_sha256,
                "independent_source": "raw save file bytes re-read from disk and parsed fresh",
            },
            "readback_current_chapter": readback_current,
            "readback_completed_count": readback_completed,
            "readback_chapter_unlock_ch1_order": readback_unlock_ch1,
            "readback_hero_level": readback_hero.level,
            "readback_hero_items": readback_hero.item_keys,
            "save_fingerprint": reloaded.save_fingerprint,
            "fingerprint_round_trips": fingerprint_round_trips,
            "save_readback_ok": save_readback_ok,
        },
        "part_5_negative_controls": {
            "circular_unlock_failed_closed": cycle_failed_closed,
            "tampered_save_failed_closed": tamper_failed_closed,
        },
        "expected_actuals": { "all_assertions_passed": true },
        "manual_gate":
            "Read this JSON via Synapse: chapter_count=2, player0_victory=true, advanced_to_ch2=true, \
             hero_carried_ok=true, ch2 hash distinct from ch1, save readback current_chapter=ch2 with \
             hero level 5 + items, and both negative controls failed_closed=true.",
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1852_CAMPAIGN_FLOW_FSV report={} report_sha256={} ch1_victory={} advanced_to_ch2={} hero_carried_ok={} ch2_distinct={} save_readback_ok={} raw_save_sha256={} cycle_failed_closed={} tamper_failed_closed={}",
        readback.path.display(),
        readback.sha256,
        ch1_victory,
        advanced_to_ch2,
        hero_carried_ok,
        ch2_hash != ch1_hash,
        save_readback_ok,
        raw_sha256,
        cycle_failed_closed,
        tamper_failed_closed,
    );
    Ok(())
}
