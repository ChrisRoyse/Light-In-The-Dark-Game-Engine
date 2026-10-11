//! #1852 / #1853 deterministic unit coverage for the campaign flow runner. Uses the real
//! lowered campaign fixtures (genuine engine lowering) -- no mocked data.

use super::super::campaign_flow_fixture::{
    ENEMY_STRUCTURE_UNIT, HERO_KEY, HERO_SLOT_ID, MIREN_HERO_KEY, MIREN_SLOT_ID, VICTORY_CLEARED,
    three_chapter_permanent_death_campaign, two_chapter_campaign,
};
use super::super::{
    CAMPAIGN_PROGRESS_CATEGORY, CHAPTER_UNLOCK_CATEGORY, HERO_ROSTER_CATEGORY,
    HERO_ROSTER_STATE_FALLEN, HeroProgress, PlayerResult, SessionOrder,
};
use super::CampaignFlow;

const SAVE_SLOT: u32 = 1;

fn hero_progress() -> HeroProgress {
    HeroProgress {
        hero_key: HERO_KEY.to_string(),
        level: 5,
        experience: 1_200,
        hp_milli: 800_000,
        item_keys: vec!["item.emberblade".to_string(), "item.ward".to_string()],
    }
}

fn active_slots() -> Vec<(String, HeroProgress)> {
    vec![(HERO_SLOT_ID.to_string(), hero_progress())]
}

fn booted() -> CampaignFlow {
    let fixture = two_chapter_campaign();
    CampaignFlow::boot(fixture.manifest, SAVE_SLOT).expect("boot campaign flow")
}

#[test]
fn boot_starts_at_entry_chapter_with_progress_head() {
    let flow = booted();
    assert_eq!(flow.current_chapter_id(), "ch1");
    assert!(flow.completed_chapters().is_empty());
    assert_eq!(
        flow.kv()
            .get_text(CAMPAIGN_PROGRESS_CATEGORY, "current_chapter")
            .unwrap(),
        "ch1"
    );
    assert_eq!(
        flow.kv()
            .get_integer(CAMPAIGN_PROGRESS_CATEGORY, "completed_count")
            .unwrap(),
        0
    );
}

#[test]
fn chapter_one_boots_a_real_match_session_to_victory() {
    let flow = booted();
    assert!(flow.current_chapter_boots_match());
    let mut session = flow
        .instantiate_current_session()
        .expect("chapter 1 boots a session");
    assert_eq!(session.match_state().roster().player_count(), 2);
    // Destroy the AI's only structure -> player 0 wins the melee.
    session
        .apply(SessionOrder::DestroyUnit {
            unit: ENEMY_STRUCTURE_UNIT,
        })
        .expect("destroy enemy structure");
    let won = session
        .match_state()
        .game_over_log()
        .iter()
        .any(|event| event.player == 0 && event.result == PlayerResult::Victory);
    assert!(
        won,
        "player 0 must win once the enemy structure is destroyed"
    );
}

#[test]
fn victory_advances_via_branch_on_victory() {
    let mut flow = booted();
    let advance = flow
        .complete_chapter(Some(VICTORY_CLEARED), &active_slots())
        .expect("complete chapter 1 on the cleared branch");
    assert_eq!(advance.completed_chapter_id, "ch1");
    assert_eq!(advance.next_chapter_id.as_deref(), Some("ch2"));
    assert!(!advance.campaign_complete);
    assert_eq!(flow.current_chapter_id(), "ch2");
    assert_eq!(flow.completed_chapters(), ["ch1"]);
    // Chapter 2 boots its OWN datasets-backed session.
    assert!(flow.current_chapter_boots_match());
    let session = flow.instantiate_current_session().expect("chapter 2 boots");
    assert_eq!(session.match_state().roster().player_count(), 2);
}

#[test]
fn victory_advances_via_unlock_after_without_a_branch_condition() {
    let mut flow = booted();
    // No victory condition -> fall back to the unlock_after DAG (ch2 unlock_after ch1).
    let advance = flow
        .complete_chapter(None, &active_slots())
        .expect("complete chapter 1");
    assert_eq!(advance.next_chapter_id.as_deref(), Some("ch2"));
    assert_eq!(flow.current_chapter_id(), "ch2");
}

#[test]
fn hero_carryover_round_trips_across_the_chapter_boundary() {
    let mut flow = booted();
    flow.complete_chapter(Some(VICTORY_CLEARED), &active_slots())
        .expect("advance");
    // At chapter 2 start the hero is rehydrated with the carried level + items.
    let carried = flow.rehydrate_hero(HERO_SLOT_ID).expect("rehydrate hero");
    let expected = hero_progress();
    assert_eq!(carried.level, expected.level);
    assert_eq!(carried.experience, expected.experience);
    assert_eq!(carried.item_keys, expected.item_keys);
    assert_eq!(carried.hero_key, HERO_KEY);
}

#[test]
fn hero_key_mismatch_fails_loud() {
    let mut flow = booted();
    let mut bad = hero_progress();
    bad.hero_key = "hero_impostor".to_string();
    let error = flow
        .complete_chapter(Some(VICTORY_CLEARED), &[(HERO_SLOT_ID.to_string(), bad)])
        .unwrap_err()
        .to_string();
    assert!(error.contains("hero_impostor"), "error was {error}");
}

#[test]
fn missing_active_hero_slot_fails_loud() {
    let mut flow = booted();
    let error = flow
        .complete_chapter(Some(VICTORY_CLEARED), &[])
        .unwrap_err()
        .to_string();
    assert!(error.contains(HERO_SLOT_ID), "error was {error}");
}

#[test]
fn unknown_hero_slot_fails_loud() {
    let mut flow = booted();
    let error = flow
        .complete_chapter(
            Some(VICTORY_CLEARED),
            &[("slot_ghost".to_string(), hero_progress())],
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("slot_ghost"), "error was {error}");
}

#[test]
fn campaign_completes_after_the_final_chapter() {
    let mut flow = booted();
    flow.complete_chapter(Some(VICTORY_CLEARED), &active_slots())
        .expect("advance to ch2");
    let advance = flow
        .complete_chapter(None, &active_slots())
        .expect("complete ch2");
    assert_eq!(advance.completed_chapter_id, "ch2");
    assert_eq!(advance.next_chapter_id, None);
    assert!(advance.campaign_complete);
    assert_eq!(flow.completed_chapters(), ["ch1", "ch2"]);
}

#[test]
fn save_captures_progress_and_hero_payload() {
    let mut flow = booted();
    flow.complete_chapter(Some(VICTORY_CLEARED), &active_slots())
        .expect("advance");
    let save = flow.capture_save().expect("capture save");
    save.validate().expect("save fingerprint valid");
    // Progress head + chapter unlock + hero carry-over all live in the save's KV store.
    assert_eq!(
        save.kv
            .get_text(CAMPAIGN_PROGRESS_CATEGORY, "current_chapter")
            .unwrap(),
        "ch2"
    );
    assert_eq!(
        save.kv
            .get_integer(CAMPAIGN_PROGRESS_CATEGORY, "completed_count")
            .unwrap(),
        1
    );
    assert_eq!(
        save.kv.get_integer(CHAPTER_UNLOCK_CATEGORY, "ch1").unwrap(),
        0
    );
    let hero = save.kv.get_unit("hero_carryover", HERO_SLOT_ID).unwrap();
    assert_eq!(hero.level, 5);
    assert_eq!(hero.item_keys, ["item.emberblade", "item.ward"]);
}

#[test]
fn circular_unlock_topology_fails_loud() {
    let mut fixture = two_chapter_campaign();
    // ch1 <-> ch2 mutual unlock_after: a cycle no chapter can ever satisfy.
    fixture.manifest.chapters[0].unlock_after = vec!["ch2".to_string()];
    fixture.manifest.chapters[1].unlock_after = vec!["ch1".to_string()];
    let error = CampaignFlow::boot(fixture.manifest, SAVE_SLOT)
        .unwrap_err()
        .to_string();
    assert!(error.contains("cycle"), "error was {error}");
}

#[test]
fn chapter_without_embedded_datasets_cannot_boot_a_match() {
    let mut fixture = two_chapter_campaign();
    fixture.manifest.chapter_runtime_datasets.remove("map_a");
    let flow = CampaignFlow::boot(fixture.manifest, SAVE_SLOT).expect("boot");
    assert!(!flow.current_chapter_boots_match());
    // MatchRuntimeSession is not Debug, so take the error via .err().unwrap().
    let error = flow
        .instantiate_current_session()
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("no runtime datasets") || error.contains("embeds no runtime"));
}

fn miren_progress() -> HeroProgress {
    HeroProgress {
        hero_key: MIREN_HERO_KEY.to_string(),
        level: 4,
        experience: 900,
        hp_milli: 700_000,
        item_keys: vec!["item.miren_charm".to_string()],
    }
}

fn both_heroes_m2() -> Vec<(String, HeroProgress)> {
    vec![
        (HERO_SLOT_ID.to_string(), hero_progress()),
        (MIREN_SLOT_ID.to_string(), miren_progress()),
    ]
}

/// #1853: Miren rehydrates into m3, is script-tombstoned mid-m3, and is absent from m5.
#[test]
fn permanent_death_tombstone_skips_later_chapter_seeding() {
    let fixture = three_chapter_permanent_death_campaign();
    let mut flow = CampaignFlow::boot(fixture.manifest, SAVE_SLOT).expect("boot m2");
    assert_eq!(flow.current_chapter_id(), "m2");
    let m2_seed = flow
        .active_hero_slot_ids_for_current_chapter()
        .expect("m2 seed");
    assert_eq!(m2_seed, [HERO_SLOT_ID, MIREN_SLOT_ID]);

    flow.complete_chapter(Some(VICTORY_CLEARED), &both_heroes_m2())
        .expect("m2 -> m3");
    assert_eq!(flow.current_chapter_id(), "m3");
    let miren_at_m3 = flow.rehydrate_hero(MIREN_SLOT_ID).expect("Miren into m3");
    assert_eq!(miren_at_m3.level, 4);
    assert_eq!(miren_at_m3.item_keys, ["item.miren_charm"]);

    // Scripted permanent death mid-m3.
    flow.tombstone_hero(MIREN_SLOT_ID).expect("tombstone Miren");
    assert!(flow.is_hero_fallen(MIREN_SLOT_ID).expect("fallen query"));
    let revival_err = flow
        .assert_revival_allowed(MIREN_SLOT_ID)
        .unwrap_err()
        .to_string();
    assert!(
        revival_err.contains("FALLEN") || revival_err.contains("revival"),
        "revival must refuse: {revival_err}"
    );
    let rehydrate_err = flow.rehydrate_hero(MIREN_SLOT_ID).unwrap_err().to_string();
    assert!(
        rehydrate_err.contains("FALLEN") || rehydrate_err.contains("inactive"),
        "rehydrate must fail: {rehydrate_err}"
    );

    // Completing m3 only requires the surviving active hero (Miren is inactive).
    flow.complete_chapter(
        Some(VICTORY_CLEARED),
        &[(HERO_SLOT_ID.to_string(), hero_progress())],
    )
    .expect("m3 -> m5");
    assert_eq!(flow.current_chapter_id(), "m5");
    let m5_seed = flow
        .active_hero_slot_ids_for_current_chapter()
        .expect("m5 seed");
    assert_eq!(
        m5_seed,
        [HERO_SLOT_ID.to_string()],
        "Miren must be absent from m5 roster"
    );
    assert!(!m5_seed.iter().any(|id| id == MIREN_SLOT_ID));

    // Source of truth: campaign save KV holds the FALLEN tombstone.
    let save = flow.capture_save().expect("capture");
    let roster_state = save
        .kv
        .get_integer(HERO_ROSTER_CATEGORY, MIREN_SLOT_ID)
        .expect("hero_roster tombstone present");
    assert_eq!(roster_state, HERO_ROSTER_STATE_FALLEN);
}

#[test]
fn permanent_death_rejects_progress_for_fallen_slot() {
    let fixture = three_chapter_permanent_death_campaign();
    let mut flow = CampaignFlow::boot(fixture.manifest, SAVE_SLOT).expect("boot");
    flow.complete_chapter(Some(VICTORY_CLEARED), &both_heroes_m2())
        .expect("to m3");
    flow.tombstone_hero(MIREN_SLOT_ID).expect("tombstone");
    let err = flow
        .complete_chapter(Some(VICTORY_CLEARED), &both_heroes_m2())
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(MIREN_SLOT_ID) || err.contains("inactive") || err.contains("FALLEN"),
        "error was {err}"
    );
}
