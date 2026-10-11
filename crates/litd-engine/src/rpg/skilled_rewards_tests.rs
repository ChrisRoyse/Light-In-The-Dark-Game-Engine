use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        SkilledPlayEvidence, SkilledPlayEvidenceKind, SkilledRewardCandidate, SkilledRewardKind,
        evaluate_skilled_reward, persist_and_lower_rpg_authored_content,
        skilled_reward_content_records, validate_rpg_authored_content_records,
        validate_skilled_reward_policy,
    },
    rpg_skilled_rewards_test_fixture::ashen_reach_skilled_reward_policy,
};

#[test]
fn issue1047_accepts_skilled_play_and_roleplay_for_immortal_reward() {
    let policy = ashen_reach_skilled_reward_policy();
    let readback = evaluate_skilled_reward(&policy, &candidate()).unwrap();

    assert!(readback.accepted);
    assert_eq!(readback.grade, "accepted");
    assert_eq!(readback.skill_score_milli, 844);
    assert_eq!(readback.rp_quality_milli, 780);
    assert_eq!(readback.combined_score_milli, 818);
    assert_eq!(readback.evidence_count, 4);
    assert_eq!(readback.distinct_evidence_kind_count, 4);
    assert_eq!(readback.effect_count, 1);
    assert_eq!(readback.lowered_effects[0].tier, 4);
    assert_eq!(
        readback.lowered_effects[0].approval_actor_id,
        "immortal.korthac.unbroken_witness"
    );
    assert!(readback.lowered_effects[0].revocable);
    assert!(!readback.built_in_map_required);
    assert!(!readback.grants_map_or_minimap);
    assert!(!readback.grants_gps_markers);
    assert!(!readback.result_sha256.is_empty());
}

#[test]
fn issue1047_issue1455_persists_skilled_rewards_as_authored_calyx_content() {
    let policy = ashen_reach_skilled_reward_policy();
    let readback = evaluate_skilled_reward(&policy, &candidate()).unwrap();
    let records = skilled_reward_content_records(&policy, &readback).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault) = content_store("skilled");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();

    assert_eq!(validation.record_count, 3);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);
    assert!(validation.no_self_attesting_anchors);
    assert_eq!(lowered.row_count, 3);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(lowered.entities.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/skilled_rewards/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|row| row.lowered_field_count > 0)
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.kind == EngineContentKind::Lore)
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.kind == EngineContentKind::Story)
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.id == "reward.skill.immortal_boon.candidate.skill.morvane.1")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "reward_policy")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "reward_ledger")
    );
    assert!(
        lowered
            .entities
            .iter()
            .any(|row| row.surface_kind == "reward_effect")
    );
    assert!(
        records
            .iter()
            .all(|row| row.payload["built_in_map_required"] != true)
    );
    assert!(
        records
            .iter()
            .all(|row| row.payload["grants_map_or_minimap"] != true)
    );
    assert!(
        records
            .iter()
            .all(|row| row.payload["grants_gps_markers"] != true)
    );

    let mut drift = readback;
    drift.grants_map_or_minimap = true;
    assert_code(
        skilled_reward_content_records(&policy, &drift),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1047_rejects_gameplay_only_roleplay_only_and_bad_evidence() {
    let policy = ashen_reach_skilled_reward_policy();

    let low_skill = evaluate_skilled_reward(&policy, &low_skill_candidate()).unwrap();
    assert!(!low_skill.accepted);
    assert_eq!(low_skill.grade, "needs_gameplay_skill");
    assert_eq!(low_skill.effect_count, 0);

    let mut low_rp = candidate();
    low_rp.rp_quality_milli = 620;
    let readback = evaluate_skilled_reward(&policy, &low_rp).unwrap();
    assert!(!readback.accepted);
    assert_eq!(readback.grade, "needs_roleplay");
    assert_eq!(readback.effect_count, 0);

    let mut duplicate = candidate();
    duplicate.evidence[1].evidence_id = duplicate.evidence[0].evidence_id.clone();
    assert_code(
        evaluate_skilled_reward(&policy, &duplicate),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut unwitnessed = candidate();
    unwitnessed.evidence[0].witnessed_by_system = false;
    assert_code(
        evaluate_skilled_reward(&policy, &unwitnessed),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut one_kind = candidate();
    for row in &mut one_kind.evidence {
        row.kind = SkilledPlayEvidenceKind::CombatProwess;
    }
    assert_code(
        evaluate_skilled_reward(&policy, &one_kind),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1047_rejects_map_drift_and_unapproved_policy() {
    let mut built_in_map = ashen_reach_skilled_reward_policy();
    built_in_map.built_in_map_required = true;
    assert_code(
        validate_skilled_reward_policy(&built_in_map),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut minimap = ashen_reach_skilled_reward_policy();
    minimap.grants_map_or_minimap = true;
    assert_code(
        validate_skilled_reward_policy(&minimap),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut gps = ashen_reach_skilled_reward_policy();
    gps.grants_gps_markers = true;
    assert_code(
        validate_skilled_reward_policy(&gps),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut unapproved = ashen_reach_skilled_reward_policy();
    unapproved.immortal_or_system_approval_required = false;
    assert_code(
        validate_skilled_reward_policy(&unapproved),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn candidate() -> SkilledRewardCandidate {
    SkilledRewardCandidate {
        candidate_id: "candidate.skill.morvane.1".to_string(),
        character_id: "char.morvane_ash".to_string(),
        character_name: "Morvane Ash".to_string(),
        class_id: "class.unhallowed_knight".to_string(),
        rp_quality_milli: 780,
        requested_reward_kind: SkilledRewardKind::ImmortalBoon,
        immortal_id: Some("immortal.korthac.unbroken_witness".to_string()),
        evidence: skilled_evidence(),
    }
}

fn low_skill_candidate() -> SkilledRewardCandidate {
    let mut candidate = candidate();
    for row in &mut candidate.evidence {
        row.metric_milli = 680;
        row.difficulty_milli = 800;
    }
    candidate
}

fn skilled_evidence() -> Vec<SkilledPlayEvidence> {
    vec![
        evidence(
            "evidence.combat.gatebreak.1",
            SkilledPlayEvidenceKind::CombatProwess,
            920,
            900,
            "combat.gatebreak.1",
            "Held threat while allies recovered from a wipe pattern.",
        ),
        evidence(
            "evidence.pvp.bridge.1",
            SkilledPlayEvidenceKind::PvpVictory,
            880,
            1000,
            "pvp.bridge.1",
            "Won a hostile bridge duel against equal-rank opposition.",
        ),
        evidence(
            "evidence.ability.parry.1",
            SkilledPlayEvidenceKind::AbilityTiming,
            960,
            850,
            "ability.parry.1",
            "Timed an interrupt inside the final cast window.",
        ),
        evidence(
            "evidence.boss.cinder_judge.1",
            SkilledPlayEvidenceKind::BossMechanic,
            900,
            950,
            "boss.cinder_judge.1",
            "Solved rotating fire lines without external markers.",
        ),
    ]
}

fn evidence(
    evidence_id: &str,
    kind: SkilledPlayEvidenceKind,
    metric_milli: u16,
    difficulty_milli: u16,
    source_ref: &str,
    summary: &str,
) -> SkilledPlayEvidence {
    SkilledPlayEvidence {
        evidence_id: evidence_id.to_string(),
        kind,
        metric_milli,
        difficulty_milli,
        witnessed_by_system: true,
        source_ref: source_ref.to_string(),
        summary: summary.to_string(),
    }
}

fn content_store(
    label: &str,
) -> (
    EngineCalyxContentStore,
    EngineCalyxDatabase,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database.clone(), label),
        database,
        vault_dir,
    )
}

// #1535: TestTempDir auto-cleans on Drop.
fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1047-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
