use std::collections::BTreeMap;

use calyx_assay::MIN_ASSAY_SAMPLES;
use calyx_core::{Anchor, AnchorKind, AnchorValue};
use serde_json::{Value, json};

use crate::{
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
    calyx_editor_intelligence::{
        EngineEditorAuthoringRequest, EngineEditorBalanceReport, EngineEditorBalanceSample,
        EngineEditorDraft, EngineEditorDraftKind, EngineEditorIntelligenceError,
        EngineEditorKernelAnswer, EngineEditorKernelAnswerRequest, EngineEditorOracleForecast,
        EngineEditorOracleRequest, answer_editor_lodestar, draft_editor_content,
        forecast_editor_oracle, measure_editor_balance_bits, read_editor_intelligence,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{build_engine_calyx_semantic_index, store_engine_calyx_semantic_index},
};

#[test]
fn editor_intelligence_surfaces_authoring_balance_lodestar_and_oracle() {
    let (database, index_id, vault_dir) = seeded_world("happy");

    let draft_stored = draft_editor_content(
        &database,
        EngineEditorAuthoringRequest {
            action_id: "issue1123-draft".to_string(),
            semantic_index_id: index_id.clone(),
            prompt: "homing fireball that burns over time".to_string(),
            draft_kind: EngineEditorDraftKind::Ability,
            draft_id: "ability-homing-burn".to_string(),
        },
    )
    .unwrap();
    let draft = payload::<EngineEditorDraft>(&draft_stored.ledger.payload);
    let draft_readback = read_editor_intelligence(&database, "issue1123-draft").unwrap();

    let balance_stored =
        measure_editor_balance_bits(&database, "issue1123-balance", &balance_samples()).unwrap();
    let balance = payload::<EngineEditorBalanceReport>(&balance_stored.ledger.payload);

    let lodestar_stored = answer_editor_lodestar(
        &database,
        EngineEditorKernelAnswerRequest {
            action_id: "issue1123-lodestar".to_string(),
            semantic_index_id: index_id,
            query: "fire burn homing ability".to_string(),
            max_kernel_hits: 3,
            min_grounding_hits: 1,
        },
    )
    .unwrap();
    let lodestar = payload::<EngineEditorKernelAnswer>(&lodestar_stored.ledger.payload);

    let oracle_stored = forecast_editor_oracle(
        &database,
        EngineEditorOracleRequest {
            action_id: "issue1123-oracle".to_string(),
            panel_bits: balance.total_bits,
            anchor_entropy_bits: balance.target_entropy_bits,
            requested_confidence: 0.8,
            desired_outcome: "high damage wins".to_string(),
            cause_refs: vec![balance.top_stat.clone()],
        },
    )
    .unwrap();
    let oracle = payload::<EngineEditorOracleForecast>(&oracle_stored.ledger.payload);

    assert_eq!(draft.base_content_id, "ability-fireball");
    assert!(!draft.provenance_refs.is_empty());
    assert_eq!(
        draft_readback.ledger.result_fingerprint,
        draft_stored.ledger.result_fingerprint
    );
    assert_eq!(balance.sample_count, MIN_ASSAY_SAMPLES);
    assert_eq!(balance.top_stat, "damage_fp");
    assert!(balance.total_bits >= balance.target_entropy_bits);
    assert!(!lodestar.refused);
    assert!(
        lodestar
            .kernel_content_ids
            .contains(&"ability/ability-fireball".to_string())
    );
    assert_eq!(oracle.status, "predicted");
    assert_eq!(oracle.reverse_causes, vec!["damage_fp".to_string()]);

    println!(
        "issue1123_editor_intelligence draft_base={} balance_bits={:.3} top_stat={} lodestar_hits={} oracle_status={} vault={}",
        draft.base_content_id,
        balance.total_bits,
        balance.top_stat,
        lodestar.kernel_hit_count,
        oracle.status,
        vault_dir.display()
    );
}

#[test]
fn editor_intelligence_fail_loud_and_refuses_ungrounded() {
    let (database, index_id, _vault_dir) = seeded_world("edge");

    match draft_editor_content(
        &database,
        EngineEditorAuthoringRequest {
            action_id: "issue1123-empty".to_string(),
            semantic_index_id: index_id.clone(),
            prompt: " ".to_string(),
            draft_kind: EngineEditorDraftKind::Ability,
            draft_id: "ability-empty".to_string(),
        },
    ) {
        Err(EngineEditorIntelligenceError::InvalidInput { field, reason }) => {
            assert_eq!(field, "prompt");
            assert!(reason.contains("non-empty"));
        }
        other => panic!("expected empty prompt rejection, got {other:?}"),
    }

    let too_few = balance_samples()
        .into_iter()
        .take(MIN_ASSAY_SAMPLES - 1)
        .collect::<Vec<_>>();
    match measure_editor_balance_bits(&database, "issue1123-too-few", &too_few) {
        Err(EngineEditorIntelligenceError::InvalidInput { field, reason }) => {
            assert_eq!(field, "samples");
            assert!(reason.contains("at least 50"));
        }
        other => panic!("expected sample-count rejection, got {other:?}"),
    }

    let refused_lodestar = answer_editor_lodestar(
        &database,
        EngineEditorKernelAnswerRequest {
            action_id: "issue1123-ungrounded-lodestar".to_string(),
            semantic_index_id: index_id,
            query: "zzznomatch qqqtoken".to_string(),
            max_kernel_hits: 2,
            min_grounding_hits: 1,
        },
    )
    .unwrap();
    let lodestar = payload::<EngineEditorKernelAnswer>(&refused_lodestar.ledger.payload);
    assert!(lodestar.refused);
    assert_eq!(
        lodestar.refusal_code.as_deref(),
        Some("CALYX_KERNEL_UNGROUNDED")
    );

    let insufficient_oracle = forecast_editor_oracle(
        &database,
        EngineEditorOracleRequest {
            action_id: "issue1123-insufficient-oracle".to_string(),
            panel_bits: 0.1,
            anchor_entropy_bits: 1.0,
            requested_confidence: 0.8,
            desired_outcome: "win".to_string(),
            cause_refs: vec!["damage_fp".to_string()],
        },
    )
    .unwrap();
    let oracle = payload::<EngineEditorOracleForecast>(&insufficient_oracle.ledger.payload);
    assert_eq!(oracle.status, "refused_insufficient");
    assert_eq!(
        oracle.refusal_code.as_deref(),
        Some("CALYX_ORACLE_INSUFFICIENT")
    );
}

fn seeded_world(label: &str) -> (EngineCalyxDatabase, String, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    let store = crate::calyx_content_tests::default_fixture_content_store(database.clone(), label);
    let readbacks = vec![
        store
            .put_content(&content(
                EngineContentKind::Ability,
                "ability-fireball",
                "Fireball",
                json!({
                    "ability_id": "fireball",
                    "summary": "homing fire projectile that burns over time",
                    "tags": ["fire", "homing", "burn"],
                    "damage_fp": 80
                }),
            ))
            .unwrap(),
        store
            .put_content(&content(
                EngineContentKind::Unit,
                "unit-footman",
                "Footman",
                json!({
                    "unit_id": "footman",
                    "role": "frontline",
                    "tags": ["melee", "armor"],
                    "move_speed_fp": 30
                }),
            ))
            .unwrap(),
        store
            .put_content(&content(
                EngineContentKind::Lore,
                "lore-ash-ward",
                "Ash Ward",
                json!({
                    "lore_id": "ash-ward",
                    "summary": "ward stones remember old fire magic",
                    "tags": ["ward", "fire", "history"]
                }),
            ))
            .unwrap(),
    ];
    let index_id = format!("issue1123-{label}-semantic-index");
    let index = build_engine_calyx_semantic_index(&index_id, &readbacks).unwrap();
    store_engine_calyx_semantic_index(&database, &index).unwrap();
    (database, index_id, vault_dir)
}

fn balance_samples() -> Vec<EngineEditorBalanceSample> {
    (0..MIN_ASSAY_SAMPLES)
        .map(|idx| {
            let high = idx >= MIN_ASSAY_SAMPLES / 2;
            let damage = if high { 90.0 } else { 10.0 };
            let armor = if idx % 2 == 0 { 35.0 } else { 40.0 };
            EngineEditorBalanceSample {
                sample_id: format!("sample-{idx:02}"),
                stats: BTreeMap::from([
                    ("damage_fp".to_string(), damage),
                    ("armor_fp".to_string(), armor),
                ]),
                outcome_win: high,
                provenance_ref: format!("playtest://issue1123/{idx:02}"),
            }
        })
        .collect()
}

fn content(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue1123/{id}"),
            author: "issue1123-test".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1123:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1123_editor_intelligence".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn payload<T>(value: &Value) -> T
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value.clone()).unwrap()
}

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1123-{label}"))
}
