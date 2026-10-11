use std::{collections::BTreeMap, fs, io, path::Path};

use calyx_assay::MIN_ASSAY_SAMPLES;
use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_editor_intelligence::{
        EngineEditorAuthoringRequest, EngineEditorBalanceReport, EngineEditorBalanceSample,
        EngineEditorDraft, EngineEditorDraftKind, EngineEditorIntelligenceError,
        EngineEditorKernelAnswer, EngineEditorKernelAnswerRequest, EngineEditorOracleForecast,
        EngineEditorOracleRequest, answer_editor_lodestar, draft_editor_content,
        forecast_editor_oracle, measure_editor_balance_bits, read_editor_intelligence,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{build_engine_calyx_semantic_index, store_engine_calyx_semantic_index},
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1123_calyx_editor_intelligence")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database.clone());
    let readbacks = seeded_content(&store)?;
    let index_id = "issue1123-fsv-semantic-index".to_string();
    let index = build_engine_calyx_semantic_index(&index_id, &readbacks)?;
    let semantic = store_engine_calyx_semantic_index(&database, &index)?;

    let draft_stored = draft_editor_content(
        &database,
        EngineEditorAuthoringRequest {
            action_id: "issue1123-draft".to_string(),
            semantic_index_id: index_id.clone(),
            prompt: "homing fireball that burns over time".to_string(),
            draft_kind: EngineEditorDraftKind::Ability,
            draft_id: "ability-homing-burn".to_string(),
        },
    )?;
    let draft = payload::<EngineEditorDraft>(&draft_stored.ledger.payload)?;
    let draft_readback = read_editor_intelligence(&database, "issue1123-draft")?;

    let balance_stored =
        measure_editor_balance_bits(&database, "issue1123-balance", &balance_samples())?;
    let balance = payload::<EngineEditorBalanceReport>(&balance_stored.ledger.payload)?;

    let lodestar_stored = answer_editor_lodestar(
        &database,
        EngineEditorKernelAnswerRequest {
            action_id: "issue1123-lodestar".to_string(),
            semantic_index_id: index_id.clone(),
            query: "fire burn homing ability".to_string(),
            max_kernel_hits: 3,
            min_grounding_hits: 1,
        },
    )?;
    let lodestar = payload::<EngineEditorKernelAnswer>(&lodestar_stored.ledger.payload)?;

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
    )?;
    let oracle = payload::<EngineEditorOracleForecast>(&oracle_stored.ledger.payload)?;

    let refusal_readbacks = refusal_readbacks(&database, &index_id)?;
    let edges = edge_readbacks(&database, &index_id)?;
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let draft_acceptance = draft.base_content_id == "ability-fireball"
        && !draft.provenance_refs.is_empty()
        && draft_readback.ledger.result_fingerprint == draft_stored.ledger.result_fingerprint;
    let balance_acceptance = balance.sample_count == MIN_ASSAY_SAMPLES
        && balance.top_stat == "damage_fp"
        && balance.total_bits + f32::EPSILON >= balance.target_entropy_bits
        && !balance.stats.is_empty();
    let lodestar_acceptance = !lodestar.refused
        && lodestar
            .kernel_content_ids
            .contains(&"ability/ability-fireball".to_string())
        && !lodestar.provenance_refs.is_empty();
    let oracle_acceptance = oracle.status == "predicted"
        && oracle.refusal_code.is_none()
        && oracle.reverse_causes == vec!["damage_fp".to_string()];
    let refusal_acceptance = refusal_readbacks
        .iter()
        .all(|item| item["matched"].as_bool().unwrap_or(false));
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));

    ensure(draft_acceptance, "editor draft acceptance failed")?;
    ensure(balance_acceptance, "editor balance acceptance failed")?;
    ensure(lodestar_acceptance, "editor Lodestar acceptance failed")?;
    ensure(oracle_acceptance, "editor Oracle acceptance failed")?;
    ensure(refusal_acceptance, "editor refusal acceptance failed")?;
    ensure(
        all_edges_matched,
        "editor intelligence edge acceptance failed",
    )?;

    let report = json!({
        "schema": "litd.engine.issue1123.calyx_editor_intelligence_fsv.v1",
        "schema_version": 1,
        "issue": 1123,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, Aster vault rows, semantic index, editor intelligence ledgers, refusal payloads, and hashes"
        },
        "happy_path": {
            "semantic_database": semantic.database,
            "semantic_index_id": index_id,
            "draft": {"ledger": draft_stored.ledger, "database": draft_readback.database},
            "balance": {"ledger": balance_stored.ledger, "payload": balance},
            "lodestar": {"ledger": lodestar_stored.ledger, "payload": lodestar},
            "oracle": {"ledger": oracle_stored.ledger, "payload": oracle},
            "refusals": refusal_readbacks,
            "vault_files": vault_files
        },
        "acceptance": {
            "draft_acceptance": draft_acceptance,
            "draft_base_content_id": draft.base_content_id,
            "draft_provenance_count": draft.provenance_refs.len(),
            "draft_fingerprint": draft_readback.ledger.result_fingerprint,
            "balance_acceptance": balance_acceptance,
            "balance_sample_count": balance.sample_count,
            "balance_total_bits": balance.total_bits,
            "balance_target_entropy_bits": balance.target_entropy_bits,
            "balance_top_stat": balance.top_stat,
            "lodestar_acceptance": lodestar_acceptance,
            "lodestar_kernel_hit_count": lodestar.kernel_hit_count,
            "lodestar_content_ids": lodestar.kernel_content_ids,
            "oracle_acceptance": oracle_acceptance,
            "oracle_status": oracle.status,
            "oracle_confidence": oracle.confidence,
            "oracle_reverse_causes": oracle.reverse_causes,
            "refusal_acceptance": refusal_acceptance,
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let readback_report = run.write_report(&report)?;
    println!(
        "ISSUE1123_CALYX_EDITOR_INTELLIGENCE_FSV report={} sha256={} manifest={} run_id={} draft_base={} balance_bits={:.3} top_stat={} lodestar_hits={} oracle_status={} refusals={} edges={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.latest_manifest_path.display(),
        run.run_id(),
        draft.base_content_id,
        balance.total_bits,
        balance.top_stat,
        lodestar.kernel_hit_count,
        oracle.status,
        refusal_readbacks.len(),
        edges.len()
    );
    Ok(())
}

fn seeded_content(
    store: &EngineCalyxContentStore,
) -> Result<
    Vec<litd_engine::calyx_content::EngineContentConstellationReadback>,
    Box<dyn std::error::Error>,
> {
    [
        content(
            EngineContentKind::Ability,
            "ability-fireball",
            "Fireball",
            json!({
                "ability_id": "fireball",
                "summary": "homing fire projectile that burns over time",
                "tags": ["fire", "homing", "burn"],
                "damage_fp": 80
            }),
        ),
        content(
            EngineContentKind::Unit,
            "unit-footman",
            "Footman",
            json!({
                "unit_id": "footman",
                "role": "frontline",
                "tags": ["melee", "armor"],
                "move_speed_fp": 30
            }),
        ),
        content(
            EngineContentKind::Lore,
            "lore-ash-ward",
            "Ash Ward",
            json!({
                "lore_id": "ash-ward",
                "summary": "ward stones remember old fire magic",
                "tags": ["ward", "fire", "history"]
            }),
        ),
    ]
    .iter()
    .map(|record| store.put_content(record).map_err(Into::into))
    .collect()
}

fn refusal_readbacks(
    database: &EngineCalyxDatabase,
    index_id: &str,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let refused_lodestar = answer_editor_lodestar(
        database,
        EngineEditorKernelAnswerRequest {
            action_id: "issue1123-ungrounded-lodestar".to_string(),
            semantic_index_id: index_id.to_string(),
            query: "zzznomatch qqqtoken".to_string(),
            max_kernel_hits: 2,
            min_grounding_hits: 1,
        },
    )?;
    let lodestar = payload::<EngineEditorKernelAnswer>(&refused_lodestar.ledger.payload)?;
    let insufficient_oracle = forecast_editor_oracle(
        database,
        EngineEditorOracleRequest {
            action_id: "issue1123-insufficient-oracle".to_string(),
            panel_bits: 0.1,
            anchor_entropy_bits: 1.0,
            requested_confidence: 0.8,
            desired_outcome: "win".to_string(),
            cause_refs: vec!["damage_fp".to_string()],
        },
    )?;
    let insufficient = payload::<EngineEditorOracleForecast>(&insufficient_oracle.ledger.payload)?;
    let ungrounded_oracle = forecast_editor_oracle(
        database,
        EngineEditorOracleRequest {
            action_id: "issue1123-ungrounded-oracle".to_string(),
            panel_bits: 1.0,
            anchor_entropy_bits: 1.0,
            requested_confidence: 0.8,
            desired_outcome: "win".to_string(),
            cause_refs: Vec::new(),
        },
    )?;
    let ungrounded = payload::<EngineEditorOracleForecast>(&ungrounded_oracle.ledger.payload)?;
    Ok(vec![
        json!({
            "label": "lodestar_ungrounded_refusal",
            "matched": lodestar.refused && lodestar.refusal_code.as_deref() == Some("CALYX_KERNEL_UNGROUNDED"),
            "database_sha256": refused_lodestar.database.sha256,
            "fingerprint": refused_lodestar.ledger.result_fingerprint,
            "kernel_hit_count": lodestar.kernel_hit_count
        }),
        json!({
            "label": "oracle_insufficient_refusal",
            "matched": insufficient.status == "refused_insufficient" && insufficient.refusal_code.as_deref() == Some("CALYX_ORACLE_INSUFFICIENT"),
            "database_sha256": insufficient_oracle.database.sha256,
            "fingerprint": insufficient_oracle.ledger.result_fingerprint,
            "deficit_bits": insufficient.deficit_bits
        }),
        json!({
            "label": "oracle_ungrounded_refusal",
            "matched": ungrounded.status == "refused_ungrounded" && ungrounded.refusal_code.as_deref() == Some("CALYX_KERNEL_UNGROUNDED"),
            "database_sha256": ungrounded_oracle.database.sha256,
            "fingerprint": ungrounded_oracle.ledger.result_fingerprint
        }),
    ])
}

fn edge_readbacks(
    database: &EngineCalyxDatabase,
    index_id: &str,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let too_few = balance_samples()
        .into_iter()
        .take(MIN_ASSAY_SAMPLES - 1)
        .collect::<Vec<_>>();
    Ok(vec![
        expect_invalid_input(
            "empty_prompt_rejected",
            draft_editor_content(
                database,
                EngineEditorAuthoringRequest {
                    action_id: "issue1123-empty-prompt".to_string(),
                    semantic_index_id: index_id.to_string(),
                    prompt: " ".to_string(),
                    draft_kind: EngineEditorDraftKind::Ability,
                    draft_id: "ability-empty".to_string(),
                },
            ),
            "prompt",
            "non-empty",
        ),
        expect_invalid_input(
            "too_few_balance_samples_rejected",
            measure_editor_balance_bits(database, "issue1123-too-few", &too_few),
            "samples",
            "at least 50",
        ),
        expect_invalid_input(
            "bad_kernel_bounds_rejected",
            answer_editor_lodestar(
                database,
                EngineEditorKernelAnswerRequest {
                    action_id: "issue1123-bad-kernel".to_string(),
                    semantic_index_id: index_id.to_string(),
                    query: "fire".to_string(),
                    max_kernel_hits: 0,
                    min_grounding_hits: 1,
                },
            ),
            "kernel_hits",
            "min_grounding_hits",
        ),
        expect_invalid_input(
            "empty_oracle_outcome_rejected",
            forecast_editor_oracle(
                database,
                EngineEditorOracleRequest {
                    action_id: "issue1123-empty-outcome".to_string(),
                    panel_bits: 1.0,
                    anchor_entropy_bits: 1.0,
                    requested_confidence: 0.8,
                    desired_outcome: " ".to_string(),
                    cause_refs: vec!["damage_fp".to_string()],
                },
            ),
            "desired_outcome",
            "outcome text",
        ),
        expect_invalid_input(
            "bad_oracle_confidence_rejected",
            forecast_editor_oracle(
                database,
                EngineEditorOracleRequest {
                    action_id: "issue1123-bad-confidence".to_string(),
                    panel_bits: 1.0,
                    anchor_entropy_bits: 1.0,
                    requested_confidence: 1.5,
                    desired_outcome: "win".to_string(),
                    cause_refs: vec!["damage_fp".to_string()],
                },
            ),
            "requested_confidence",
            "0.0..=1.0",
        ),
        missing_ledger_edge(database),
    ])
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
            author: "issue1123-fsv".to_string(),
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

fn balance_samples() -> Vec<EngineEditorBalanceSample> {
    (0..MIN_ASSAY_SAMPLES)
        .map(|idx| {
            let high = idx >= MIN_ASSAY_SAMPLES / 2;
            EngineEditorBalanceSample {
                sample_id: format!("sample-{idx:02}"),
                stats: BTreeMap::from([
                    ("damage_fp".to_string(), if high { 90.0 } else { 10.0 }),
                    (
                        "armor_fp".to_string(),
                        if idx % 2 == 0 { 35.0 } else { 40.0 },
                    ),
                ]),
                outcome_win: high,
                provenance_ref: format!("playtest://issue1123/{idx:02}"),
            }
        })
        .collect()
}

fn payload<T>(value: &Value) -> Result<T, serde_json::Error>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value.clone())
}

fn expect_invalid_input<T>(
    label: &str,
    result: Result<T, EngineEditorIntelligenceError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineEditorIntelligenceError::InvalidInput { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "field": field,
            "reason": reason,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
        }),
    }
}

fn missing_ledger_edge(database: &EngineCalyxDatabase) -> Value {
    match read_editor_intelligence(database, "missing-editor-ledger") {
        Err(EngineEditorIntelligenceError::Persistence(error)) => json!({
            "label": "missing_persisted_ledger_rejected",
            "matched": error.to_string().contains("missing-editor-ledger"),
            "error": error.to_string(),
        }),
        Err(error) => json!({
            "label": "missing_persisted_ledger_rejected",
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": "missing_persisted_ledger_rejected",
            "matched": false,
            "unexpected": "read succeeded",
        }),
    }
}

fn vault_file_readbacks(vault_dir: &Path) -> io::Result<Vec<Value>> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(vault_dir)? {
        collect_file_readbacks(&entry?.path(), &mut rows)?;
    }
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(rows)
}

fn collect_file_readbacks(path: &Path, rows: &mut Vec<Value>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_file_readbacks(&entry?.path(), rows)?;
        }
    } else {
        let bytes = fs::read(path)?;
        rows.push(json!({
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": litd_hash::sha256_hex(&bytes),
        }));
    }
    Ok(())
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
