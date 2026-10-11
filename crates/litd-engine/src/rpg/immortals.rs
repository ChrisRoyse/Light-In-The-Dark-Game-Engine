use super::serialize_sha256;
use litd_hash::sha256_hex;
use serde_json::json;

use crate::{
    EngineError,
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

use super::{
    CharacterThemeReadback, ImmortalActionKind, ImmortalDecisionReadback, ImmortalDefinition,
    ImmortalEvaluationInput, ImmortalLoweredEffect, ImmortalRules, ImmortalSignal,
    ImmortalSignalKind, RPG_IMMORTAL_SCHEMA, evaluate_character_theme,
    immortals_validate::{validate_immortal_input, validate_immortal_rules},
};

pub fn evaluate_immortal_signal(
    input: &ImmortalEvaluationInput,
    rules: &ImmortalRules,
) -> Result<ImmortalDecisionReadback, EngineError> {
    validate_immortal_rules(rules)?;
    let indexes = validate_immortal_input(input)?;
    let immortal = indexes
        .immortals
        .get(input.signal.immortal_id.as_str())
        .copied()
        .ok_or_else(|| EngineError::RpgImmortalInvalidConfig {
            field: "signal.immortal_id".to_string(),
            reason: "unknown immortal".to_string(),
        })?;
    let theme =
        evaluate_character_theme(&input.theme_input, &input.theme_catalog, &input.theme_rules)?;
    let acceptance = acceptance(immortal, &input.signal, &theme, rules);
    let lowered_effects = if acceptance.accepted {
        vec![lower_effect(immortal, &input.signal, rules)?]
    } else {
        Vec::new()
    };
    let decision_id = sha256_hex(
        format!(
            "{}:{}:{}:{}",
            input.catalog.catalog_id,
            input.signal.immortal_id,
            input.signal.signal_id,
            input.signal.requested_action.as_str()
        )
        .as_bytes(),
    );
    let mut readback = ImmortalDecisionReadback {
        schema: RPG_IMMORTAL_SCHEMA.to_string(),
        decision_id,
        immortal_id: immortal.immortal_id.clone(),
        display_name: immortal.display_name.clone(),
        religion_id: immortal.religion_id.clone(),
        signal_id: input.signal.signal_id.clone(),
        source_cx_id: input.signal.source_cx_id.clone(),
        target_character_id: input.signal.target_character_id.clone(),
        action_kind: input.signal.requested_action,
        accepted: acceptance.accepted,
        grade: acceptance.grade,
        reason: acceptance.reason,
        signal_strength_milli: input.signal.strength_milli,
        rp_quality_milli: input.signal.rp_quality_milli,
        theme_match: theme,
        effect_count: lowered_effects.len(),
        lowered_effects,
        evidence_refs: input.signal.evidence_refs.clone(),
        catalog_sha256: serialize_sha256(&input.catalog)?,
        signal_sha256: serialize_sha256(&input.signal)?,
        rules_sha256: serialize_sha256(rules)?,
        theme_result_sha256: String::new(),
        decision_ledger_sha256: String::new(),
        result_sha256: String::new(),
    };
    readback.theme_result_sha256 = readback.theme_match.result_sha256.clone();
    readback.decision_ledger_sha256 = serialize_sha256(&ledger_payload(&readback))?;
    readback.result_sha256 = serialize_sha256(&decision_payload(&readback))?;
    Ok(readback)
}

struct Acceptance {
    accepted: bool,
    grade: String,
    reason: String,
}

fn acceptance(
    immortal: &ImmortalDefinition,
    signal: &ImmortalSignal,
    theme: &CharacterThemeReadback,
    rules: &ImmortalRules,
) -> Acceptance {
    if signal.strength_milli < rules.min_signal_strength_milli {
        return reject(
            "signal_too_weak",
            "signal strength is below Immortal action threshold",
        );
    }
    if !signal_kind_matches_action(signal.kind, signal.requested_action) {
        return reject(
            "signal_action_mismatch",
            "signal kind does not authorize requested Immortal action",
        );
    }
    if theme
        .desired_religion_id
        .as_deref()
        .is_none_or(|religion_id| religion_id != immortal.religion_id)
    {
        return reject(
            "religion_mismatch",
            "Immortal cannot act as a different religion/deity",
        );
    }
    match signal.requested_action {
        ImmortalActionKind::GrantEmpowerment => empowerment_acceptance(signal, theme, rules),
        ImmortalActionKind::IssueQuest | ImmortalActionKind::RunEvent => {
            roleplay_acceptance(signal, rules, "accepted")
        }
        ImmortalActionKind::EnforceRoleplay => {
            accept("accepted", "roleplay violation is ledgered for enforcement")
        }
        ImmortalActionKind::PossessMob => accept(
            "accepted",
            "possession request lowered to deterministic sim order",
        ),
    }
}

fn empowerment_acceptance(
    signal: &ImmortalSignal,
    theme: &CharacterThemeReadback,
    rules: &ImmortalRules,
) -> Acceptance {
    let Some(religion) = &theme.religion_match else {
        return reject(
            "missing_religion",
            "empowerment requires a desired religion",
        );
    };
    if !religion.empowerment_fit_accepted {
        return reject(
            "needs_role_development",
            "theme/religion fit does not satisfy empowerment",
        );
    }
    roleplay_acceptance(signal, rules, "accepted")
}

fn roleplay_acceptance(signal: &ImmortalSignal, rules: &ImmortalRules, grade: &str) -> Acceptance {
    if signal.rp_quality_milli < rules.min_rp_quality_milli {
        return reject(
            "needs_role_development",
            "roleplay quality score is below Immortal action threshold",
        );
    }
    accept(
        grade,
        "signal, Ward envelope, religion, and roleplay gates accepted",
    )
}

fn accept(grade: &str, reason: &str) -> Acceptance {
    Acceptance {
        accepted: true,
        grade: grade.to_string(),
        reason: reason.to_string(),
    }
}

fn reject(grade: &str, reason: &str) -> Acceptance {
    Acceptance {
        accepted: false,
        grade: grade.to_string(),
        reason: reason.to_string(),
    }
}

fn signal_kind_matches_action(kind: ImmortalSignalKind, action: ImmortalActionKind) -> bool {
    matches!(
        (kind, action),
        (
            ImmortalSignalKind::Prayer | ImmortalSignalKind::ShrineVisit,
            ImmortalActionKind::GrantEmpowerment
        ) | (
            ImmortalSignalKind::QuestOpportunity | ImmortalSignalKind::RoleplayScene,
            ImmortalActionKind::IssueQuest
        ) | (
            ImmortalSignalKind::EventOpportunity | ImmortalSignalKind::RoleplayScene,
            ImmortalActionKind::RunEvent
        ) | (
            ImmortalSignalKind::RuleBreak,
            ImmortalActionKind::EnforceRoleplay
        ) | (
            ImmortalSignalKind::PossessionRequest,
            ImmortalActionKind::PossessMob
        )
    )
}

fn lower_effect(
    immortal: &ImmortalDefinition,
    signal: &ImmortalSignal,
    rules: &ImmortalRules,
) -> Result<ImmortalLoweredEffect, EngineError> {
    let effect_id = sha256_hex(
        format!(
            "{}:{}:{}",
            immortal.immortal_id,
            signal.signal_id,
            signal.requested_action.as_str()
        )
        .as_bytes(),
    );
    let (empowerment_tier, quest_id, event_id, enforcement_severity, sim_order) =
        match signal.requested_action {
            ImmortalActionKind::GrantEmpowerment => (
                Some(
                    immortal
                        .max_empowerment_tier
                        .min(rules.max_empowerment_tier),
                ),
                None,
                None,
                None,
                None,
            ),
            ImmortalActionKind::IssueQuest => (
                None,
                Some(format!("quest.immortal.{}", signal.signal_id)),
                None,
                None,
                None,
            ),
            ImmortalActionKind::RunEvent => (
                None,
                None,
                Some(format!("event.immortal.{}", signal.signal_id)),
                None,
                None,
            ),
            ImmortalActionKind::EnforceRoleplay => {
                (None, None, None, Some("warning".to_string()), None)
            }
            ImmortalActionKind::PossessMob => (
                None,
                None,
                None,
                None,
                Some(possession_order(immortal, signal)?),
            ),
        };
    let ledger_entry = format!(
        "{}:{}:{}:{}",
        signal.requested_action.as_str(),
        immortal.immortal_id,
        signal.target_character_id,
        signal.signal_id
    );
    let mut effect = ImmortalLoweredEffect {
        effect_id,
        kind: signal.requested_action,
        subject_character_id: signal.target_character_id.clone(),
        ledger_entry,
        empowerment_tier,
        quest_id,
        event_id,
        enforcement_severity,
        sim_order,
        effect_sha256: String::new(),
    };
    effect.effect_sha256 = serialize_sha256(&effect_payload(&effect))?;
    Ok(effect)
}

fn possession_order(
    immortal: &ImmortalDefinition,
    signal: &ImmortalSignal,
) -> Result<SimOrder, EngineError> {
    let target = signal
        .target_body_id
        .ok_or_else(|| EngineError::RpgImmortalInvalidConfig {
            field: "signal.target_body_id".to_string(),
            reason: "possession requires target body id".to_string(),
        })?;
    let velocity =
        signal
            .possession_velocity
            .ok_or_else(|| EngineError::RpgImmortalInvalidConfig {
                field: "signal.possession_velocity".to_string(),
                reason: "possession requires deterministic velocity".to_string(),
            })?;
    Ok(SimOrder::new(
        signal
            .tick
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?,
        SimOrderSource::lowered_agent(immortal.lowered_agent_id),
        1,
        SimBodyId::from_raw(target),
        SimOrderKind::SetVelocity(velocity),
    ))
}

fn ledger_payload(readback: &ImmortalDecisionReadback) -> serde_json::Value {
    json!({
        "decision_id": readback.decision_id,
        "accepted": readback.accepted,
        "grade": readback.grade,
        "action_kind": readback.action_kind,
        "lowered_effects": readback.lowered_effects,
        "evidence_refs": readback.evidence_refs,
    })
}

fn decision_payload(readback: &ImmortalDecisionReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "decision_id": readback.decision_id,
        "immortal_id": readback.immortal_id,
        "religion_id": readback.religion_id,
        "signal_id": readback.signal_id,
        "action_kind": readback.action_kind,
        "accepted": readback.accepted,
        "grade": readback.grade,
        "reason": readback.reason,
        "effect_count": readback.effect_count,
        "catalog_sha256": readback.catalog_sha256,
        "signal_sha256": readback.signal_sha256,
        "rules_sha256": readback.rules_sha256,
        "theme_result_sha256": readback.theme_result_sha256,
        "decision_ledger_sha256": readback.decision_ledger_sha256,
    })
}

fn effect_payload(effect: &ImmortalLoweredEffect) -> serde_json::Value {
    json!({
        "effect_id": effect.effect_id,
        "kind": effect.kind,
        "subject_character_id": effect.subject_character_id,
        "ledger_entry": effect.ledger_entry,
        "empowerment_tier": effect.empowerment_tier,
        "quest_id": effect.quest_id,
        "event_id": effect.event_id,
        "enforcement_severity": effect.enforcement_severity,
        "sim_order": effect.sim_order,
    })
}
