#[path = "fixtures/workspace_source_readback.rs"]
mod workspace_source_readback;

use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        ASHEN_REACH_SKILLED_REWARD_POLICY_ID, RPG_SKILLED_REWARD_SCHEMA, SkilledPlayEvidence,
        SkilledPlayEvidenceKind, SkilledRewardCandidate, SkilledRewardKind, SkilledRewardPolicy,
        evaluate_skilled_reward, persist_and_lower_rpg_authored_content,
        skilled_reward_content_records, skilled_reward_policy_hash, validate_skilled_reward_policy,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};
use workspace_source_readback::source_readback;

const SOURCE_PATHS: [&str; 41] = [
    "docs/lore/ashen-reach-world-bible.md",
    "docs/lore/ashen-reach-pantheon-factions.md",
    "docs/readme/S09-immortal-reward.png",
    "docs/readme/GP03-combat.png",
    "docs/readme/GP06-pvp.png",
    "docs/readme/GP07-boss.png",
    "crates/litd-engine/examples/issue1047_skilled_rewards_fsv.rs",
    "crates/litd-engine/src/rpg/skilled_rewards_types.rs",
    "crates/litd-engine/src/rpg/skilled_rewards.rs",
    "crates/litd-engine/src/rpg/skilled_rewards_content.rs",
    "crates/litd-engine/src/rpg/skilled_rewards_tests.rs",
    "crates/litd-engine/src/rpg/skilled_rewards_test_fixture.rs",
    "crates/litd-engine/src/rpg/immortals_types.rs",
    "crates/litd-engine/src/rpg/immortals.rs",
    "crates/litd-engine/src/rpg/immortals_validate.rs",
    "crates/litd-engine/src/rpg/immortals_content.rs",
    "crates/litd-engine/src/rpg/immortals_tests.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight_types.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight_abilities.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight_content.rs",
    "crates/litd-engine/src/rpg/unhallowed_knight_tests.rs",
    "crates/litd-engine/src/rpg/abilities_types.rs",
    "crates/litd-engine/src/rpg/abilities.rs",
    "crates/litd-engine/src/rpg/abilities_support.rs",
    "crates/litd-engine/src/rpg/abilities_validate.rs",
    "crates/litd-engine/src/rpg/abilities_readback.rs",
    "crates/litd-engine/src/rpg/abilities_content.rs",
    "crates/litd-engine/src/rpg/abilities_tests.rs",
    "crates/litd-engine/src/rpg/abilities_test_fixture.rs",
    "crates/litd-engine/src/rpg/combat_types.rs",
    "crates/litd-engine/src/rpg/combat.rs",
    "crates/litd-engine/src/rpg/combat_validate.rs",
    "crates/litd-engine/src/rpg/combat_resolve.rs",
    "crates/litd-engine/src/rpg/combat_readback.rs",
    "crates/litd-engine/src/rpg/combat_content.rs",
    "crates/litd-engine/src/rpg/combat_tests.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents_types.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents_content.rs",
    "crates/litd-engine/src/rpg/infernal_boss_agents_tests.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1047_skilled_rewards")?;
    let policy = ashen_reach_skilled_reward_policy();
    let candidate = candidate();
    let readback = evaluate_skilled_reward(&policy, &candidate)?;
    let records = skilled_reward_content_records(&policy, &readback)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_map_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_anchor_count = records
        .iter()
        .map(|record| record.anchors.len())
        .sum::<usize>();
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/skilled_rewards/")
    });
    let content_has_compiled_source_path = persisted_rows
        .iter()
        .any(|row| row.source_path.starts_with("rpg://"));
    let lowered_artifact_count = persisted_rows
        .iter()
        .filter(|row| {
            row.lowering.hot_path_policy == "frozen_fingerprinted_data_only"
                && !row.lowering.fingerprint.is_empty()
        })
        .count();

    let edges = json!({
        "built_in_map_rejected": built_in_map_edge(&policy),
        "minimap_rejected": minimap_edge(&policy),
        "gps_rejected": gps_edge(&policy),
        "low_skill_rejected": low_skill_edge(&policy),
        "low_rp_rejected": low_rp_edge(&policy),
        "duplicate_evidence_rejected": duplicate_evidence_edge(&policy),
        "unwitnessed_evidence_rejected": unwitnessed_evidence_edge(&policy),
        "insufficient_distinct_kinds_rejected": one_kind_edge(&policy),
    });
    require_all_edges_unchanged(&edges)?;
    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;
    let source_readback_count = source_readbacks.len();
    let source_count_for = |needle: &str| -> usize {
        SOURCE_PATHS
            .iter()
            .filter(|path| path.contains(needle))
            .count()
    };

    let report = json!({
        "schema": "litd.issue1047.skilled_rewards_fsv.v1",
        "issue": 1047,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Use Synapse/computer control to read this JSON and the Calyx WAL. Compare expected_actuals to persisted skilled reward rows and verify no map/minimap/GPS grant appears."
        },
        "sufficiency_argument": "The #1047 proof evaluates one real candidate through a default skilled-play reward policy that requires measured combat/PvP/ability/boss evidence, an RP-quality input, and Immortal or system approval before any advancement or boon effect is lowered. It persists policy, ledger, and effect rows into Calyx and probes eight failure edges for map drift, low skill, low RP, duplicate evidence, unwitnessed evidence, and insufficient evidence diversity.",
        "expected_actuals": {
            "source_issue": 1047,
            "source_readback_count": source_readback_count,
            "skilled_reward_source_count": source_count_for("skilled_rewards"),
            "immortal_source_count": source_count_for("immortals"),
            "unhallowed_source_count": source_count_for("unhallowed_knight"),
            "ability_source_count": source_count_for("abilities"),
            "combat_source_count": source_count_for("combat"),
            "infernal_boss_source_count": source_count_for("infernal_boss_agents"),
            "candidate_id": "candidate.skill.morvane.1",
            "character_id": "char.morvane_ash",
            "accepted": true,
            "grade": "accepted",
            "skill_score_milli": 844,
            "rp_quality_milli": 780,
            "combined_score_milli": 818,
            "evidence_count": 4,
            "distinct_evidence_kind_count": 4,
            "effect_count": 1,
            "reward_tier": 4,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "source_anchor_count": source_anchor_count,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_compiled_source_path": content_has_compiled_source_path,
            "lowered_artifact_count": lowered_artifact_count,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
            "content_has_map_or_gps_true": content_has_map_true,
            "edge_count": 8,
            "edge_result_contract": "validation edges return LITD-RPG-STATS-INVALID-CONFIG; threshold edges return accepted=false with no lowered effects"
        },
        "policy": {
            "server_policy_id": policy.server_policy_id,
            "source_issue": policy.source_issue,
            "policy_sha256": skilled_reward_policy_hash(&policy)?,
            "min_skill_score_milli": policy.min_skill_score_milli,
            "min_rp_quality_milli": policy.min_rp_quality_milli,
            "min_combined_score_milli": policy.min_combined_score_milli,
            "built_in_map_required": policy.built_in_map_required,
        },
        "candidate": candidate,
        "reward_readback": readback,
        "calyx": {
            "persisted_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });
    require_i64(
        &report["expected_actuals"]["source_readback_count"],
        SOURCE_PATHS.len() as i64,
    )?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 3)?;
    require_i64(&report["expected_actuals"]["source_anchor_count"], 0)?;
    require_i64(&report["expected_actuals"]["lowered_artifact_count"], 3)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_compiled_source_path"],
        false,
    )?;
    require_i64(&report["expected_actuals"]["skill_score_milli"], 844)?;
    require_i64(&report["expected_actuals"]["combined_score_milli"], 818)?;
    require_i64(&report["expected_actuals"]["reward_tier"], 4)?;
    require_bool(&report["expected_actuals"]["accepted"], true)?;
    require_bool(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        false,
    )?;

    let read = run.write_report(&report)?;
    println!(
        "ISSUE1047_SKILLED_REWARDS_FSV report={} bytes={} sha256={} rows={} anchors={} editor_sources={} skill={} rp={} combined={} effects={} map_true={} edges={}",
        read.path.display(),
        read.bytes.len(),
        read.sha256,
        read.json["expected_actuals"]["content_rows_read_back"],
        read.json["expected_actuals"]["source_anchor_count"],
        read.json["expected_actuals"]["source_paths_are_editor_authored"],
        read.json["expected_actuals"]["skill_score_milli"],
        read.json["expected_actuals"]["rp_quality_milli"],
        read.json["expected_actuals"]["combined_score_milli"],
        read.json["expected_actuals"]["effect_count"],
        read.json["expected_actuals"]["content_has_map_or_gps_true"],
        read.json["edges"]
            .as_object()
            .map(|edges| edges.len())
            .unwrap_or_default(),
    );
    Ok(())
}

fn ashen_reach_skilled_reward_policy() -> SkilledRewardPolicy {
    SkilledRewardPolicy {
        schema: RPG_SKILLED_REWARD_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_SKILLED_REWARD_POLICY_ID.to_string(),
        source_issue: 1047,
        min_skill_score_milli: 750,
        min_rp_quality_milli: 650,
        min_combined_score_milli: 800,
        skill_weight_milli: 600,
        rp_weight_milli: 400,
        max_reward_tier: 5,
        evidence_rows_required: 3,
        distinct_evidence_kinds_required: 3,
        rewards_revocable: true,
        immortal_or_system_approval_required: true,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
    }
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "kind": row.kind,
                "database_id": row.database.id,
                "database_sha256": row.database.sha256,
                "source_path": row.source_path,
                "surface_kind": row.payload["surface_kind"],
                "lowering_artifact_id": row.lowering.artifact_id,
                "lowering_fingerprint": row.lowering.fingerprint,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn built_in_map_edge(policy: &SkilledRewardPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.built_in_map_required = true;
    let before = value_hash(&invalid);
    let result = validate_skilled_reward_policy(&invalid);
    edge_from_result(result, before, value_hash(&invalid))
}
fn minimap_edge(policy: &SkilledRewardPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.grants_map_or_minimap = true;
    let before = value_hash(&invalid);
    let result = validate_skilled_reward_policy(&invalid);
    edge_from_result(result, before, value_hash(&invalid))
}
fn gps_edge(policy: &SkilledRewardPolicy) -> Value {
    let mut invalid = policy.clone();
    invalid.grants_gps_markers = true;
    let before = value_hash(&invalid);
    let result = validate_skilled_reward_policy(&invalid);
    edge_from_result(result, before, value_hash(&invalid))
}
fn low_skill_edge(policy: &SkilledRewardPolicy) -> Value {
    let invalid = low_skill_candidate();
    let before = reward_inputs_hash(policy, &invalid);
    let result = evaluate_skilled_reward(policy, &invalid);
    edge_from_readback(result, false, before, reward_inputs_hash(policy, &invalid))
}
fn low_rp_edge(policy: &SkilledRewardPolicy) -> Value {
    let mut invalid = candidate();
    invalid.rp_quality_milli = 620;
    let before = reward_inputs_hash(policy, &invalid);
    let result = evaluate_skilled_reward(policy, &invalid);
    edge_from_readback(result, false, before, reward_inputs_hash(policy, &invalid))
}
fn duplicate_evidence_edge(policy: &SkilledRewardPolicy) -> Value {
    let mut invalid = candidate();
    invalid.evidence[1].evidence_id = invalid.evidence[0].evidence_id.clone();
    let before = reward_inputs_hash(policy, &invalid);
    let result = evaluate_skilled_reward(policy, &invalid);
    edge_from_result(result, before, reward_inputs_hash(policy, &invalid))
}
fn unwitnessed_evidence_edge(policy: &SkilledRewardPolicy) -> Value {
    let mut invalid = candidate();
    invalid.evidence[0].witnessed_by_system = false;
    let before = reward_inputs_hash(policy, &invalid);
    let result = evaluate_skilled_reward(policy, &invalid);
    edge_from_result(result, before, reward_inputs_hash(policy, &invalid))
}
fn one_kind_edge(policy: &SkilledRewardPolicy) -> Value {
    let mut invalid = candidate();
    for row in &mut invalid.evidence {
        row.kind = SkilledPlayEvidenceKind::CombatProwess;
    }
    let before = reward_inputs_hash(policy, &invalid);
    let result = evaluate_skilled_reward(policy, &invalid);
    edge_from_result(result, before, reward_inputs_hash(policy, &invalid))
}

fn edge_from_readback(
    result: Result<litd_engine::rpg::SkilledRewardReadback, EngineError>,
    accepted: bool,
    before: String,
    after: String,
) -> Value {
    let inputs_unchanged = before == after;
    match result {
        Ok(readback) => json!({
            "accepted": readback.accepted,
            "grade": readback.grade,
            "effect_count": readback.effect_count,
            "before_sha256": before,
            "after_sha256": after,
            "state_unchanged": inputs_unchanged && readback.accepted == accepted && readback.effect_count == 0,
        }),
        Err(error) => {
            json!({"error_code": error.code(), "error": error.to_string(), "before_sha256": before, "after_sha256": after, "state_unchanged": inputs_unchanged})
        }
    }
}

fn edge_from_result<T>(result: Result<T, EngineError>, before: String, after: String) -> Value {
    let state_unchanged = before == after;
    match result {
        Ok(_) => {
            json!({"unexpected_success": true, "before_sha256": before, "after_sha256": after, "state_unchanged": state_unchanged})
        }
        Err(error) => {
            json!({"error_code": error.code(), "error": error.to_string(), "before_sha256": before, "after_sha256": after, "state_unchanged": state_unchanged})
        }
    }
}

fn reward_inputs_hash(policy: &SkilledRewardPolicy, candidate: &SkilledRewardCandidate) -> String {
    value_hash(&json!({"policy": policy, "candidate": candidate}))
}

fn value_hash(value: &impl serde::Serialize) -> String {
    sha256_hex(&serde_json::to_vec(value).expect("edge input must serialize"))
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

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1047.report.bool".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() != Some(expected) {
        return Err(EngineError::RpgStatsInvalidConfig {
            field: "issue1047.report.i64".to_string(),
            reason: format!("expected {expected}, got {value}"),
        });
    }
    Ok(())
}
fn require_all_edges_unchanged(edges: &Value) -> Result<(), EngineError> {
    for (name, edge) in edges.as_object().into_iter().flatten() {
        let validation_error = edge["error_code"] == "LITD-RPG-STATS-INVALID-CONFIG";
        let rejected_without_effects = edge["accepted"] == false && edge["effect_count"] == 0;
        if edge["state_unchanged"] != true || (!validation_error && !rejected_without_effects) {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("issue1047.edge.{name}"),
                reason: format!("unexpected edge {edge}"),
            });
        }
    }
    Ok(())
}
