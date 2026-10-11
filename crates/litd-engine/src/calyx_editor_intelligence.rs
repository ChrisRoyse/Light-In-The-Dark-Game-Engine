//! Calyx-backed editor intelligence surfaces.

use std::collections::BTreeSet;

use calyx_assay::{
    MIN_ASSAY_SAMPLES, TrustTag, bits_report, entropy_bits, logistic_probe_mi,
    per_sensor_attribution,
};
use calyx_core::{CalyxError, SlotId};
use calyx_oracle::oracle_formula_predict;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

pub use crate::calyx_editor_intelligence_types::*;
use crate::{
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxRecord, EngineCalyxRecordKind,
    },
    calyx_semantic::{
        EngineCalyxSemanticHit, query_engine_calyx_semantic_index, read_engine_calyx_semantic_index,
    },
};

pub fn draft_editor_content(
    database: &EngineCalyxDatabase,
    request: EngineEditorAuthoringRequest,
) -> Result<EngineEditorIntelligenceStoredReadback, EngineEditorIntelligenceError> {
    validate_action_id(&request.action_id)?;
    validate_action_id(&request.draft_id)?;
    if request.prompt.trim().is_empty() {
        return invalid(
            "prompt",
            "natural-language authoring prompt must be non-empty",
        );
    }
    let index = read_engine_calyx_semantic_index(database, &request.semantic_index_id)?.index;
    let semantic_query =
        query_engine_calyx_semantic_index(&index, &request.prompt, index.records.len())?;
    let hit = semantic_query
        .hits
        .iter()
        .find(|hit| hit.kind == request.draft_kind.content_kind())
        .ok_or_else(|| EngineEditorIntelligenceError::InvalidInput {
            field: "prompt".to_string(),
            reason: format!(
                "no grounded {} content matched prompt",
                request.draft_kind.as_str()
            ),
        })?;
    let draft = draft_from_hit(&request, hit);
    store_editor_intelligence(
        database,
        &request.action_id,
        "natural_language_authoring",
        serde_json::to_value(draft).map_err(codec_failed)?,
    )
}

pub fn measure_editor_balance_bits(
    database: &EngineCalyxDatabase,
    action_id: &str,
    samples: &[EngineEditorBalanceSample],
) -> Result<EngineEditorIntelligenceStoredReadback, EngineEditorIntelligenceError> {
    validate_action_id(action_id)?;
    let stat_names = validate_balance_samples(samples)?;
    let labels = samples
        .iter()
        .map(|sample| sample.outcome_win)
        .collect::<Vec<_>>();
    let mut slot_bits = Vec::with_capacity(stat_names.len());
    for (slot, stat_name) in stat_names.iter().enumerate() {
        let stat_samples = samples
            .iter()
            .map(|sample| vec![sample.stats[stat_name]])
            .collect::<Vec<_>>();
        let report = logistic_probe_mi(&stat_samples, &labels).map_err(assay_failed)?;
        slot_bits.push((SlotId::new(slot as u16), report.estimate.bits));
    }
    let attributions = per_sensor_attribution(&slot_bits, 0.05);
    let bits = bits_report(attributions.clone(), TrustTag::Trusted);
    let stats = attributions
        .iter()
        .zip(stat_names.iter())
        .map(|(slot, stat_name)| {
            balance_stat(
                samples,
                stat_name,
                slot.slot,
                slot.marginal_bits,
                slot.sole_carrier,
            )
        })
        .collect::<Vec<_>>();
    let target_entropy_bits = entropy_bits(&labels);
    let top_stat = stats
        .iter()
        .max_by(|left, right| left.marginal_bits.total_cmp(&right.marginal_bits))
        .map(|stat| stat.stat_name.clone())
        .unwrap_or_default();
    let report = EngineEditorBalanceReport {
        sample_count: samples.len(),
        target_entropy_bits,
        total_bits: bits.total_bits,
        sufficient: bits.total_bits + f32::EPSILON >= target_entropy_bits,
        top_stat,
        stats,
    };
    store_editor_intelligence(
        database,
        action_id,
        "balance_bits",
        serde_json::to_value(report).map_err(codec_failed)?,
    )
}

pub fn answer_editor_lodestar(
    database: &EngineCalyxDatabase,
    request: EngineEditorKernelAnswerRequest,
) -> Result<EngineEditorIntelligenceStoredReadback, EngineEditorIntelligenceError> {
    validate_action_id(&request.action_id)?;
    if request.max_kernel_hits == 0
        || request.min_grounding_hits == 0
        || request.min_grounding_hits > request.max_kernel_hits
    {
        return invalid(
            "kernel_hits",
            "Lodestar answer requires 0 < min_grounding_hits <= max_kernel_hits",
        );
    }
    let index = read_engine_calyx_semantic_index(database, &request.semantic_index_id)?.index;
    let semantic_query =
        query_engine_calyx_semantic_index(&index, &request.query, request.max_kernel_hits)?;
    let answer = if semantic_query.hits.len() < request.min_grounding_hits {
        EngineEditorKernelAnswer {
            refused: true,
            refusal_code: Some("CALYX_KERNEL_UNGROUNDED".to_string()),
            answer: String::new(),
            kernel_hit_count: semantic_query.hits.len(),
            kernel_content_ids: hit_ids(&semantic_query.hits),
            provenance_refs: Vec::new(),
        }
    } else {
        EngineEditorKernelAnswer {
            refused: false,
            refusal_code: None,
            answer: format!(
                "grounded editor answer for '{}' via {} Calyx kernel hits",
                request.query.trim(),
                semantic_query.hits.len()
            ),
            kernel_hit_count: semantic_query.hits.len(),
            kernel_content_ids: hit_ids(&semantic_query.hits),
            provenance_refs: provenance_refs(&semantic_query.hits)?,
        }
    };
    store_editor_intelligence(
        database,
        &request.action_id,
        "lodestar_kernel_answer",
        serde_json::to_value(answer).map_err(codec_failed)?,
    )
}

pub fn forecast_editor_oracle(
    database: &EngineCalyxDatabase,
    request: EngineEditorOracleRequest,
) -> Result<EngineEditorIntelligenceStoredReadback, EngineEditorIntelligenceError> {
    validate_action_id(&request.action_id)?;
    if request.desired_outcome.trim().is_empty() {
        return invalid(
            "desired_outcome",
            "Oracle forecast requires desired outcome text",
        );
    }
    validate_non_negative(request.panel_bits, "panel_bits")?;
    validate_non_negative(request.anchor_entropy_bits, "anchor_entropy_bits")?;
    validate_unit_interval(request.requested_confidence, "requested_confidence")?;
    let forecast = if request.cause_refs.is_empty() {
        EngineEditorOracleForecast {
            status: "refused_ungrounded".to_string(),
            confidence: 0.0,
            deficit_bits: request.anchor_entropy_bits,
            desired_outcome: request.desired_outcome,
            reverse_causes: Vec::new(),
            refusal_code: Some("CALYX_KERNEL_UNGROUNDED".to_string()),
        }
    } else {
        match oracle_formula_predict(
            request.panel_bits,
            request.anchor_entropy_bits,
            request.requested_confidence,
        ) {
            Ok(prediction) => EngineEditorOracleForecast {
                status: "predicted".to_string(),
                confidence: prediction.confidence,
                deficit_bits: prediction.deficit_bits,
                desired_outcome: request.desired_outcome,
                reverse_causes: normalize_strings(request.cause_refs)?,
                refusal_code: None,
            },
            Err(error) => EngineEditorOracleForecast {
                status: "refused_insufficient".to_string(),
                confidence: 0.0,
                deficit_bits: (request.anchor_entropy_bits - request.panel_bits).max(0.0),
                desired_outcome: request.desired_outcome,
                reverse_causes: Vec::new(),
                refusal_code: Some(error.code.to_string()),
            },
        }
    };
    store_editor_intelligence(
        database,
        &request.action_id,
        "oracle_forecast",
        serde_json::to_value(forecast).map_err(codec_failed)?,
    )
}

pub fn read_editor_intelligence(
    database: &EngineCalyxDatabase,
    action_id: &str,
) -> Result<EngineEditorIntelligenceStoredReadback, EngineEditorIntelligenceError> {
    validate_action_id(action_id)?;
    parse_stored_editor_intelligence(
        database.get_record(EngineCalyxRecordKind::EditorIntelligenceLedger, action_id)?,
    )
}

fn draft_from_hit(
    request: &EngineEditorAuthoringRequest,
    hit: &EngineCalyxSemanticHit,
) -> EngineEditorDraft {
    let terms = prompt_terms(&request.prompt);
    EngineEditorDraft {
        draft_id: request.draft_id.clone(),
        draft_kind: request.draft_kind,
        base_content_id: hit.id.clone(),
        base_display_name: hit.display_name.clone(),
        payload: json!({
            "draft_id": request.draft_id,
            "draft_kind": request.draft_kind.as_str(),
            "base_content_id": hit.id,
            "display_name": format!("Draft {} {}", request.draft_kind.as_str(), request.draft_id),
            "summary": request.prompt.trim(),
            "grounded_delta_terms": terms,
        }),
        provenance_refs: hit
            .provenance_refs
            .iter()
            .map(|reference| format!("{}#{}", reference.source_path, reference.input_sha256))
            .collect(),
        warnings: Vec::new(),
    }
}

fn validate_balance_samples(
    samples: &[EngineEditorBalanceSample],
) -> Result<Vec<String>, EngineEditorIntelligenceError> {
    if samples.len() < MIN_ASSAY_SAMPLES {
        return invalid(
            "samples",
            format!("Assay balance requires at least {MIN_ASSAY_SAMPLES} grounded samples"),
        );
    }
    let first = samples
        .first()
        .ok_or_else(|| EngineEditorIntelligenceError::InvalidInput {
            field: "samples".to_string(),
            reason: "samples must be non-empty".to_string(),
        })?;
    let stat_names = first.stats.keys().cloned().collect::<Vec<_>>();
    if stat_names.is_empty() {
        return invalid("stats", "balance samples require at least one stat");
    }
    let expected = stat_names.iter().cloned().collect::<BTreeSet<_>>();
    for sample in samples {
        validate_action_id(&sample.sample_id)?;
        if sample.provenance_ref.trim().is_empty() {
            return invalid("provenance_ref", "balance samples require provenance refs");
        }
        if sample.stats.keys().cloned().collect::<BTreeSet<_>>() != expected {
            return invalid(
                "stats",
                "all balance samples must expose the same stat keys",
            );
        }
        if sample.stats.values().any(|value| !value.is_finite()) {
            return invalid("stats", "balance stat values must be finite");
        }
    }
    Ok(stat_names)
}

fn balance_stat(
    samples: &[EngineEditorBalanceSample],
    stat_name: &str,
    slot: SlotId,
    marginal_bits: f32,
    sole_carrier: bool,
) -> EngineEditorBalanceStat {
    let values = samples
        .iter()
        .map(|sample| sample.stats[stat_name])
        .collect::<Vec<_>>();
    EngineEditorBalanceStat {
        stat_name: stat_name.to_string(),
        slot_id: slot.get(),
        marginal_bits,
        sole_carrier,
        min_value: values.iter().copied().fold(f32::INFINITY, f32::min),
        max_value: values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
    }
}

fn store_editor_intelligence(
    database: &EngineCalyxDatabase,
    action_id: &str,
    operation: &str,
    payload: Value,
) -> Result<EngineEditorIntelligenceStoredReadback, EngineEditorIntelligenceError> {
    let mut ledger = EngineEditorIntelligenceLedger {
        schema: ENGINE_CALYX_EDITOR_INTELLIGENCE_SCHEMA.to_string(),
        action_id: action_id.to_string(),
        operation: operation.to_string(),
        payload,
        result_fingerprint: String::new(),
    };
    ledger.result_fingerprint = ledger_fingerprint(&ledger)?;
    validate_ledger(&ledger)?;
    let stored = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::EditorIntelligenceLedger,
        ledger.action_id.clone(),
        serde_json::to_value(&ledger).map_err(codec_failed)?,
        json!({
            "source_issue": 1123,
            "operation": ledger.operation,
            "result_fingerprint": ledger.result_fingerprint
        }),
    ))?;
    parse_stored_editor_intelligence(stored)
}

fn parse_stored_editor_intelligence(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineEditorIntelligenceStoredReadback, EngineEditorIntelligenceError> {
    let ledger = serde_json::from_value::<EngineEditorIntelligenceLedger>(database.payload.clone())
        .map_err(codec_failed)?;
    validate_ledger(&ledger)?;
    Ok(EngineEditorIntelligenceStoredReadback {
        schema: ENGINE_CALYX_EDITOR_INTELLIGENCE_READBACK_SCHEMA.to_string(),
        database,
        ledger,
    })
}

fn validate_ledger(
    ledger: &EngineEditorIntelligenceLedger,
) -> Result<(), EngineEditorIntelligenceError> {
    if ledger.schema != ENGINE_CALYX_EDITOR_INTELLIGENCE_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_CALYX_EDITOR_INTELLIGENCE_SCHEMA}"),
        );
    }
    validate_action_id(&ledger.action_id)?;
    if ledger.operation.trim().is_empty() || ledger.payload.is_null() {
        return invalid("ledger", "operation and non-null payload are required");
    }
    if ledger.result_fingerprint != ledger_fingerprint(ledger)? {
        return invalid(
            "result_fingerprint",
            "editor intelligence fingerprint mismatch",
        );
    }
    Ok(())
}

fn ledger_fingerprint(
    ledger: &EngineEditorIntelligenceLedger,
) -> Result<String, EngineEditorIntelligenceError> {
    serde_json::to_vec(&json!({
        "schema": ledger.schema,
        "action_id": ledger.action_id,
        "operation": ledger.operation,
        "payload": ledger.payload,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(codec_failed)
}

fn hit_ids(hits: &[EngineCalyxSemanticHit]) -> Vec<String> {
    hits.iter().map(|hit| hit.key.clone()).collect()
}

fn provenance_refs(
    hits: &[EngineCalyxSemanticHit],
) -> Result<Vec<String>, EngineEditorIntelligenceError> {
    normalize_strings(
        hits.iter()
            .flat_map(|hit| hit.provenance_refs.iter())
            .map(|reference| format!("{}#{}", reference.source_path, reference.input_sha256)),
    )
}

fn prompt_terms(prompt: &str) -> Vec<String> {
    let mut terms = BTreeSet::new();
    let mut current = String::new();
    for ch in prompt.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if current.len() >= 2 {
            terms.insert(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }
    if current.len() >= 2 {
        terms.insert(current);
    }
    terms.into_iter().collect()
}

fn normalize_strings(
    values: impl IntoIterator<Item = String>,
) -> Result<Vec<String>, EngineEditorIntelligenceError> {
    let values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if values.is_empty() {
        return invalid("refs", "grounded refs must be non-empty");
    }
    Ok(values)
}

fn validate_action_id(action_id: &str) -> Result<(), EngineEditorIntelligenceError> {
    if action_id.trim().is_empty() || action_id.chars().any(char::is_control) {
        return invalid(
            "action_id",
            "action id must be non-empty and contain no controls",
        );
    }
    Ok(())
}

fn validate_non_negative(value: f32, field: &str) -> Result<(), EngineEditorIntelligenceError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        invalid(field, "value must be finite and non-negative")
    }
}

fn validate_unit_interval(value: f32, field: &str) -> Result<(), EngineEditorIntelligenceError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        invalid(field, "value must be finite and in 0.0..=1.0")
    }
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineEditorIntelligenceError> {
    Err(EngineEditorIntelligenceError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    })
}

fn assay_failed(error: CalyxError) -> EngineEditorIntelligenceError {
    EngineEditorIntelligenceError::Assay {
        code: error.code.to_string(),
        reason: error.message,
    }
}

fn codec_failed(error: impl std::fmt::Display) -> EngineEditorIntelligenceError {
    EngineEditorIntelligenceError::Codec(error.to_string())
}
