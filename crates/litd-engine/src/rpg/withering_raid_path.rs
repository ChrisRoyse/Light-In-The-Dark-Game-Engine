use std::collections::BTreeSet;

use super::serialize_sha256;
use serde_json::json;

use crate::EngineError;

use super::{
    InfernalAttritionPolicy, InfernalBossAgentPolicy, InfernalCircleId, InfernalZonePolicy,
    RPG_WITHERING_RAID_PATH_SCHEMA, UndeadForm, UndeathPolicy, WitheringRaidPathPolicy,
    WitheringRaidPathReadback, WitheringRaidPhase, WitheringRaidPhaseKind, WitheringRequest,
    boss_for_circle, infernal_boss_agent_policy_hash, infernal_zone_policy_hash,
    undeath_policy_hash, validate_infernal_boss_agent_policy, validate_undeath_policy,
    validate_withering_request, zone_for_circle,
};

const EXPECTED_PHASES: [WitheringRaidPhaseKind; 6] = [
    WitheringRaidPhaseKind::CipherTrail,
    WitheringRaidPhaseKind::OuterGuardian,
    WitheringRaidPhaseKind::InnerGuardian,
    WitheringRaidPhaseKind::ItemOfPowerSeizure,
    WitheringRaidPhaseKind::WailingTombsHarvest,
    WitheringRaidPhaseKind::WitheringRite,
];

pub fn derive_withering_raid_path_readback(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    attrition: &InfernalAttritionPolicy,
) -> Result<WitheringRaidPathReadback, EngineError> {
    validate_withering_raid_path_policy(policy, undeath, zones, bosses, attrition)?;
    let request = withering_request_from_raid_path(policy, undeath, UndeadForm::Lich)?;
    let source_zone = zone_for_circle(zones, policy.raid.source_circle_id)?;
    let mut readback = WitheringRaidPathReadback {
        schema: RPG_WITHERING_RAID_PATH_SCHEMA.to_string(),
        server_policy_id: policy.server_policy_id.clone(),
        source_issue: policy.source_issue,
        built_in_map_required: policy.built_in_map_required,
        raid_id: policy.raid.raid_id.clone(),
        questline_id: policy.questline.questline_id.clone(),
        elite_group_required: policy.raid.elite_group_required,
        minimum_party_size: policy.raid.minimum_party_size,
        minimum_character_level: policy.raid.minimum_character_level,
        pressure_tier_milli: policy.raid.pressure_tier_milli,
        phase_count: policy.raid.phases.len(),
        required_discovery_ref_count: policy.questline.required_discovery_refs.len(),
        required_component_count: policy.questline.required_components.len(),
        allowed_form_count: policy.questline.allowed_forms.len(),
        source_zone_id: policy.raid.source_zone_id.clone(),
        source_zone_has_reagents: source_zone.withering_reagent_source,
        final_guard_agent_id: policy.raid.final_guard_agent_id.clone(),
        target_faction_id: policy.raid.target_faction_id.clone(),
        target_item_of_power_id: policy.raid.target_item_of_power_id.clone(),
        sample_withering_request_valid: validate_withering_request(&request, undeath).is_ok(),
        policy_sha256: withering_raid_path_policy_hash(policy)?,
        undeath_policy_sha256: undeath_policy_hash(undeath)?,
        zone_policy_sha256: infernal_zone_policy_hash(zones)?,
        boss_policy_sha256: infernal_boss_agent_policy_hash(bosses)?,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    Ok(readback)
}

pub fn withering_request_from_raid_path(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
    form: UndeadForm,
) -> Result<WitheringRequest, EngineError> {
    validate_undeath_policy(undeath)?;
    validate_core(policy, undeath)?;
    request_from_validated_path(policy, undeath, form)
}

fn request_from_validated_path(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
    form: UndeadForm,
) -> Result<WitheringRequest, EngineError> {
    if !policy.questline.allowed_forms.contains(&form) {
        return invalid("questline.allowed_forms", "requested form is not unlocked");
    }
    let focus = undeath
        .forms
        .iter()
        .find(|spec| spec.form == form)
        .map(|spec| spec.required_focus_id.clone())
        .ok_or_else(|| invalid_error("undeath.forms", "requested form is missing"))?;
    Ok(WitheringRequest {
        ritual_id: "ritual.withering.no_name_vigil".to_string(),
        chosen_form: form,
        caster_class_id: "class.necromancer".to_string(),
        caster_level: policy.raid.minimum_character_level,
        discovery_refs: policy.questline.required_discovery_refs.clone(),
        ritual_components: policy.questline.required_components.clone(),
        prepared_focus_id: focus,
        accepted_irreversible_cost: true,
    })
}

pub fn validate_withering_raid_path_policy(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
    zones: &InfernalZonePolicy,
    bosses: &InfernalBossAgentPolicy,
    attrition: &InfernalAttritionPolicy,
) -> Result<(), EngineError> {
    validate_undeath_policy(undeath)?;
    validate_infernal_boss_agent_policy(bosses, zones, attrition)?;
    validate_core(policy, undeath)?;
    let zone = zone_for_circle(zones, policy.raid.source_circle_id)?;
    if policy.raid.source_circle_id != InfernalCircleId::WailingTombs
        || policy.raid.source_zone_id != zone.zone_id
        || !zone.withering_reagent_source
    {
        return invalid(
            "raid.source_zone_id",
            "Withering path must harvest rare reagents from #1067 Wailing Tombs",
        );
    }
    let boss = boss_for_circle(bosses, InfernalCircleId::WailingTombs)?;
    if policy.raid.final_guard_ruler_ref != boss.ruler_ref
        || policy.raid.final_guard_agent_id != boss.calyx_agent_id
    {
        return invalid(
            "raid.final_guard",
            "final raid guard must match the #1068 Wailing Tombs Choirmaster agent",
        );
    }
    Ok(())
}

pub fn withering_raid_path_policy_hash(
    policy: &WitheringRaidPathPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn validate_core(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Result<(), EngineError> {
    if policy.schema != RPG_WITHERING_RAID_PATH_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_WITHERING_RAID_PATH_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1064 {
        return invalid("policy.source_issue", "must point at issue 1064");
    }
    if policy.built_in_map_required {
        return invalid(
            "policy.built_in_map_required",
            "Withering raid path cannot require built-in maps/minimaps",
        );
    }
    validate_raid(policy, undeath)?;
    validate_questline(policy, undeath)?;
    Ok(())
}

fn validate_raid(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Result<(), EngineError> {
    let raid = &policy.raid;
    validate_token("raid.raid_id", &raid.raid_id)?;
    validate_display("raid.display_name", &raid.display_name)?;
    for (field, value) in [
        ("raid.attacking_faction_id", &raid.attacking_faction_id),
        ("raid.target_faction_id", &raid.target_faction_id),
        (
            "raid.target_item_of_power_id",
            &raid.target_item_of_power_id,
        ),
        ("raid.source_zone_id", &raid.source_zone_id),
        ("raid.final_guard_ruler_ref", &raid.final_guard_ruler_ref),
        ("raid.final_guard_agent_id", &raid.final_guard_agent_id),
        ("raid.outer_guardian_ref", &raid.outer_guardian_ref),
        ("raid.inner_guardian_ref", &raid.inner_guardian_ref),
    ] {
        validate_token(field, value)?;
    }
    if raid.attacking_faction_id == raid.target_faction_id {
        return invalid("raid.faction", "faction raid must target an enemy faction");
    }
    if raid.target_faction_id != "organization.hollow_pact"
        || raid.target_item_of_power_id != "item.hollow_pact.rift_shard"
    {
        return invalid(
            "raid.target_item_of_power_id",
            "hardest Withering raid must target the Hollow Pact Rift-Shard",
        );
    }
    if !raid.elite_group_required || raid.minimum_party_size < 8 {
        return invalid(
            "raid.minimum_party_size",
            "elite group of at least 8 required",
        );
    }
    if raid.minimum_character_level < undeath.minimum_caster_level {
        return invalid(
            "raid.minimum_character_level",
            "cannot be below Withering caster requirement",
        );
    }
    if !raid.faction_leader_order_required || raid.pressure_tier_milli != 1000 {
        return invalid(
            "raid.pressure_tier_milli",
            "raid must be leader-ordered at hardest pressure tier",
        );
    }
    validate_phases(policy)
}

fn validate_phases(policy: &WitheringRaidPathPolicy) -> Result<(), EngineError> {
    if policy.raid.phases.len() != EXPECTED_PHASES.len() {
        return invalid("raid.phases", "six ordered raid/quest phases are required");
    }
    let mut ids = BTreeSet::new();
    let mut unlocks = BTreeSet::new();
    for (phase, expected) in policy.raid.phases.iter().zip(EXPECTED_PHASES) {
        validate_phase(phase, expected)?;
        unique(&mut ids, phase.phase_id.as_str(), "raid.phases.phase_id")?;
        unique(
            &mut unlocks,
            phase.unlock_ref.as_str(),
            "raid.phases.unlock_ref",
        )?;
    }
    Ok(())
}

fn validate_phase(
    phase: &WitheringRaidPhase,
    expected: WitheringRaidPhaseKind,
) -> Result<(), EngineError> {
    if phase.kind != expected {
        return invalid("raid.phases.kind", "phases must stay in canonical order");
    }
    validate_token("raid.phases.phase_id", &phase.phase_id)?;
    validate_display("raid.phases.display_name", &phase.display_name)?;
    validate_token("raid.phases.required_ref", &phase.required_ref)?;
    validate_token("raid.phases.unlock_ref", &phase.unlock_ref)?;
    if phase.pressure_tier_milli < 900 || phase.pressure_tier_milli > 1000 {
        return invalid(
            "raid.phases.pressure_tier_milli",
            "secret raid phases must stay elite tier",
        );
    }
    if !phase.in_character_only
        || phase.grants_map_or_minimap
        || phase.grants_gps_markers
        || phase.bypasses_withering_requirements
    {
        return invalid(
            "raid.phases.flags",
            "phases must be in-character and cannot grant map/GPS or bypass Withering gates",
        );
    }
    Ok(())
}

fn validate_questline(
    policy: &WitheringRaidPathPolicy,
    undeath: &UndeathPolicy,
) -> Result<(), EngineError> {
    let quest = &policy.questline;
    validate_token("questline.questline_id", &quest.questline_id)?;
    validate_display("questline.display_name", &quest.display_name)?;
    if !quest.secret_in_character || quest.revealed_by_default_ui || quest.quest_log_autocomplete {
        return invalid(
            "questline.flags",
            "Withering path must be secret, in-character, and not UI-autocompleted",
        );
    }
    validate_token_vec(
        "questline.required_discovery_refs",
        &quest.required_discovery_refs,
        8,
    )?;
    validate_token_vec(
        "questline.required_components",
        &quest.required_components,
        8,
    )?;
    validate_token_vec("questline.required_focus_ids", &quest.required_focus_ids, 8)?;
    if quest.required_discovery_refs.len() < undeath.required_discovery_ref_count {
        return invalid(
            "questline.required_discovery_refs",
            "must satisfy the #1061 discovery gate",
        );
    }
    let unlocks = policy
        .raid
        .phases
        .iter()
        .map(|phase| phase.unlock_ref.as_str())
        .collect::<BTreeSet<_>>();
    for required in &quest.required_discovery_refs {
        if !unlocks.contains(required.as_str()) {
            return invalid(
                "questline.required_discovery_refs",
                "ref is not earned by phases",
            );
        }
    }
    set_contains_all(
        "questline.required_components",
        &quest.required_components,
        &undeath.required_components,
    )?;
    let expected_focus = undeath
        .forms
        .iter()
        .map(|form| form.required_focus_id.clone())
        .collect::<Vec<_>>();
    set_contains_all(
        "questline.required_focus_ids",
        &quest.required_focus_ids,
        &expected_focus,
    )?;
    let forms = quest.allowed_forms.iter().copied().collect::<BTreeSet<_>>();
    let expected_forms = [UndeadForm::Lich, UndeadForm::Mummy, UndeadForm::Wraith]
        .into_iter()
        .collect::<BTreeSet<_>>();
    if forms != expected_forms {
        return invalid(
            "questline.allowed_forms",
            "lich, mummy, and wraith must unlock",
        );
    }
    for form in quest.allowed_forms.iter().copied() {
        validate_withering_request(
            &request_from_validated_path(policy, undeath, form)?,
            undeath,
        )?;
    }
    Ok(())
}

fn set_contains_all(
    field: &str,
    actual: &[String],
    required: &[String],
) -> Result<(), EngineError> {
    let actual = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    for value in required {
        if !actual.contains(value.as_str()) {
            return invalid(field, format!("missing {value}"));
        }
    }
    Ok(())
}

fn validate_token_vec(field: &str, values: &[String], max_len: usize) -> Result<(), EngineError> {
    if values.is_empty() || values.len() > max_len {
        return invalid(field, format!("must contain 1..={max_len} values"));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_token(field, value)?;
        unique(&mut seen, value.as_str(), field)?;
    }
    Ok(())
}

fn unique<T: Ord + Copy>(seen: &mut BTreeSet<T>, value: T, field: &str) -> Result<(), EngineError> {
    if !seen.insert(value) {
        return invalid(field, "duplicate value");
    }
    Ok(())
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 128 {
        return invalid(field, "must be non-empty and <= 128 bytes");
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

fn readback_fingerprint(readback: &WitheringRaidPathReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "server_policy_id": readback.server_policy_id,
        "source_issue": readback.source_issue,
        "built_in_map_required": readback.built_in_map_required,
        "raid_id": readback.raid_id,
        "questline_id": readback.questline_id,
        "minimum_party_size": readback.minimum_party_size,
        "minimum_character_level": readback.minimum_character_level,
        "phase_count": readback.phase_count,
        "required_discovery_ref_count": readback.required_discovery_ref_count,
        "required_component_count": readback.required_component_count,
        "source_zone_has_reagents": readback.source_zone_has_reagents,
        "final_guard_agent_id": readback.final_guard_agent_id,
        "sample_withering_request_valid": readback.sample_withering_request_valid,
        "policy_sha256": readback.policy_sha256,
        "undeath_policy_sha256": readback.undeath_policy_sha256,
        "zone_policy_sha256": readback.zone_policy_sha256,
        "boss_policy_sha256": readback.boss_policy_sha256,
    })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("withering_raid_path.{}", field.into()),
        reason: reason.to_string(),
    }
}
