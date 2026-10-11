use std::collections::{BTreeMap, BTreeSet};

use super::serialize_sha256;
use serde_json::json;

use crate::EngineError;

use super::{
    InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalCircleId, InfernalRelicItem,
    InfernalRelicItemReadback, InfernalRelicRarity, InfernalRelicSetBonus, InfernalRelicVault,
    InfernalRelicVaultKind, InfernalRelicVaultPolicy, InfernalRelicVaultReadback,
    InfernalZonePolicy, RPG_INFERNAL_RELIC_VAULTS_SCHEMA, boss_for_circle,
    infernal_boss_agent_policy_hash, infernal_zone_policy_hash,
    validate_infernal_boss_agent_policy, zone_for_circle,
};

const EXPECTED_VAULTS: [InfernalRelicVaultKind; 2] = [
    InfernalRelicVaultKind::MoltenThroneRoad,
    InfernalRelicVaultKind::ObsidianThrone,
];

pub fn derive_infernal_relic_vault_readback(
    policy: &InfernalRelicVaultPolicy,
    zone_policy: &InfernalZonePolicy,
    boss_policy: &InfernalBossAgentPolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<InfernalRelicVaultReadback, EngineError> {
    validate_infernal_relic_vault_policy(policy, zone_policy, boss_policy, attrition_policy)?;
    let world_finite_item_total = policy
        .item_set
        .iter()
        .map(|item| item.world_finite_count)
        .sum::<u16>();
    let highest_power_rating_milli = policy
        .item_set
        .iter()
        .map(|item| item.power_rating_milli)
        .max()
        .unwrap_or_default();
    let item_rows = policy
        .item_set
        .iter()
        .map(|item| InfernalRelicItemReadback {
            item_id: item.item_id.clone(),
            display_name: item.display_name.clone(),
            slot: item.slot,
            source_vault_id: item.source_vault_id.clone(),
            world_finite_count: item.world_finite_count,
            power_rating_milli: item.power_rating_milli,
            perk_count: item.perks.len(),
            guarded_by_ruler_ref: item.guarded_by_ruler_ref.clone(),
        })
        .collect::<Vec<_>>();
    let mut readback = InfernalRelicVaultReadback {
        schema: RPG_INFERNAL_RELIC_VAULTS_SCHEMA.to_string(),
        server_policy_id: policy.server_policy_id.clone(),
        source_issue: policy.source_issue,
        built_in_map_required: policy.built_in_map_required,
        world_finite_enforced: policy.world_finite_enforced,
        vault_count: policy.vaults.len(),
        item_count: policy.item_set.len(),
        set_bonus_count: policy.set_bonuses.len(),
        world_finite_item_total,
        highest_power_rating_milli,
        molten_vault_guard: vault_for_kind(policy, InfernalRelicVaultKind::MoltenThroneRoad)?
            .guarded_by_ruler_ref
            .clone(),
        obsidian_vault_guard: vault_for_kind(policy, InfernalRelicVaultKind::ObsidianThrone)?
            .guarded_by_ruler_ref
            .clone(),
        item_rows,
        policy_sha256: infernal_relic_vault_policy_hash(policy)?,
        zone_policy_sha256: infernal_zone_policy_hash(zone_policy)?,
        boss_policy_sha256: infernal_boss_agent_policy_hash(boss_policy)?,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    Ok(readback)
}

pub fn validate_infernal_relic_vault_policy(
    policy: &InfernalRelicVaultPolicy,
    zone_policy: &InfernalZonePolicy,
    boss_policy: &InfernalBossAgentPolicy,
    attrition_policy: &InfernalAttritionPolicy,
) -> Result<(), EngineError> {
    validate_infernal_boss_agent_policy(boss_policy, zone_policy, attrition_policy)?;
    if policy.schema != RPG_INFERNAL_RELIC_VAULTS_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_INFERNAL_RELIC_VAULTS_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1069 {
        return invalid("policy.source_issue", "must point at issue 1069");
    }
    if policy.built_in_map_required {
        return invalid(
            "policy.built_in_map_required",
            "Infernal relics cannot grant or require built-in maps/minimaps",
        );
    }
    if !policy.world_finite_enforced {
        return invalid(
            "policy.world_finite_enforced",
            "hell-forged relic counts must be world-finite",
        );
    }
    validate_vaults(policy, zone_policy, boss_policy)?;
    validate_items(policy)?;
    validate_set_bonuses(policy)?;
    Ok(())
}

pub fn vault_for_kind(
    policy: &InfernalRelicVaultPolicy,
    kind: InfernalRelicVaultKind,
) -> Result<&InfernalRelicVault, EngineError> {
    policy
        .vaults
        .iter()
        .find(|vault| vault.kind == kind)
        .ok_or_else(|| invalid_error("policy.vaults.kind", "missing required vault"))
}

pub fn infernal_relic_vault_policy_hash(
    policy: &InfernalRelicVaultPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn validate_vaults(
    policy: &InfernalRelicVaultPolicy,
    zone_policy: &InfernalZonePolicy,
    boss_policy: &InfernalBossAgentPolicy,
) -> Result<(), EngineError> {
    if policy.vaults.len() != EXPECTED_VAULTS.len() {
        return invalid(
            "policy.vaults",
            "Molten Throne-road and Obsidian Throne vaults are required",
        );
    }
    let mut kinds = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for vault in &policy.vaults {
        unique(&mut kinds, vault.kind, "policy.vaults.kind")?;
        unique(&mut ids, vault.vault_id.as_str(), "policy.vaults.vault_id")?;
        validate_vault(vault, zone_policy, boss_policy)?;
    }
    for kind in EXPECTED_VAULTS {
        vault_for_kind(policy, kind)?;
    }
    Ok(())
}

fn validate_vault(
    vault: &InfernalRelicVault,
    zone_policy: &InfernalZonePolicy,
    boss_policy: &InfernalBossAgentPolicy,
) -> Result<(), EngineError> {
    validate_token("policy.vaults.vault_id", &vault.vault_id)?;
    validate_display("policy.vaults.display_name", &vault.display_name)?;
    validate_token("policy.vaults.zone_id", &vault.zone_id)?;
    validate_token(
        "policy.vaults.guarded_by_ruler_ref",
        &vault.guarded_by_ruler_ref,
    )?;
    validate_token(
        "policy.vaults.guarded_by_agent_id",
        &vault.guarded_by_agent_id,
    )?;
    validate_token("policy.vaults.access_lock_ref", &vault.access_lock_ref)?;
    if vault.world_finite_capacity == 0 || vault.world_finite_capacity > 12 {
        return invalid(
            "policy.vaults.world_finite_capacity",
            "must be inside 1..=12",
        );
    }
    if vault.respawns_world_finite_items {
        return invalid(
            "policy.vaults.respawns_world_finite_items",
            "world-finite relics cannot respawn from vault resets",
        );
    }
    let zone = zone_for_circle(zone_policy, vault.circle_id)?;
    if vault.zone_id != zone.zone_id {
        return invalid("policy.vaults.zone_id", "must match #1067 zone id");
    }
    if vault.kind == InfernalRelicVaultKind::MoltenThroneRoad && !zone.relic_vault_source {
        return invalid(
            "policy.vaults.kind",
            "Molten Throne-road must carry the #1067 relic vault source",
        );
    }
    if vault.kind == InfernalRelicVaultKind::ObsidianThrone
        && vault.circle_id != InfernalCircleId::ObsidianThrone
    {
        return invalid(
            "policy.vaults.circle_id",
            "Obsidian vault must be under the Obsidian Throne",
        );
    }
    let boss = boss_for_circle(boss_policy, vault.circle_id)?;
    if vault.guarded_by_ruler_ref != boss.ruler_ref
        || vault.guarded_by_agent_id != boss.calyx_agent_id
    {
        return invalid(
            "policy.vaults.guard",
            "vault guard must match the #1068 boss-agent for its circle",
        );
    }
    Ok(())
}

fn validate_items(policy: &InfernalRelicVaultPolicy) -> Result<(), EngineError> {
    if policy.item_set.len() != 6 {
        return invalid("policy.item_set", "the hell-forged set requires six relics");
    }
    let mut ids = BTreeSet::new();
    let mut slots = BTreeSet::new();
    let mut per_vault = BTreeMap::<String, u16>::new();
    let mut highest = 0;
    for item in &policy.item_set {
        validate_item(item, policy)?;
        unique(&mut ids, item.item_id.as_str(), "policy.item_set.item_id")?;
        unique(&mut slots, item.slot, "policy.item_set.slot")?;
        *per_vault.entry(item.source_vault_id.clone()).or_default() += item.world_finite_count;
        highest = highest.max(item.power_rating_milli);
    }
    if highest != 1000 {
        return invalid(
            "policy.item_set.power_rating_milli",
            "one top-tier relic must reach 1000 milli-power",
        );
    }
    for (vault_id, count) in per_vault {
        let vault = vault_for_id(policy, &vault_id)?;
        if count > vault.world_finite_capacity {
            return invalid(
                "policy.item_set.world_finite_count",
                "vault item counts exceed world-finite capacity",
            );
        }
    }
    Ok(())
}

fn validate_item(
    item: &InfernalRelicItem,
    policy: &InfernalRelicVaultPolicy,
) -> Result<(), EngineError> {
    validate_token("policy.item_set.item_id", &item.item_id)?;
    validate_display("policy.item_set.display_name", &item.display_name)?;
    if item.rarity != InfernalRelicRarity::HellforgedLegendary {
        return invalid(
            "policy.item_set.rarity",
            "all relics must be hell-forged legendary",
        );
    }
    let vault = vault_for_id(policy, &item.source_vault_id)?;
    if item.guarded_by_ruler_ref != vault.guarded_by_ruler_ref {
        return invalid(
            "policy.item_set.guarded_by_ruler_ref",
            "item guard must match source vault guard",
        );
    }
    if item.world_finite_count != 1 {
        return invalid(
            "policy.item_set.world_finite_count",
            "each top-tier relic must be unique in the world",
        );
    }
    if !(950..=1000).contains(&item.power_rating_milli) {
        return invalid(
            "policy.item_set.power_rating_milli",
            "hell-forged relics must be inside 950..=1000",
        );
    }
    if item.grants_map_or_minimap {
        return invalid(
            "policy.item_set.grants_map_or_minimap",
            "relics must not grant built-in map/minimap behavior",
        );
    }
    if item.bypasses_infernal_no_recall {
        return invalid(
            "policy.item_set.bypasses_infernal_no_recall",
            "relics must not bypass Infernal no-recall locks",
        );
    }
    validate_perks(item)
}

fn validate_perks(item: &InfernalRelicItem) -> Result<(), EngineError> {
    if item.perks.len() < 2 || item.perks.len() > 5 {
        return invalid("policy.item_set.perks", "each relic needs 2..=5 perks");
    }
    let mut ids = BTreeSet::new();
    for perk in &item.perks {
        validate_token("policy.item_set.perks.perk_id", &perk.perk_id)?;
        unique(
            &mut ids,
            perk.perk_id.as_str(),
            "policy.item_set.perks.perk_id",
        )?;
        if !(950..=1000).contains(&perk.power_rating_milli) {
            return invalid(
                "policy.item_set.perks.power_rating_milli",
                "perk power must be inside 950..=1000",
            );
        }
    }
    Ok(())
}

fn validate_set_bonuses(policy: &InfernalRelicVaultPolicy) -> Result<(), EngineError> {
    let required = [2_u8, 4, 6];
    if policy.set_bonuses.len() != required.len() {
        return invalid("policy.set_bonuses", "2/4/6-piece set bonuses are required");
    }
    for (bonus, required_pieces) in policy.set_bonuses.iter().zip(required) {
        validate_set_bonus(bonus, required_pieces)?;
    }
    Ok(())
}

fn validate_set_bonus(
    bonus: &InfernalRelicSetBonus,
    expected_pieces: u8,
) -> Result<(), EngineError> {
    validate_token("policy.set_bonuses.bonus_id", &bonus.bonus_id)?;
    validate_display("policy.set_bonuses.effect_summary", &bonus.effect_summary)?;
    if bonus.pieces_required != expected_pieces {
        return invalid(
            "policy.set_bonuses.pieces_required",
            "must be 2, then 4, then 6",
        );
    }
    if !(950..=1000).contains(&bonus.power_rating_milli) {
        return invalid(
            "policy.set_bonuses.power_rating_milli",
            "set bonus power must be inside 950..=1000",
        );
    }
    if bonus.grants_map_or_minimap || bonus.bypasses_infernal_no_recall {
        return invalid(
            "policy.set_bonuses",
            "set bonuses must not grant maps or bypass no-recall locks",
        );
    }
    Ok(())
}

pub(super) fn vault_for_id<'a>(
    policy: &'a InfernalRelicVaultPolicy,
    vault_id: &str,
) -> Result<&'a InfernalRelicVault, EngineError> {
    policy
        .vaults
        .iter()
        .find(|vault| vault.vault_id == vault_id)
        .ok_or_else(|| invalid_error("policy.vaults.vault_id", "missing source vault"))
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 180 {
        return invalid(field, "must be non-empty and <= 180 bytes");
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn unique<T: Ord + Copy>(seen: &mut BTreeSet<T>, value: T, field: &str) -> Result<(), EngineError> {
    if !seen.insert(value) {
        return invalid(field, "duplicate value");
    }
    Ok(())
}

fn readback_fingerprint(readback: &InfernalRelicVaultReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "server_policy_id": readback.server_policy_id,
        "source_issue": readback.source_issue,
        "built_in_map_required": readback.built_in_map_required,
        "world_finite_enforced": readback.world_finite_enforced,
        "vault_count": readback.vault_count,
        "item_count": readback.item_count,
        "set_bonus_count": readback.set_bonus_count,
        "world_finite_item_total": readback.world_finite_item_total,
        "highest_power_rating_milli": readback.highest_power_rating_milli,
        "item_rows": readback.item_rows,
        "policy_sha256": readback.policy_sha256,
        "zone_policy_sha256": readback.zone_policy_sha256,
        "boss_policy_sha256": readback.boss_policy_sha256,
    })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("infernal_relic_vaults.{}", field.into()),
        reason: reason.to_string(),
    }
}
