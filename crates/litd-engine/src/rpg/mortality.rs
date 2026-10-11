use std::collections::BTreeSet;

use super::serialize_sha256;

use crate::EngineError;

use super::{
    MortalityAgeBand, MortalityAgingReadback, MortalityCharacterState, MortalityDerivedStats,
    MortalityEvent, MortalityEventKind, MortalityEventReadback, MortalityPolicy, MourningWallEntry,
    RPG_MORTALITY_SCHEMA, RaceLifespan,
};

pub fn derive_mortality_stats(
    constitution: i16,
    policy: &MortalityPolicy,
) -> Result<MortalityDerivedStats, EngineError> {
    validate_mortality_policy(policy)?;
    if !(1..=100).contains(&constitution) {
        return invalid("constitution", "must be inside 1..=100");
    }
    let constitution = i32::from(constitution);
    let threshold = policy
        .death_threshold_base
        .checked_add(
            constitution
                .checked_mul(policy.death_threshold_per_constitution)
                .ok_or_else(|| invalid_error("death_threshold", "overflowed i32"))?,
        )
        .ok_or_else(|| invalid_error("death_threshold", "overflowed i32"))?;
    Ok(MortalityDerivedStats {
        constitution: i16::try_from(constitution).unwrap_or(100),
        constitution_hit_points: constitution * policy.constitution_hit_points,
        regen_per_tick: constitution * policy.constitution_regen_per_tick,
        death_threshold_hit_points: -threshold,
    })
}

pub fn evaluate_mortality_aging(
    state: &MortalityCharacterState,
    policy: &MortalityPolicy,
) -> Result<MortalityAgingReadback, EngineError> {
    validate_mortality_policy(policy)?;
    let lifespan = lifespan_for(policy, &state.race_id)?;
    let age_band = if state.age_years < lifespan.adulthood_years {
        MortalityAgeBand::Youth
    } else if state.age_years >= lifespan.maximum_lifespan_years {
        MortalityAgeBand::OldAgeDue
    } else if state.age_years >= lifespan.expected_lifespan_years {
        MortalityAgeBand::Elder
    } else {
        MortalityAgeBand::Adult
    };
    Ok(MortalityAgingReadback {
        race_id: state.race_id.clone(),
        age_years: state.age_years,
        adulthood_years: lifespan.adulthood_years,
        expected_lifespan_years: lifespan.expected_lifespan_years,
        maximum_lifespan_years: lifespan.maximum_lifespan_years,
        age_band,
        old_age_death_due: age_band == MortalityAgeBand::OldAgeDue,
    })
}

pub fn record_mortality_event(
    state: &MortalityCharacterState,
    policy: &MortalityPolicy,
    event: &MortalityEvent,
) -> Result<MortalityEventReadback, EngineError> {
    validate_mortality_state(state, policy)?;
    validate_mortality_event(event)?;
    if state.permanently_dead {
        return invalid(
            "state.permanently_dead",
            "cannot record death after permanent death",
        );
    }
    let before_state_sha256 = mortality_state_hash(state)?;
    let policy_sha256 = mortality_policy_hash(policy)?;
    let aging_before = evaluate_mortality_aging(state, policy)?;
    let derived_before = derive_mortality_stats(state.current_constitution, policy)?;
    let mut after = state.clone();
    after.total_deaths = after
        .total_deaths
        .checked_add(1)
        .ok_or_else(|| invalid_error("state.total_deaths", "overflowed u32"))?;
    let mut constitution_lost = false;
    let mut permanent_death_reason = None;
    match event.kind {
        MortalityEventKind::OldAge => {
            if !aging_before.old_age_death_due {
                return invalid(
                    "event.kind",
                    "old-age death requires age at or beyond race maximum lifespan",
                );
            }
            after.permanently_dead = true;
            permanent_death_reason = Some("old_age".to_string());
        }
        MortalityEventKind::CombatDeath
        | MortalityEventKind::ExposureDeath
        | MortalityEventKind::HollowAttrition
        | MortalityEventKind::RitualBacklash => {
            after.deaths_since_constitution_loss = after
                .deaths_since_constitution_loss
                .checked_add(1)
                .ok_or_else(|| invalid_error("state.deaths_since_loss", "overflowed u16"))?;
            if after.deaths_since_constitution_loss >= policy.deaths_per_constitution_loss {
                after.deaths_since_constitution_loss = 0;
                after.current_constitution = after
                    .current_constitution
                    .checked_sub(1)
                    .ok_or_else(|| invalid_error("state.current_constitution", "underflowed"))?;
                constitution_lost = true;
            }
            if after.current_constitution <= policy.permanent_death_constitution_threshold {
                after.permanently_dead = true;
                permanent_death_reason = Some("constitution_depleted".to_string());
            }
        }
    }
    let mourning_wall_entry = if after.permanently_dead {
        let entry = mourning_wall_entry(&after, policy, event);
        after.mourning_wall_entry_id = Some(entry.entry_id.clone());
        Some(entry)
    } else {
        None
    };
    validate_mortality_state(&after, policy)?;
    let derived_after = derive_mortality_stats(after.current_constitution, policy)?;
    let aging_after = evaluate_mortality_aging(&after, policy)?;
    let after_state_sha256 = mortality_state_hash(&after)?;
    let mut readback = MortalityEventReadback {
        schema: RPG_MORTALITY_SCHEMA.to_string(),
        event_id: event.event_id.clone(),
        character_id: state.character_id.clone(),
        race_id: state.race_id.clone(),
        event_kind: event.kind,
        constitution_before: state.current_constitution,
        constitution_after: after.current_constitution,
        deaths_since_loss_before: state.deaths_since_constitution_loss,
        deaths_since_loss_after: after.deaths_since_constitution_loss,
        total_deaths_before: state.total_deaths,
        total_deaths_after: after.total_deaths,
        constitution_lost_this_event: constitution_lost,
        permanently_dead: after.permanently_dead,
        permanent_death_reason,
        aging: aging_after,
        derived_before,
        derived_after,
        mourning_wall_entry,
        before_state_sha256,
        after_state_sha256,
        policy_sha256,
        result_sha256: String::new(),
        after_state: after,
    };
    readback.result_sha256 = serialize_sha256(&result_fingerprint_payload(&readback))?;
    Ok(readback)
}

pub fn validate_mortality_policy(policy: &MortalityPolicy) -> Result<(), EngineError> {
    if policy.schema != RPG_MORTALITY_SCHEMA {
        return invalid("policy.schema", format!("expected {RPG_MORTALITY_SCHEMA}"));
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1060 {
        return invalid("policy.source_issue", "must point at issue 1060");
    }
    if !(1..=30).contains(&policy.deaths_per_constitution_loss) {
        return invalid(
            "policy.deaths_per_constitution_loss",
            "must be inside 1..=30",
        );
    }
    if !(1..=20).contains(&policy.permanent_death_constitution_threshold) {
        return invalid(
            "policy.permanent_death_constitution_threshold",
            "must be inside 1..=20",
        );
    }
    if !(1..=1000).contains(&policy.constitution_hit_points) {
        return invalid("policy.constitution_hit_points", "must be inside 1..=1000");
    }
    if !(0..=1000).contains(&policy.constitution_regen_per_tick) {
        return invalid(
            "policy.constitution_regen_per_tick",
            "must be inside 0..=1000",
        );
    }
    if !(1..=10_000).contains(&policy.death_threshold_base)
        || !(0..=10_000).contains(&policy.death_threshold_per_constitution)
    {
        return invalid(
            "policy.death_threshold",
            "base/per-constitution must be bounded positive values",
        );
    }
    if !policy.mourning_wall_required {
        return invalid(
            "policy.mourning_wall_required",
            "Ashen Reach permanent death must emit memorial handoff",
        );
    }
    if policy.race_lifespans.is_empty() {
        return invalid(
            "policy.race_lifespans",
            "at least one race lifespan is required",
        );
    }
    let mut seen = BTreeSet::new();
    for lifespan in &policy.race_lifespans {
        validate_lifespan(lifespan)?;
        if !seen.insert(lifespan.race_id.as_str()) {
            return invalid(
                "policy.race_lifespans.race_id",
                format!("duplicate {}", lifespan.race_id),
            );
        }
    }
    Ok(())
}

pub fn validate_mortality_state(
    state: &MortalityCharacterState,
    policy: &MortalityPolicy,
) -> Result<(), EngineError> {
    validate_mortality_policy(policy)?;
    if state.schema != RPG_MORTALITY_SCHEMA {
        return invalid("state.schema", format!("expected {RPG_MORTALITY_SCHEMA}"));
    }
    validate_token("state.character_id", &state.character_id)?;
    validate_text("state.character_name", &state.character_name, 128)?;
    validate_token("state.race_id", &state.race_id)?;
    lifespan_for(policy, &state.race_id)?;
    if state.level == 0 || state.level > 100 {
        return invalid("state.level", "must be inside 1..=100");
    }
    validate_attribute_scores(state.base_attributes)?;
    if !(1..=state.base_attributes.constitution).contains(&state.current_constitution) {
        return invalid(
            "state.current_constitution",
            "must be inside 1..=base constitution",
        );
    }
    if state.deaths_since_constitution_loss >= policy.deaths_per_constitution_loss {
        return invalid(
            "state.deaths_since_constitution_loss",
            "must be lower than policy deaths_per_constitution_loss",
        );
    }
    if state.permanently_dead {
        if state.mourning_wall_entry_id.is_none() {
            return invalid(
                "state.mourning_wall_entry_id",
                "permanent death requires a Mourning Wall handoff id",
            );
        }
    } else if state.current_constitution <= policy.permanent_death_constitution_threshold {
        return invalid(
            "state.current_constitution",
            "living character cannot be at or below permanent-death CON threshold",
        );
    } else if state.mourning_wall_entry_id.is_some() {
        return invalid(
            "state.mourning_wall_entry_id",
            "living character cannot already have a Mourning Wall handoff id",
        );
    }
    Ok(())
}

pub fn mortality_state_hash(state: &MortalityCharacterState) -> Result<String, EngineError> {
    serialize_sha256(state)
}

pub fn mortality_policy_hash(policy: &MortalityPolicy) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn validate_mortality_event(event: &MortalityEvent) -> Result<(), EngineError> {
    validate_token("event.event_id", &event.event_id)?;
    if let Some(witness_id) = &event.witness_id {
        validate_token("event.witness_id", witness_id)?;
    }
    validate_text("event.epitaph", &event.epitaph, 240)
}

fn validate_lifespan(lifespan: &RaceLifespan) -> Result<(), EngineError> {
    validate_token("race_lifespans.race_id", &lifespan.race_id)?;
    validate_text("race_lifespans.display_name", &lifespan.display_name, 64)?;
    if lifespan.adulthood_years == 0 {
        return invalid("race_lifespans.adulthood_years", "must be non-zero");
    }
    if lifespan.adulthood_years > lifespan.expected_lifespan_years {
        return invalid(
            "race_lifespans.adulthood_years",
            "must be <= expected lifespan",
        );
    }
    if lifespan.expected_lifespan_years > lifespan.maximum_lifespan_years {
        return invalid(
            "race_lifespans.expected_lifespan_years",
            "must be <= maximum lifespan",
        );
    }
    if lifespan.maximum_lifespan_years > 10_000 {
        return invalid("race_lifespans.maximum_lifespan_years", "must be <= 10000");
    }
    Ok(())
}

fn validate_attribute_scores(scores: super::AttributeScores) -> Result<(), EngineError> {
    for (name, value) in scores.pairs() {
        if !(1..=100).contains(&value) {
            return invalid(
                format!("state.base_attributes.{name}"),
                "must be inside 1..=100",
            );
        }
    }
    Ok(())
}

fn lifespan_for<'a>(
    policy: &'a MortalityPolicy,
    race_id: &str,
) -> Result<&'a RaceLifespan, EngineError> {
    policy
        .race_lifespans
        .iter()
        .find(|lifespan| lifespan.race_id == race_id)
        .ok_or_else(|| {
            invalid_error(
                "state.race_id",
                format!("missing race lifespan row for {race_id}"),
            )
        })
}

fn mourning_wall_entry(
    state: &MortalityCharacterState,
    policy: &MortalityPolicy,
    event: &MortalityEvent,
) -> MourningWallEntry {
    MourningWallEntry {
        entry_id: format!(
            "mourning.wall.{}.{}",
            state.character_id, state.total_deaths
        ),
        carved_name: state.character_name.clone(),
        character_id: state.character_id.clone(),
        race_id: state.race_id.clone(),
        age_years: state.age_years,
        final_constitution: state.current_constitution,
        total_deaths: state.total_deaths,
        cause: event.kind,
        witness_id: event.witness_id.clone(),
        epitaph: event.epitaph.clone(),
        source_issue: policy.source_issue,
    }
}

fn result_fingerprint_payload(readback: &MortalityEventReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "event_id": readback.event_id,
        "character_id": readback.character_id,
        "race_id": readback.race_id,
        "event_kind": readback.event_kind,
        "constitution_after": readback.constitution_after,
        "total_deaths_after": readback.total_deaths_after,
        "permanently_dead": readback.permanently_dead,
        "permanent_death_reason": readback.permanent_death_reason,
        "mourning_wall_entry": readback.mourning_wall_entry,
        "before_state_sha256": readback.before_state_sha256,
        "after_state_sha256": readback.after_state_sha256,
        "policy_sha256": readback.policy_sha256,
    })
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

fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > max_len {
        return invalid(field, format!("must be <= {max_len} bytes"));
    }
    Ok(())
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

fn invalid_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("mortality.{}", field.into()),
        reason: reason.to_string(),
    }
}
