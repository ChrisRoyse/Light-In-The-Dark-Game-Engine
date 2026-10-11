use std::collections::BTreeSet;

use litd_hash::sha256_hex;

use crate::EngineError;

use super::{
    CharacterThemeReadback, FactionLeaderActionKind, FactionLeaderDecisionReadback,
    FactionLeaderEvaluationInput, FactionLeaderLoweredEffect, FactionLeaderLoweredOrder,
    FactionLeaderOrderSourceKind, FactionLeaderRules, FactionLeaderSignal, FactionLeaderSignalKind,
    FactionOrganizationDefinition, FactionRankDefinition, FactionThemeFitReadback,
    RPG_FACTION_LEADER_SCHEMA, evaluate_character_theme,
    faction_leaders_readback::{decision_payload, effect_payload, ledger_payload},
    faction_leaders_validate::{
        FactionLeaderIndexes, invalid, validate_faction_input, validate_faction_rules,
    },
    serialize_sha256,
};

pub fn evaluate_faction_leader_signal(
    input: &FactionLeaderEvaluationInput,
    rules: &FactionLeaderRules,
) -> Result<FactionLeaderDecisionReadback, EngineError> {
    validate_faction_rules(rules)?;
    let indexes = validate_faction_input(input, rules)?;
    let organization = indexes
        .organizations
        .get(input.signal.organization_id.as_str())
        .copied()
        .ok_or_else(|| EngineError::RpgFactionInvalidConfig {
            field: "signal.organization_id".to_string(),
            reason: "unknown organization".to_string(),
        })?;
    let theme =
        evaluate_character_theme(&input.theme_input, &input.theme_catalog, &input.theme_rules)?;
    let fit = organization_theme_fit(organization, &theme);
    let target_rank = target_rank(organization, &indexes, &input.signal)?;
    let acceptance = acceptance(
        organization,
        &indexes,
        &input.signal,
        &fit,
        target_rank,
        rules,
    );
    let lowered_effects = if acceptance.accepted {
        vec![lower_effect(
            organization,
            &indexes,
            &input.signal,
            target_rank,
        )?]
    } else {
        Vec::new()
    };
    let decision_id = sha256_hex(
        format!(
            "{}:{}:{}:{}",
            input.catalog.catalog_id,
            input.signal.organization_id,
            input.signal.signal_id,
            input.signal.requested_action.as_str()
        )
        .as_bytes(),
    );
    let mut readback = FactionLeaderDecisionReadback {
        schema: RPG_FACTION_LEADER_SCHEMA.to_string(),
        decision_id,
        organization_id: organization.organization_id.clone(),
        display_name: organization.display_name.clone(),
        organization_kind: organization.organization_kind,
        leader_agent_id: organization.leader_agent_id,
        leader_display_name: organization.leader_display_name.clone(),
        signal_id: input.signal.signal_id.clone(),
        source_cx_id: input.signal.source_cx_id.clone(),
        target_character_id: input.signal.target_character_id.clone(),
        action_kind: input.signal.requested_action,
        accepted: acceptance.accepted,
        grade: acceptance.grade,
        reason: acceptance.reason,
        signal_strength_milli: input.signal.strength_milli,
        rp_quality_milli: input.signal.rp_quality_milli,
        loyalty_milli: input.signal.loyalty_milli,
        target_rank,
        theme_match: theme,
        organization_theme_fit: fit,
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
    organization: &FactionOrganizationDefinition,
    indexes: &FactionLeaderIndexes<'_>,
    signal: &FactionLeaderSignal,
    fit: &FactionThemeFitReadback,
    target_rank: Option<u16>,
    rules: &FactionLeaderRules,
) -> Acceptance {
    if signal.strength_milli < rules.min_signal_strength_milli {
        return reject(
            "signal_too_weak",
            "signal strength is below leader action threshold",
        );
    }
    if !signal_kind_matches_action(signal.kind, signal.requested_action) {
        return reject(
            "signal_action_mismatch",
            "signal kind does not authorize requested leader action",
        );
    }
    if !fit.accepted {
        return reject(
            "theme_mismatch",
            "character theme does not satisfy organization dogma",
        );
    }
    match signal.requested_action {
        FactionLeaderActionKind::SetObjective => accept(
            "accepted",
            "world-state signal and Ward envelope authorize organization objective",
        ),
        FactionLeaderActionKind::WageWarfare => {
            warfare_acceptance(organization, indexes, signal, rules)
        }
        FactionLeaderActionKind::Recruit
        | FactionLeaderActionKind::Induct
        | FactionLeaderActionKind::RewardLoyalty
        | FactionLeaderActionKind::AdvanceRank => {
            roleplay_acceptance(indexes, organization, signal, target_rank, rules)
        }
    }
}

fn warfare_acceptance(
    organization: &FactionOrganizationDefinition,
    indexes: &FactionLeaderIndexes<'_>,
    signal: &FactionLeaderSignal,
    rules: &FactionLeaderRules,
) -> Acceptance {
    if signal.strength_milli < rules.min_warfare_pressure_milli {
        return reject(
            "war_pressure_too_low",
            "item-of-power raid pressure is below warfare threshold",
        );
    }
    let Some(target_id) = signal.target_organization_id.as_deref() else {
        return reject("missing_target", "warfare requires a target organization");
    };
    if !organization
        .enemy_organization_ids
        .iter()
        .any(|enemy| enemy == target_id)
    {
        return reject("not_enemy", "leader cannot raid a non-enemy organization");
    }
    let Some(target) = indexes.organizations.get(target_id).copied() else {
        return reject("missing_target", "target organization is not indexed");
    };
    if target.item_of_power_id.as_deref() != signal.target_item_id.as_deref() {
        return reject(
            "item_mismatch",
            "target item does not match enemy item of power",
        );
    }
    accept(
        "accepted",
        "enemy item-of-power threat lowered to deterministic raid order",
    )
}

fn roleplay_acceptance(
    indexes: &FactionLeaderIndexes<'_>,
    organization: &FactionOrganizationDefinition,
    signal: &FactionLeaderSignal,
    target_rank: Option<u16>,
    rules: &FactionLeaderRules,
) -> Acceptance {
    if signal.rp_quality_milli < rules.min_rp_quality_milli {
        return reject(
            "needs_role_development",
            "roleplay quality score is below organization threshold",
        );
    }
    if matches!(
        signal.requested_action,
        FactionLeaderActionKind::RewardLoyalty | FactionLeaderActionKind::AdvanceRank
    ) && signal.loyalty_milli < rules.min_loyalty_milli
    {
        return reject(
            "needs_loyalty",
            "loyalty score is below organization threshold",
        );
    }
    if signal.requested_action == FactionLeaderActionKind::AdvanceRank {
        let Some(rank) = target_rank else {
            return reject("missing_rank", "rank advancement requires a target rank");
        };
        let Some(rank_def) = rank_definition(indexes, organization, rank) else {
            return reject("missing_rank", "target rank is not defined");
        };
        if signal.rp_quality_milli < rank_def.min_rp_quality_milli {
            return reject(
                "needs_role_development",
                "rank requires stronger RP quality",
            );
        }
        if signal.loyalty_milli < rank_def.min_loyalty_milli {
            return reject("needs_loyalty", "rank requires stronger loyalty");
        }
    }
    accept(
        "accepted",
        "organization theme, roleplay, loyalty, and Ward gates accepted",
    )
}

fn organization_theme_fit(
    organization: &FactionOrganizationDefinition,
    theme: &CharacterThemeReadback,
) -> FactionThemeFitReadback {
    let effective: BTreeSet<&str> = theme
        .effective_theme_tags
        .iter()
        .map(String::as_str)
        .collect();
    let missing_required_tags = organization
        .required_theme_tags
        .iter()
        .filter(|tag| !effective.contains(tag.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let forbidden_tags_present = organization
        .forbidden_theme_tags
        .iter()
        .filter(|tag| effective.contains(tag.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let religion_match_accepted = organization.religion_id.as_ref().is_none_or(|religion_id| {
        theme
            .desired_religion_id
            .as_ref()
            .is_some_and(|desired| desired == religion_id)
            && theme
                .religion_match
                .as_ref()
                .is_some_and(|religion| religion.religion_fit_accepted)
    });
    let accepted = religion_match_accepted
        && missing_required_tags.is_empty()
        && forbidden_tags_present.is_empty();
    FactionThemeFitReadback {
        required_theme_tags: organization.required_theme_tags.clone(),
        missing_required_tags,
        forbidden_tags_present,
        religion_required: organization.religion_id.clone(),
        religion_match_accepted,
        accepted,
    }
}

fn signal_kind_matches_action(
    kind: FactionLeaderSignalKind,
    action: FactionLeaderActionKind,
) -> bool {
    matches!(
        (kind, action),
        (
            FactionLeaderSignalKind::Application,
            FactionLeaderActionKind::Recruit
        ) | (
            FactionLeaderSignalKind::SponsorSupport,
            FactionLeaderActionKind::Induct
        ) | (
            FactionLeaderSignalKind::WorldThreat,
            FactionLeaderActionKind::SetObjective
        ) | (
            FactionLeaderSignalKind::ItemOfPowerThreat,
            FactionLeaderActionKind::WageWarfare
        ) | (
            FactionLeaderSignalKind::LoyaltyReport,
            FactionLeaderActionKind::RewardLoyalty
        ) | (
            FactionLeaderSignalKind::RankReview,
            FactionLeaderActionKind::AdvanceRank
        )
    )
}

fn target_rank(
    organization: &FactionOrganizationDefinition,
    indexes: &FactionLeaderIndexes<'_>,
    signal: &FactionLeaderSignal,
) -> Result<Option<u16>, EngineError> {
    match signal.requested_action {
        FactionLeaderActionKind::Induct => Ok(Some(1)),
        // Validation guarantees `current_rank` is present for loyalty reports
        // (#1484-2); fail loud rather than fabricate rank 1 if that ever regresses.
        FactionLeaderActionKind::RewardLoyalty => match signal.current_rank {
            Some(rank) => Ok(Some(rank)),
            None => invalid(
                "signal.current_rank",
                "reward loyalty requires the member's current rank",
            ),
        },
        FactionLeaderActionKind::AdvanceRank => Ok(signal.requested_rank),
        FactionLeaderActionKind::Recruit
        | FactionLeaderActionKind::SetObjective
        | FactionLeaderActionKind::WageWarfare => Ok(None),
    }
    .and_then(|rank| {
        if let Some(rank) = rank
            && rank_definition(indexes, organization, rank).is_none()
        {
            return invalid("signal.rank", "rank is not defined for organization");
        }
        Ok(rank)
    })
}

fn rank_definition<'a>(
    indexes: &'a FactionLeaderIndexes<'a>,
    organization: &FactionOrganizationDefinition,
    rank: u16,
) -> Option<&'a FactionRankDefinition> {
    indexes
        .ranks_by_organization
        .get(organization.organization_id.as_str())
        .and_then(|ranks| ranks.get(&rank).copied())
}

fn lower_effect(
    organization: &FactionOrganizationDefinition,
    indexes: &FactionLeaderIndexes<'_>,
    signal: &FactionLeaderSignal,
    target_rank: Option<u16>,
) -> Result<FactionLeaderLoweredEffect, EngineError> {
    let effect_id = sha256_hex(
        format!(
            "{}:{}:{}",
            organization.organization_id,
            signal.signal_id,
            signal.requested_action.as_str()
        )
        .as_bytes(),
    );
    let unlocked_power_ids = target_rank
        .and_then(|rank| rank_definition(indexes, organization, rank))
        .map(|rank| rank.power_ids.clone())
        .unwrap_or_default();
    let recruit_invitation_id =
        (signal.requested_action == FactionLeaderActionKind::Recruit).then(|| {
            format!(
                "recruit.{}.{}",
                organization.organization_id, signal.signal_id
            )
        });
    let membership_status = matches!(
        signal.requested_action,
        FactionLeaderActionKind::Induct | FactionLeaderActionKind::AdvanceRank
    )
    .then(|| "member".to_string());
    let objective_id =
        (signal.requested_action == FactionLeaderActionKind::SetObjective).then(|| {
            format!(
                "objective.{}.{}",
                organization.organization_id, signal.signal_id
            )
        });
    let raid_order_id = (signal.requested_action == FactionLeaderActionKind::WageWarfare)
        .then(|| raid_order_id(organization, signal));
    let reward_id =
        (signal.requested_action == FactionLeaderActionKind::RewardLoyalty).then(|| {
            format!(
                "reward.{}.{}",
                organization.organization_id, signal.signal_id
            )
        });
    let order = leader_order(organization, signal, target_rank)?;
    let ledger_entry = format!(
        "{}:{}:{}:{}",
        signal.requested_action.as_str(),
        organization.organization_id,
        signal.target_character_id,
        signal.signal_id
    );
    let mut effect = FactionLeaderLoweredEffect {
        effect_id,
        kind: signal.requested_action,
        subject_character_id: signal.target_character_id.clone(),
        ledger_entry,
        recruit_invitation_id,
        membership_status,
        objective_id,
        raid_order_id,
        reward_id,
        rank_after: target_rank,
        unlocked_power_ids,
        leader_order: order,
        effect_sha256: String::new(),
    };
    effect.effect_sha256 = serialize_sha256(&effect_payload(&effect))?;
    Ok(effect)
}

fn leader_order(
    organization: &FactionOrganizationDefinition,
    signal: &FactionLeaderSignal,
    target_rank: Option<u16>,
) -> Result<FactionLeaderLoweredOrder, EngineError> {
    Ok(FactionLeaderLoweredOrder {
        order_id: sha256_hex(
            format!(
                "order:{}:{}:{}",
                organization.organization_id,
                signal.signal_id,
                signal.requested_action.as_str()
            )
            .as_bytes(),
        ),
        tick: signal
            .tick
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?,
        source_kind: FactionLeaderOrderSourceKind::LoweredAgent,
        lowered_agent_id: organization.leader_agent_id,
        organization_id: organization.organization_id.clone(),
        action_kind: signal.requested_action,
        subject_character_id: signal.target_character_id.clone(),
        target_organization_id: signal.target_organization_id.clone(),
        target_item_id: signal.target_item_id.clone(),
        rank_after: target_rank,
    })
}

fn raid_order_id(
    organization: &FactionOrganizationDefinition,
    signal: &FactionLeaderSignal,
) -> String {
    format!(
        "raid.{}.{}.{}",
        organization.organization_id,
        signal.target_organization_id.as_deref().unwrap_or("none"),
        signal.target_item_id.as_deref().unwrap_or("none")
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
