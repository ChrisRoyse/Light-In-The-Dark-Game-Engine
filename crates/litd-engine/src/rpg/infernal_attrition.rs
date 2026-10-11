use std::collections::BTreeSet;

use super::serialize_sha256;

use crate::EngineError;

use super::{
    InfernalAttritionDeathReadback, InfernalAttritionPolicy, InfernalAttritionReadback,
    InfernalAttritionState, InfernalAttritionStep, InfernalCircleId, InfernalCircleProfile,
    MortalityEvent, MortalityEventKind, MortalityPolicy, MourningWallEntry,
    RPG_INFERNAL_ATTRITION_SCHEMA, derive_mortality_stats, record_mortality_event,
    validate_mortality_state,
};

pub fn apply_infernal_attrition(
    state: &InfernalAttritionState,
    attrition_policy: &InfernalAttritionPolicy,
    mortality_policy: &MortalityPolicy,
    step: &InfernalAttritionStep,
) -> Result<InfernalAttritionReadback, EngineError> {
    validate_infernal_attrition_state(state, attrition_policy, mortality_policy)?;
    validate_infernal_attrition_step(step, attrition_policy)?;
    if state.circle_id != step.circle_id {
        return invalid(
            "step.circle_id",
            format!(
                "step {} does not match state {}",
                step.circle_id.label(),
                state.circle_id.label()
            ),
        );
    }
    let profile = circle_profile(attrition_policy, step.circle_id)?;
    let state_before_sha256 = infernal_attrition_state_hash(state)?;
    let policy_sha256 = infernal_attrition_policy_hash(attrition_policy)?;
    let health_damage = scaled_damage(
        profile.health_damage_per_tick,
        step.tick_count,
        step.mitigation_percent,
    )?;
    let strain_gained = scaled_damage(
        profile.constitution_strain_per_tick,
        step.tick_count,
        step.mitigation_percent,
    )?;
    let mut after = state.clone();
    after.hit_points = after
        .hit_points
        .checked_sub(health_damage)
        .ok_or_else(|| invalid_error("state.hit_points", "underflowed i32"))?;
    after.constitution_strain = after
        .constitution_strain
        .checked_add(strain_gained)
        .ok_or_else(|| invalid_error("state.constitution_strain", "overflowed i32"))?;
    after.total_exposure_ticks = after
        .total_exposure_ticks
        .checked_add(u32::from(step.tick_count))
        .ok_or_else(|| invalid_error("state.total_exposure_ticks", "overflowed u32"))?;
    let con_loss = after.constitution_strain / attrition_policy.constitution_strain_per_loss;
    after.constitution_strain %= attrition_policy.constitution_strain_per_loss;
    // Snapshot before applying strain loss so the readback reports the actually
    // applied constitution delta, not the raw quotient. The quotient is clamped
    // twice (`saturating_sub(..).max(1)` and the `i16::MAX` conversion cap), so
    // reporting it makes `constitution_before - constitution_after` disagree with
    // `constitution_lost_to_strain` and poisons the committed hash (#1484-3).
    let constitution_before_strain = after.mortality_state.current_constitution;
    if con_loss > 0 {
        after.mortality_state.current_constitution = after
            .mortality_state
            .current_constitution
            .saturating_sub(i16::try_from(con_loss).unwrap_or(i16::MAX))
            .max(1);
    }
    let constitution_lost_to_strain =
        constitution_before_strain - after.mortality_state.current_constitution;
    let derived_after_attrition =
        derive_mortality_stats(after.mortality_state.current_constitution, mortality_policy)?;
    let mut death = None;
    if after.mortality_state.current_constitution
        <= mortality_policy.permanent_death_constitution_threshold
    {
        death = Some(apply_constitution_drain_death(&mut after, step, profile)?);
    } else if after.hit_points <= derived_after_attrition.death_threshold_hit_points {
        let mortality_event = MortalityEvent::new(
            &format!("death.infernal.{}", step.step_id),
            MortalityEventKind::HollowAttrition,
            "The Infernal Deeps wore the body past its last threshold.",
        );
        let mortality_readback =
            record_mortality_event(&after.mortality_state, mortality_policy, &mortality_event)?;
        let mourning_wall_entry = mortality_readback.mourning_wall_entry.clone();
        after.mortality_state = mortality_readback.after_state.clone();
        death = Some(InfernalAttritionDeathReadback {
            permanent: mortality_readback.permanently_dead,
            reason: mortality_readback
                .permanent_death_reason
                .clone()
                .unwrap_or_else(|| "infernal_death_threshold".to_string()),
            mortality_event: Some(mortality_readback),
            mourning_wall_entry,
        });
    }
    validate_infernal_attrition_state(&after, attrition_policy, mortality_policy)?;
    let state_after_sha256 = infernal_attrition_state_hash(&after)?;
    let mut readback = InfernalAttritionReadback {
        schema: RPG_INFERNAL_ATTRITION_SCHEMA.to_string(),
        step_id: step.step_id.clone(),
        run_id: state.run_id.clone(),
        party_member_id: state.party_member_id.clone(),
        circle_id: step.circle_id,
        hazard: profile.hazard,
        tick_count: step.tick_count,
        mitigation_percent: step.mitigation_percent,
        health_before: state.hit_points,
        health_after: after.hit_points,
        constitution_before: state.mortality_state.current_constitution,
        constitution_after: after.mortality_state.current_constitution,
        strain_before: state.constitution_strain,
        strain_after: after.constitution_strain,
        strain_gained,
        constitution_lost_to_strain,
        death_threshold_hit_points: derived_after_attrition.death_threshold_hit_points,
        death,
        state_after: after,
        state_before_sha256,
        state_after_sha256,
        policy_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&result_fingerprint(&readback))?;
    Ok(readback)
}

pub fn validate_infernal_attrition_policy(
    policy: &InfernalAttritionPolicy,
) -> Result<(), EngineError> {
    if policy.schema != RPG_INFERNAL_ATTRITION_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_INFERNAL_ATTRITION_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1070 {
        return invalid("policy.source_issue", "must point at issue 1070");
    }
    if !(1..=10_000).contains(&policy.constitution_strain_per_loss) {
        return invalid(
            "policy.constitution_strain_per_loss",
            "must be inside 1..=10000",
        );
    }
    if policy.max_step_ticks == 0 || policy.max_step_ticks > 10_000 {
        return invalid("policy.max_step_ticks", "must be inside 1..=10000");
    }
    if policy.max_mitigation_percent > 95 {
        return invalid("policy.max_mitigation_percent", "must be <= 95");
    }
    if policy.circles.len() != 7 {
        return invalid(
            "policy.circles",
            "all seven Infernal Deeps circles are required",
        );
    }
    let mut seen = BTreeSet::new();
    for profile in &policy.circles {
        validate_circle_profile(profile)?;
        if !seen.insert(profile.circle_id) {
            return invalid(
                "policy.circles.circle_id",
                format!("duplicate {}", profile.circle_id.label()),
            );
        }
    }
    for required in [
        InfernalCircleId::Threshold,
        InfernalCircleId::EmberWastes,
        InfernalCircleId::IronBastion,
        InfernalCircleId::HungeringDark,
        InfernalCircleId::WailingTombs,
        InfernalCircleId::MoltenThroneRoad,
        InfernalCircleId::ObsidianThrone,
    ] {
        if !seen.contains(&required) {
            return invalid(
                "policy.circles.circle_id",
                format!("missing {}", required.label()),
            );
        }
    }
    Ok(())
}

pub fn validate_infernal_attrition_state(
    state: &InfernalAttritionState,
    attrition_policy: &InfernalAttritionPolicy,
    mortality_policy: &MortalityPolicy,
) -> Result<(), EngineError> {
    validate_infernal_attrition_policy(attrition_policy)?;
    validate_mortality_state(&state.mortality_state, mortality_policy)?;
    if state.schema != RPG_INFERNAL_ATTRITION_SCHEMA {
        return invalid(
            "state.schema",
            format!("expected {RPG_INFERNAL_ATTRITION_SCHEMA}"),
        );
    }
    validate_token("state.run_id", &state.run_id)?;
    validate_token("state.party_member_id", &state.party_member_id)?;
    circle_profile(attrition_policy, state.circle_id)?;
    if state.max_hit_points <= 0 || state.max_hit_points > 100_000 {
        return invalid("state.max_hit_points", "must be inside 1..=100000");
    }
    if !(-100_000..=state.max_hit_points).contains(&state.hit_points) {
        return invalid(
            "state.hit_points",
            "must be inside -100000..=max_hit_points",
        );
    }
    if !(0..attrition_policy.constitution_strain_per_loss).contains(&state.constitution_strain) {
        return invalid(
            "state.constitution_strain",
            "must be below policy constitution_strain_per_loss",
        );
    }
    Ok(())
}

pub fn validate_infernal_attrition_step(
    step: &InfernalAttritionStep,
    policy: &InfernalAttritionPolicy,
) -> Result<(), EngineError> {
    validate_infernal_attrition_policy(policy)?;
    validate_token("step.step_id", &step.step_id)?;
    circle_profile(policy, step.circle_id)?;
    if step.tick_count == 0 || step.tick_count > policy.max_step_ticks {
        return invalid(
            "step.tick_count",
            format!("must be inside 1..={}", policy.max_step_ticks),
        );
    }
    if step.mitigation_percent > policy.max_mitigation_percent {
        return invalid(
            "step.mitigation_percent",
            format!("must be <= {}", policy.max_mitigation_percent),
        );
    }
    Ok(())
}

pub fn infernal_attrition_state_hash(
    state: &InfernalAttritionState,
) -> Result<String, EngineError> {
    serialize_sha256(state)
}

pub fn infernal_attrition_policy_hash(
    policy: &InfernalAttritionPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn apply_constitution_drain_death(
    state: &mut InfernalAttritionState,
    step: &InfernalAttritionStep,
    profile: &InfernalCircleProfile,
) -> Result<InfernalAttritionDeathReadback, EngineError> {
    state.mortality_state.total_deaths = state
        .mortality_state
        .total_deaths
        .checked_add(1)
        .ok_or_else(|| invalid_error("state.total_deaths", "overflowed u32"))?;
    state.mortality_state.permanently_dead = true;
    let entry = MourningWallEntry {
        entry_id: format!(
            "mourning.wall.{}.{}",
            state.mortality_state.character_id, state.mortality_state.total_deaths
        ),
        carved_name: state.mortality_state.character_name.clone(),
        character_id: state.mortality_state.character_id.clone(),
        race_id: state.mortality_state.race_id.clone(),
        age_years: state.mortality_state.age_years,
        final_constitution: state.mortality_state.current_constitution,
        total_deaths: state.mortality_state.total_deaths,
        cause: MortalityEventKind::HollowAttrition,
        witness_id: None,
        epitaph: format!(
            "{} exposure in {} consumed the last constitution reserve.",
            hazard_label(profile),
            step.circle_id.label()
        ),
        source_issue: 1070,
    };
    state.mortality_state.mourning_wall_entry_id = Some(entry.entry_id.clone());
    Ok(InfernalAttritionDeathReadback {
        permanent: true,
        reason: "infernal_constitution_drained".to_string(),
        mortality_event: None,
        mourning_wall_entry: Some(entry),
    })
}

fn scaled_damage(per_tick: i32, ticks: u16, mitigation_percent: u8) -> Result<i32, EngineError> {
    let raw = per_tick
        .checked_mul(i32::from(ticks))
        .ok_or_else(|| invalid_error("step.tick_count", "damage overflowed i32"))?;
    let remaining_percent = 100_i32 - i32::from(mitigation_percent);
    raw.checked_mul(remaining_percent)
        .and_then(|value| value.checked_add(99))
        .map(|value| value / 100)
        .ok_or_else(|| invalid_error("step.mitigation_percent", "damage overflowed i32"))
}

fn circle_profile(
    policy: &InfernalAttritionPolicy,
    circle_id: InfernalCircleId,
) -> Result<&InfernalCircleProfile, EngineError> {
    policy
        .circles
        .iter()
        .find(|profile| profile.circle_id == circle_id)
        .ok_or_else(|| invalid_error("circle_id", format!("missing {}", circle_id.label())))
}

fn validate_circle_profile(profile: &InfernalCircleProfile) -> Result<(), EngineError> {
    if !(0..=10_000).contains(&profile.health_damage_per_tick)
        || !(0..=10_000).contains(&profile.constitution_strain_per_tick)
    {
        return invalid(
            "policy.circles.damage",
            "health damage and constitution strain must be inside 0..=10000",
        );
    }
    if profile.health_damage_per_tick == 0 && profile.constitution_strain_per_tick == 0 {
        return invalid("policy.circles.damage", "circle hazard cannot be inert");
    }
    Ok(())
}

fn result_fingerprint(readback: &InfernalAttritionReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "step_id": readback.step_id,
        "run_id": readback.run_id,
        "party_member_id": readback.party_member_id,
        "circle_id": readback.circle_id,
        "hazard": readback.hazard,
        "health_after": readback.health_after,
        "constitution_after": readback.constitution_after,
        "strain_after": readback.strain_after,
        "constitution_lost_to_strain": readback.constitution_lost_to_strain,
        "death": readback.death,
        "state_before_sha256": readback.state_before_sha256,
        "state_after_sha256": readback.state_after_sha256,
        "policy_sha256": readback.policy_sha256,
    })
}

fn hazard_label(profile: &InfernalCircleProfile) -> &'static str {
    match profile.hazard {
        super::InfernalHazardKind::Heat => "heat",
        super::InfernalHazardKind::DarkPressure => "dark-pressure",
        super::InfernalHazardKind::SoulDrain => "soul-drain",
    }
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

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("infernal_attrition.{}", field.into()),
        reason: reason.to_string(),
    }
}
