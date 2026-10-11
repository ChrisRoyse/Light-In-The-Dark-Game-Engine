//! Pre-run Calyx signal verdicts for agent harness authoring checks.

use std::collections::BTreeMap;

use litd_engine::EngineError;

use crate::authoring::{AuthoringStatus, GroundedAbilityRequest, generate_grounded_ability};

use super::{
    HarnessCalyxAuthoredContent, HarnessCalyxSignalStatus, HarnessCalyxSignalSubsystem,
    HarnessCalyxSignalVerdict, HarnessCalyxSignalsRecord, HarnessRunRecord,
    mcp_harness_tools_validation::invalid_harness,
};

#[path = "mcp_harness_tools_calyx_support.rs"]
mod mcp_harness_tools_calyx_support;

use mcp_harness_tools_calyx_support::{
    content_terms, grounded_bits_milli, matched_terms, signal, token_set, validate_panel,
    validate_same_panel,
};

pub(super) fn build_calyx_signals_record(
    signal_batch_id: &str,
    run: Option<&HarnessRunRecord>,
    authored_content: &[HarnessCalyxAuthoredContent],
) -> Result<HarnessCalyxSignalsRecord, EngineError> {
    let mut signals = Vec::new();
    let panel = authored_content.first().map(|item| item.panel.clone());
    if authored_content.is_empty() {
        let run = run.ok_or_else(|| {
            invalid_harness::<()>(
                "calyx_signals.input",
                "run_id or authored_content is required",
            )
            .unwrap_err()
        })?;
        signals.push(run_state_signal(run));
    } else {
        let by_id = authored_content
            .iter()
            .map(|item| (item.content_id.clone(), item))
            .collect::<BTreeMap<_, _>>();
        for content in authored_content {
            validate_panel(&content.panel)?;
            validate_same_panel(panel.as_ref(), &content.panel)?;
            signals.extend(evaluate_authored_content(content, &by_id)?);
        }
    }
    let admitted = signals
        .iter()
        .all(|signal| signal.status == HarnessCalyxSignalStatus::Admitted);
    let record = HarnessCalyxSignalsRecord {
        signal_batch_id: signal_batch_id.to_string(),
        run_id: run.map(|run| run.run_id.clone()),
        source_of_truth: "Calyx Aster MCP harness state row + ledger rows".to_string(),
        panel,
        request_count: authored_content.len(),
        signal_count: signals.len(),
        signals,
        admitted,
    };
    validate_calyx_signals_record(&record)?;
    Ok(record)
}

pub(super) fn validate_calyx_signals_record(
    record: &HarnessCalyxSignalsRecord,
) -> Result<(), EngineError> {
    if record.signal_batch_id.trim().is_empty()
        || record.signal_count == 0
        || record.signal_count != record.signals.len()
    {
        return invalid_harness(
            "calyx_signals.record",
            "signal batch id and signal count must be valid",
        );
    }
    if record.request_count > 0 && record.panel.is_none() {
        return invalid_harness(
            "calyx_signals.panel",
            "authored-content signal batches require a panel readback",
        );
    }
    for signal in &record.signals {
        if signal.content_id.trim().is_empty()
            || signal.code.trim().is_empty()
            || signal.message.trim().is_empty()
            || signal.evidence.is_empty()
            || signal.remediation.trim().is_empty()
        {
            return invalid_harness(
                "calyx_signals.signal",
                "signal verdicts require identity, code, message, evidence, and remediation",
            );
        }
    }
    Ok(())
}

fn evaluate_authored_content(
    content: &HarnessCalyxAuthoredContent,
    by_id: &BTreeMap<String, &HarnessCalyxAuthoredContent>,
) -> Result<Vec<HarnessCalyxSignalVerdict>, EngineError> {
    let outcome = generate_grounded_ability(
        &merged_request(content),
        &content.corpus,
        &content.assay,
        &content.ward,
    )?;
    let bits = grounded_bits_milli(&outcome.grounding);
    let ward = ward_signal(content, &outcome, bits);
    let assay = assay_signal(content, bits);
    let oracle = oracle_signal(content, bits, &ward, &assay);
    let lodestar = lodestar_signal(content);
    let sextant = sextant_signal(content);
    let loom = loom_signal(content, by_id);
    Ok(vec![assay, oracle, ward, lodestar, sextant, loom])
}

fn merged_request(content: &HarnessCalyxAuthoredContent) -> GroundedAbilityRequest {
    let candidate = &content.candidate;
    let mut request = content.request.clone();
    request.prompt = format!(
        "{} {} {} {} {}",
        request.prompt,
        candidate.display_name,
        candidate.damage_type,
        candidate.summary,
        candidate.tags.join(" ")
    );
    request
}

fn ward_signal(
    content: &HarnessCalyxAuthoredContent,
    outcome: &crate::authoring::AuthoringOutcome,
    bits: u32,
) -> HarnessCalyxSignalVerdict {
    match outcome.status {
        AuthoringStatus::Admitted => signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Ward,
            HarnessCalyxSignalStatus::Admitted,
            "LITD-HARNESS-WARD-ADMITTED",
            "authored content is inside the Ward grounding envelope",
            vec![
                format!("grounded_bits_milli={bits}"),
                format!(
                    "matched_terms={}",
                    outcome.grounding.matched_terms.join("|")
                ),
                format!("source_refs={}", outcome.grounding.source_refs.join("|")),
            ],
            "continue to Assay and Oracle gates",
        ),
        AuthoringStatus::Provisional => signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Ward,
            HarnessCalyxSignalStatus::Insufficient,
            "LITD-HARNESS-WARD-INSUFFICIENT-GROUNDING",
            "Ward refused to admit content without more grounded evidence",
            vec![
                format!("grounded_bits_milli={bits}"),
                format!(
                    "required_terms={} required_sources={}",
                    content.ward.min_grounded_terms, content.ward.min_source_anchors
                ),
            ],
            "add grounded world-corpus anchors before lowering",
        ),
        AuthoringStatus::Refused => signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Ward,
            HarnessCalyxSignalStatus::Refused,
            "LITD-HARNESS-WARD-OOD",
            outcome.admission_code.clone(),
            vec![
                format!(
                    "blocked_terms={}",
                    outcome.grounding.blocked_terms.join("|")
                ),
                format!(
                    "matched_terms={}",
                    outcome.grounding.matched_terms.join("|")
                ),
            ],
            "remove out-of-domain or unsafe authored content",
        ),
    }
}

fn assay_signal(content: &HarnessCalyxAuthoredContent, bits: u32) -> HarnessCalyxSignalVerdict {
    let assay = &content.assay;
    let candidate = &content.candidate;
    let mut failures = Vec::new();
    if candidate.damage < assay.min_damage || candidate.damage > assay.max_damage {
        failures.push(format!(
            "damage={} outside {}..{}",
            candidate.damage, assay.min_damage, assay.max_damage
        ));
    }
    if candidate.cooldown_seconds < assay.min_cooldown_seconds
        || candidate.cooldown_seconds > assay.max_cooldown_seconds
    {
        failures.push(format!(
            "cooldown={} outside {}..{}",
            candidate.cooldown_seconds, assay.min_cooldown_seconds, assay.max_cooldown_seconds
        ));
    }
    if candidate.mana_cost > assay.max_mana_cost {
        failures.push(format!(
            "mana_cost={} exceeds {}",
            candidate.mana_cost, assay.max_mana_cost
        ));
    }
    if !assay
        .allowed_damage_types
        .iter()
        .any(|kind| kind == &candidate.damage_type)
    {
        failures.push(format!("damage_type={} not allowed", candidate.damage_type));
    }
    if failures.is_empty() {
        signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Assay,
            HarnessCalyxSignalStatus::Admitted,
            "LITD-HARNESS-ASSAY-BALANCED",
            "candidate stayed inside measured Assay balance bounds",
            vec![
                format!("assay_id={}", assay.assay_id),
                format!("profile_id={}", assay.profile_id),
                format!("grounded_bits_milli={bits}"),
            ],
            "continue to Ward and Oracle gates",
        )
    } else {
        signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Assay,
            HarnessCalyxSignalStatus::Refused,
            "LITD-HARNESS-ASSAY-IMBALANCED",
            "candidate violated measured Assay balance bounds",
            failures,
            "rebalance the authored candidate before any sim run",
        )
    }
}

fn oracle_signal(
    content: &HarnessCalyxAuthoredContent,
    bits: u32,
    ward: &HarnessCalyxSignalVerdict,
    assay: &HarnessCalyxSignalVerdict,
) -> HarnessCalyxSignalVerdict {
    let min = content.oracle.min_grounded_bits_milli;
    let honesty = if min == 0 {
        1000
    } else {
        bits.saturating_mul(1000)
            .checked_div(min)
            .unwrap_or(0)
            .min(1000)
    };
    if bits < min || honesty < u32::from(content.oracle.min_honesty_milli) {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Oracle,
            HarnessCalyxSignalStatus::Insufficient,
            "LITD-HARNESS-ORACLE-INSUFFICIENT",
            "Oracle honesty gate refused to guess with insufficient grounded bits",
            vec![
                format!("grounded_bits_milli={bits}"),
                format!("min_grounded_bits_milli={min}"),
                format!("honesty_milli={honesty}"),
            ],
            "add grounded anchors or lower the authored scope before asking Oracle",
        );
    }
    if ward.status != HarnessCalyxSignalStatus::Admitted
        || assay.status != HarnessCalyxSignalStatus::Admitted
    {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Oracle,
            HarnessCalyxSignalStatus::Refused,
            "LITD-HARNESS-ORACLE-REFUSED-UPSTREAM",
            "Oracle refused what-if preview because Ward or Assay failed first",
            vec![
                format!("ward={}", ward.code),
                format!("assay={}", assay.code),
            ],
            "fix the upstream Ward/Assay refusal before consequence preview",
        );
    }
    signal(
        &content.content_id,
        HarnessCalyxSignalSubsystem::Oracle,
        HarnessCalyxSignalStatus::Admitted,
        "LITD-HARNESS-ORACLE-ADMITTED",
        "Oracle honesty gate had enough grounded bits for a pre-run consequence signal",
        vec![
            format!("grounded_bits_milli={bits}"),
            format!("honesty_milli={honesty}"),
        ],
        "safe to run focused what-if or playtest checks",
    )
}

fn lodestar_signal(content: &HarnessCalyxAuthoredContent) -> HarnessCalyxSignalVerdict {
    let matched = matched_terms(&content.lodestar_question, content);
    if matched.terms.is_empty() {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Lodestar,
            HarnessCalyxSignalStatus::Insufficient,
            "LITD-HARNESS-LODESTAR-INSUFFICIENT-KERNEL",
            "Lodestar refused answer-over-world-corpus without grounded terms",
            vec![format!("question={}", content.lodestar_question)],
            "ask with terms that exist in the world corpus",
        );
    }
    signal(
        &content.content_id,
        HarnessCalyxSignalSubsystem::Lodestar,
        HarnessCalyxSignalStatus::Admitted,
        "LITD-HARNESS-LODESTAR-ANSWERED",
        "Lodestar found grounded corpus terms for the pre-run question",
        vec![
            format!("matched_terms={}", matched.terms.join("|")),
            format!("source_refs={}", matched.sources.join("|")),
        ],
        "use this answer as pre-run feedback, then verify with source readback",
    )
}

fn sextant_signal(content: &HarnessCalyxAuthoredContent) -> HarnessCalyxSignalVerdict {
    let matched = matched_terms(&content.sextant_query, content);
    let total = token_set(&content.sextant_query).len().max(1);
    let score_milli = ((matched.terms.len() * 1000) / total) as u16;
    if matched.terms.is_empty() || score_milli < content.search_calibration.tau_milli {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Sextant,
            HarnessCalyxSignalStatus::Refused,
            "LITD-HARNESS-SEXTANT-REFUSED-OOD",
            "Sextant semantic search found no calibrated grounded match",
            vec![
                format!("score_milli={score_milli}"),
                format!("tau_milli={}", content.search_calibration.tau_milli),
            ],
            "change the query or add grounded content before search",
        );
    }
    let borderline = content
        .search_calibration
        .tau_milli
        .saturating_add(content.search_calibration.borderline_margin_milli);
    if score_milli <= borderline {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Sextant,
            HarnessCalyxSignalStatus::Insufficient,
            "LITD-HARNESS-SEXTANT-BORDERLINE",
            "Sextant match was too close to the calibrated boundary",
            vec![
                format!("score_milli={score_milli}"),
                format!("borderline_milli={borderline}"),
            ],
            "add stronger grounded terms before admitting this search result",
        );
    }
    signal(
        &content.content_id,
        HarnessCalyxSignalSubsystem::Sextant,
        HarnessCalyxSignalStatus::Admitted,
        "LITD-HARNESS-SEXTANT-ADMITTED",
        "Sextant semantic search returned calibrated grounded matches",
        vec![
            format!("score_milli={score_milli}"),
            format!("matched_terms={}", matched.terms.join("|")),
            format!("source_refs={}", matched.sources.join("|")),
        ],
        "use returned anchors as pre-run feedback evidence",
    )
}

fn loom_signal(
    content: &HarnessCalyxAuthoredContent,
    by_id: &BTreeMap<String, &HarnessCalyxAuthoredContent>,
) -> HarnessCalyxSignalVerdict {
    let Some(target_id) = content.similar_to_content_id.as_deref() else {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Loom,
            HarnessCalyxSignalStatus::Insufficient,
            "LITD-HARNESS-LOOM-NO-COMPARATOR",
            "Loom find-similar needs another authored content id",
            vec!["similar_to_content_id=null".to_string()],
            "provide a real comparator content id in the same signal batch",
        );
    };
    let Some(target) = by_id.get(target_id) else {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Loom,
            HarnessCalyxSignalStatus::Refused,
            "LITD-HARNESS-LOOM-MISSING-COMPARATOR",
            "Loom comparator id was not present in the batch",
            vec![format!("similar_to_content_id={target_id}")],
            "include the comparator in authored_content before running the signal batch",
        );
    };
    let left = content_terms(content);
    let right = content_terms(target);
    let shared = left.intersection(&right).count();
    let union = left.union(&right).count().max(1);
    let similarity_milli = (shared * 1000 / union) as u16;
    if similarity_milli == 0 {
        return signal(
            &content.content_id,
            HarnessCalyxSignalSubsystem::Loom,
            HarnessCalyxSignalStatus::Refused,
            "LITD-HARNESS-LOOM-NO-SIMILARITY",
            "Loom found no shared grounded terms",
            vec![format!("similarity_milli={similarity_milli}")],
            "use find-similar only for content with overlapping grounded concepts",
        );
    }
    signal(
        &content.content_id,
        HarnessCalyxSignalSubsystem::Loom,
        HarnessCalyxSignalStatus::Admitted,
        "LITD-HARNESS-LOOM-SIMILAR",
        "Loom found similar authored content in the batch",
        vec![
            format!("similar_to_content_id={target_id}"),
            format!("similarity_milli={similarity_milli}"),
        ],
        "surface the similar content as agent feedback before a full run",
    )
}

fn run_state_signal(run: &HarnessRunRecord) -> HarnessCalyxSignalVerdict {
    let admitted = run.verdict == "passed" && run.spawned_bodies > 0;
    signal(
        &run.run_id,
        HarnessCalyxSignalSubsystem::RunState,
        if admitted {
            HarnessCalyxSignalStatus::Admitted
        } else {
            HarnessCalyxSignalStatus::Refused
        },
        if admitted {
            "LITD-HARNESS-RUN-STATE-ADMITTED"
        } else {
            "LITD-HARNESS-RUN-STATE-REFUSED"
        },
        "stored harness run state was converted to a typed signal verdict",
        vec![
            format!("verdict={}", run.verdict),
            format!("spawned_bodies={}", run.spawned_bodies),
            format!("visible_instances={}", run.visible_instance_count),
        ],
        "use authored_content for pre-run Calyx subsystem feedback",
    )
}
