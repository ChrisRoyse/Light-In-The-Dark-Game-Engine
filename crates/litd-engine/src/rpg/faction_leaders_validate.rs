use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::{
    FactionLeaderCatalog, FactionLeaderEvaluationInput, FactionLeaderRules, FactionLeaderSignal,
    FactionLeaderSignalKind, FactionOrganizationDefinition, FactionRankDefinition,
    RPG_FACTION_LEADER_SCHEMA,
};

pub(super) struct FactionLeaderIndexes<'a> {
    pub organizations: BTreeMap<&'a str, &'a FactionOrganizationDefinition>,
    pub ranks_by_organization: BTreeMap<&'a str, BTreeMap<u16, &'a FactionRankDefinition>>,
}

pub(super) fn validate_faction_rules(rules: &FactionLeaderRules) -> Result<(), EngineError> {
    if rules.schema != RPG_FACTION_LEADER_SCHEMA {
        return invalid(
            "rules.schema",
            format!("expected {RPG_FACTION_LEADER_SCHEMA}"),
        );
    }
    validate_milli_nonzero(
        "rules.min_signal_strength_milli",
        rules.min_signal_strength_milli,
    )?;
    validate_milli("rules.min_rp_quality_milli", rules.min_rp_quality_milli)?;
    validate_milli("rules.min_loyalty_milli", rules.min_loyalty_milli)?;
    validate_milli(
        "rules.min_warfare_pressure_milli",
        rules.min_warfare_pressure_milli,
    )?;
    if rules.max_rank == 0 || rules.max_rank > 100 {
        return invalid("rules.max_rank", "must be inside 1..=100");
    }
    Ok(())
}

pub(super) fn validate_faction_input<'a>(
    input: &'a FactionLeaderEvaluationInput,
    rules: &FactionLeaderRules,
) -> Result<FactionLeaderIndexes<'a>, EngineError> {
    let indexes = validate_catalog(&input.catalog)?;
    validate_signal(&input.signal)?;
    let organization = indexes
        .organizations
        .get(input.signal.organization_id.as_str())
        .ok_or_else(|| invalid_error("signal.organization_id", "unknown organization"))?;
    if !organization
        .allowed_actions
        .contains(&input.signal.requested_action)
    {
        return invalid(
            "signal.requested_action",
            "organization leader is not allowed to perform requested action",
        );
    }
    for tag in &input.signal.tags {
        if !organization.ward_tags.contains(tag) {
            return invalid(
                "signal.tags",
                format!("signal tag {tag} is outside organization Ward envelope"),
            );
        }
    }
    if let Some(rank) = input.signal.current_rank {
        require_rank(&indexes, organization.organization_id.as_str(), rank)?;
    }
    if let Some(rank) = input.signal.requested_rank {
        if rank > rules.max_rank {
            return invalid(
                "signal.requested_rank",
                "requested rank exceeds rules.max_rank",
            );
        }
        require_rank(&indexes, organization.organization_id.as_str(), rank)?;
    }
    if let Some(target) = &input.signal.target_organization_id
        && !indexes.organizations.contains_key(target.as_str())
    {
        return invalid(
            "signal.target_organization_id",
            "unknown target organization",
        );
    }
    if input.signal.kind == FactionLeaderSignalKind::ItemOfPowerThreat
        && (input.signal.target_organization_id.is_none() || input.signal.target_item_id.is_none())
    {
        return invalid(
            "signal.item_of_power",
            "warfare requires target organization and item-of-power id",
        );
    }
    if input.signal.kind == FactionLeaderSignalKind::RankReview
        && input.signal.requested_rank.is_none()
    {
        return invalid(
            "signal.requested_rank",
            "rank review requires requested rank",
        );
    }
    // A loyalty report drives `RewardLoyalty`, whose target rank is the member's
    // *current* rank. Without this guard `target_rank` fabricates rank 1 for an
    // unranked member and grants rank-1 powers from invented state (#1484-2) —
    // symmetric to the RankReview/requested_rank guard above.
    if input.signal.kind == FactionLeaderSignalKind::LoyaltyReport
        && input.signal.current_rank.is_none()
    {
        return invalid(
            "signal.current_rank",
            "loyalty report requires the member's current rank",
        );
    }
    Ok(indexes)
}

pub(super) fn validate_catalog(
    catalog: &FactionLeaderCatalog,
) -> Result<FactionLeaderIndexes<'_>, EngineError> {
    if catalog.schema != RPG_FACTION_LEADER_SCHEMA {
        return invalid(
            "catalog.schema",
            format!("expected {RPG_FACTION_LEADER_SCHEMA}"),
        );
    }
    validate_token("catalog.catalog_id", &catalog.catalog_id)?;
    if catalog.organizations.is_empty() {
        return invalid(
            "catalog.organizations",
            "at least one organization is required",
        );
    }
    let mut organizations = BTreeMap::new();
    let mut ranks_by_organization = BTreeMap::new();
    for organization in &catalog.organizations {
        let ranks = validate_organization(organization)?;
        if organizations
            .insert(organization.organization_id.as_str(), organization)
            .is_some()
        {
            return invalid("organizations.organization_id", "duplicate organization");
        }
        ranks_by_organization.insert(organization.organization_id.as_str(), ranks);
    }
    for organization in &catalog.organizations {
        for enemy_id in &organization.enemy_organization_ids {
            if enemy_id == &organization.organization_id {
                return invalid(
                    "organizations.enemy_organization_ids",
                    "organization cannot enemy itself",
                );
            }
            if !organizations.contains_key(enemy_id.as_str()) {
                return invalid(
                    "organizations.enemy_organization_ids",
                    "unknown enemy organization",
                );
            }
        }
    }
    Ok(FactionLeaderIndexes {
        organizations,
        ranks_by_organization,
    })
}

fn validate_organization(
    organization: &FactionOrganizationDefinition,
) -> Result<BTreeMap<u16, &FactionRankDefinition>, EngineError> {
    validate_token(
        "organizations.organization_id",
        &organization.organization_id,
    )?;
    validate_display("organizations.display_name", &organization.display_name)?;
    if organization.leader_agent_id == 0 {
        return invalid("organizations.leader_agent_id", "must be non-zero");
    }
    validate_display(
        "organizations.leader_display_name",
        &organization.leader_display_name,
    )?;
    if let Some(religion_id) = &organization.religion_id {
        validate_token("organizations.religion_id", religion_id)?;
    }
    validate_token("organizations.home_region_id", &organization.home_region_id)?;
    if let Some(item_id) = &organization.item_of_power_id {
        validate_token("organizations.item_of_power_id", item_id)?;
    }
    validate_actions(organization)?;
    validate_tag_rows("organizations.ward_tags", &organization.ward_tags, false)?;
    validate_tag_rows(
        "organizations.required_theme_tags",
        &organization.required_theme_tags,
        false,
    )?;
    validate_tag_rows(
        "organizations.forbidden_theme_tags",
        &organization.forbidden_theme_tags,
        true,
    )?;
    validate_token_rows(
        "organizations.enemy_organization_ids",
        &organization.enemy_organization_ids,
        true,
    )?;
    validate_ranks(&organization.ranks)
}

fn validate_actions(organization: &FactionOrganizationDefinition) -> Result<(), EngineError> {
    if organization.allowed_actions.is_empty() {
        return invalid(
            "organizations.allowed_actions",
            "at least one allowed action is required",
        );
    }
    let mut actions = BTreeSet::new();
    for action in &organization.allowed_actions {
        if !actions.insert(*action) {
            return invalid("organizations.allowed_actions", "duplicate action");
        }
    }
    Ok(())
}

fn validate_ranks(
    ranks: &[FactionRankDefinition],
) -> Result<BTreeMap<u16, &FactionRankDefinition>, EngineError> {
    if ranks.is_empty() {
        return invalid("organizations.ranks", "at least one rank is required");
    }
    let mut by_rank = BTreeMap::new();
    for rank in ranks {
        if rank.rank == 0 || rank.rank > 100 {
            return invalid("ranks.rank", "must be inside 1..=100");
        }
        validate_display("ranks.title", &rank.title)?;
        validate_milli("ranks.min_rp_quality_milli", rank.min_rp_quality_milli)?;
        validate_milli("ranks.min_loyalty_milli", rank.min_loyalty_milli)?;
        validate_token_rows("ranks.power_ids", &rank.power_ids, false)?;
        if by_rank.insert(rank.rank, rank).is_some() {
            return invalid("ranks.rank", "duplicate rank");
        }
    }
    if !by_rank.contains_key(&1) {
        return invalid("ranks.rank", "rank 1 is required for induction");
    }
    Ok(by_rank)
}

fn validate_signal(signal: &FactionLeaderSignal) -> Result<(), EngineError> {
    validate_token("signal.signal_id", &signal.signal_id)?;
    validate_token("signal.organization_id", &signal.organization_id)?;
    validate_milli_nonzero("signal.strength_milli", signal.strength_milli)?;
    validate_milli("signal.rp_quality_milli", signal.rp_quality_milli)?;
    validate_milli("signal.loyalty_milli", signal.loyalty_milli)?;
    validate_token("signal.source_cx_id", &signal.source_cx_id)?;
    validate_token("signal.target_character_id", &signal.target_character_id)?;
    if let Some(target) = &signal.target_organization_id {
        validate_token("signal.target_organization_id", target)?;
    }
    if let Some(item) = &signal.target_item_id {
        validate_token("signal.target_item_id", item)?;
    }
    validate_tag_rows("signal.tags", &signal.tags, false)?;
    validate_ref_rows("signal.evidence_refs", &signal.evidence_refs)?;
    Ok(())
}

fn require_rank(
    indexes: &FactionLeaderIndexes<'_>,
    organization_id: &str,
    rank: u16,
) -> Result<(), EngineError> {
    let ranks = indexes
        .ranks_by_organization
        .get(organization_id)
        .ok_or_else(|| invalid_error("ranks.organization_id", "missing rank index"))?;
    if !ranks.contains_key(&rank) {
        return invalid("signal.rank", "rank is not defined for organization");
    }
    Ok(())
}

fn validate_milli(field: &str, value: u16) -> Result<(), EngineError> {
    if value > 1000 {
        return invalid(field, "must be inside 0..=1000");
    }
    Ok(())
}

fn validate_milli_nonzero(field: &str, value: u16) -> Result<(), EngineError> {
    if value == 0 || value > 1000 {
        return invalid(field, "must be inside 1..=1000");
    }
    Ok(())
}

fn validate_ref_rows(field: &str, rows: &[String]) -> Result<(), EngineError> {
    if rows.is_empty() {
        return invalid(field, "at least one reference is required");
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        if row.trim().is_empty() || row.len() > 160 {
            return invalid(field, "references must be 1..=160 bytes");
        }
        if !seen.insert(row.as_str()) {
            return invalid(field, "duplicate reference");
        }
    }
    Ok(())
}

fn validate_tag_rows(field: &str, rows: &[String], allow_empty: bool) -> Result<(), EngineError> {
    if (!allow_empty && rows.is_empty()) || rows.len() > 32 {
        return invalid(field, "must contain 1..=32 tags");
    }
    validate_token_rows(field, rows, allow_empty)
}

fn validate_token_rows(field: &str, rows: &[String], allow_empty: bool) -> Result<(), EngineError> {
    if !allow_empty && rows.is_empty() {
        return invalid(field, "at least one id is required");
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        validate_token(field, row)?;
        if !seen.insert(row.as_str()) {
            return invalid(field, "duplicate id");
        }
    }
    Ok(())
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 96 {
        return invalid(field, "must be 1..=96 bytes");
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be 1..=96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgFactionInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    }
}
