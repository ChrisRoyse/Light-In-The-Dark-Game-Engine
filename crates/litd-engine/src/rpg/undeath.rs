use std::collections::BTreeSet;

use super::serialize_sha256;

use crate::EngineError;

use super::{
    MortalityCharacterState, MortalityEvent, MortalityEventKind, MortalityPolicy,
    RPG_UNDEATH_SCHEMA, UndeadAgingReadback, UndeadCharacterState, UndeadForm, UndeadFormSpec,
    UndeadMortalityReadback, UndeathPolicy, WitheringReadback, WitheringRequest,
    evaluate_mortality_aging, mortality_state_hash, record_mortality_event,
    validate_mortality_state,
};

pub fn apply_withering(
    state: &MortalityCharacterState,
    mortality_policy: &MortalityPolicy,
    undeath_policy: &UndeathPolicy,
    request: &WitheringRequest,
) -> Result<WitheringReadback, EngineError> {
    validate_mortality_state(state, mortality_policy)?;
    validate_undeath_policy(undeath_policy)?;
    validate_withering_request(request, undeath_policy)?;
    if state.permanently_dead {
        return invalid(
            "state.permanently_dead",
            "the Withering cannot resurrect the dead",
        );
    }
    let form = form_spec(undeath_policy, request.chosen_form)?;
    if request.prepared_focus_id != form.required_focus_id {
        return invalid(
            "request.prepared_focus_id",
            format!("expected {}", form.required_focus_id),
        );
    }
    let before_mortality_sha256 = mortality_state_hash(state)?;
    let mut mortality_state = state.clone();
    mortality_state.current_constitution = undeath_policy.fixed_constitution_after_withering;
    mortality_state.deaths_since_constitution_loss = 0;
    mortality_state.mourning_wall_entry_id = None;
    mortality_state.permanently_dead = false;
    validate_mortality_state(&mortality_state, mortality_policy)?;
    let undead_state = UndeadCharacterState {
        schema: RPG_UNDEATH_SCHEMA.to_string(),
        character_id: state.character_id.clone(),
        character_name: state.character_name.clone(),
        original_race_id: state.race_id.clone(),
        form: request.chosen_form,
        transformed_at_age_years: state.age_years,
        mortality_state,
        focus_id: request.prepared_focus_id.clone(),
        unlocked_rituals: vec![form.ritual.clone()],
        aging_death_immune: undeath_policy.aging_death_immune,
        hunger_thirst_immune: undeath_policy.hunger_thirst_immune,
        physical_kill_only: undeath_policy.physical_kill_only,
        irreversible: undeath_policy.irreversible,
        normal_resurrection_forbidden: undeath_policy.normal_resurrection_forbidden,
        source_issue: undeath_policy.source_issue,
    };
    validate_undead_state(&undead_state, mortality_policy, undeath_policy)?;
    let undead_state_sha256 = undeath_state_hash(&undead_state)?;
    let policy_sha256 = undeath_policy_hash(undeath_policy)?;
    let mut readback = WitheringReadback {
        schema: RPG_UNDEATH_SCHEMA.to_string(),
        ritual_id: request.ritual_id.clone(),
        character_id: state.character_id.clone(),
        chosen_form: request.chosen_form,
        caster_class_id: request.caster_class_id.clone(),
        caster_level: request.caster_level,
        constitution_before: state.current_constitution,
        constitution_after: undead_state.mortality_state.current_constitution,
        discovery_ref_count: request.discovery_refs.len(),
        component_count: request.ritual_components.len(),
        focus_id: request.prepared_focus_id.clone(),
        unlocked_rituals: undead_state.unlocked_rituals.clone(),
        undead_state,
        before_mortality_sha256,
        undead_state_sha256,
        policy_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&withering_fingerprint(&readback))?;
    Ok(readback)
}

pub fn evaluate_undead_aging(
    state: &UndeadCharacterState,
    mortality_policy: &MortalityPolicy,
    undeath_policy: &UndeathPolicy,
) -> Result<UndeadAgingReadback, EngineError> {
    validate_undead_state(state, mortality_policy, undeath_policy)?;
    let mortal_aging = evaluate_mortality_aging(&state.mortality_state, mortality_policy)?;
    Ok(UndeadAgingReadback {
        old_age_death_suppressed: state.aging_death_immune && mortal_aging.old_age_death_due,
        aging_death_immune: state.aging_death_immune,
        mortal_aging,
    })
}

pub fn record_undead_physical_death(
    state: &UndeadCharacterState,
    mortality_policy: &MortalityPolicy,
    undeath_policy: &UndeathPolicy,
    event: &MortalityEvent,
) -> Result<UndeadMortalityReadback, EngineError> {
    validate_undead_state(state, mortality_policy, undeath_policy)?;
    if event.kind != MortalityEventKind::CombatDeath {
        return invalid(
            "event.kind",
            "undead can lose constitution only through physical combat kill events",
        );
    }
    let undead_before_sha256 = undeath_state_hash(state)?;
    let aging = evaluate_undead_aging(state, mortality_policy, undeath_policy)?;
    let mortality = record_mortality_event(&state.mortality_state, mortality_policy, event)?;
    let mut undead_state_after = state.clone();
    undead_state_after.mortality_state = mortality.after_state.clone();
    validate_undead_state(&undead_state_after, mortality_policy, undeath_policy)?;
    let undead_after_sha256 = undeath_state_hash(&undead_state_after)?;
    let mut readback = UndeadMortalityReadback {
        schema: RPG_UNDEATH_SCHEMA.to_string(),
        undead_character_id: state.character_id.clone(),
        form: state.form,
        aging,
        physical_kill_only: state.physical_kill_only,
        mortality,
        undead_state_after,
        undead_before_sha256,
        undead_after_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&undead_mortality_fingerprint(&readback))?;
    Ok(readback)
}

pub fn validate_undeath_policy(policy: &UndeathPolicy) -> Result<(), EngineError> {
    if policy.schema != RPG_UNDEATH_SCHEMA {
        return invalid("policy.schema", format!("expected {RPG_UNDEATH_SCHEMA}"));
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1061 {
        return invalid("policy.source_issue", "must point at issue 1061");
    }
    if !(1..=100).contains(&policy.minimum_caster_level) {
        return invalid("policy.minimum_caster_level", "must be inside 1..=100");
    }
    if !(4..=10).contains(&policy.fixed_constitution_after_withering) {
        return invalid(
            "policy.fixed_constitution_after_withering",
            "must be a low survivable CON inside 4..=10",
        );
    }
    if !policy.aging_death_immune
        || !policy.hunger_thirst_immune
        || !policy.physical_kill_only
        || !policy.irreversible
        || !policy.normal_resurrection_forbidden
    {
        return invalid(
            "policy.flags",
            "Withering must be aging-immune, sustenance-free, physical-kill-only, irreversible, and no-resurrection",
        );
    }
    validate_token_vec(
        "policy.allowed_dark_caster_class_ids",
        &policy.allowed_dark_caster_class_ids,
        16,
    )?;
    if policy.required_discovery_ref_count == 0 || policy.required_discovery_ref_count > 8 {
        return invalid(
            "policy.required_discovery_ref_count",
            "must be inside 1..=8",
        );
    }
    validate_token_vec(
        "policy.required_components",
        &policy.required_components,
        16,
    )?;
    if policy.forms.len() != 3 {
        return invalid("policy.forms", "lich, mummy, and wraith forms are required");
    }
    let mut forms = BTreeSet::new();
    for form in &policy.forms {
        validate_form_spec(form)?;
        if !forms.insert(form.form) {
            return invalid(
                "policy.forms.form",
                format!("duplicate {}", form.form.label()),
            );
        }
    }
    for required in [UndeadForm::Lich, UndeadForm::Mummy, UndeadForm::Wraith] {
        if !forms.contains(&required) {
            return invalid("policy.forms.form", format!("missing {}", required.label()));
        }
    }
    Ok(())
}

pub fn validate_withering_request(
    request: &WitheringRequest,
    policy: &UndeathPolicy,
) -> Result<(), EngineError> {
    validate_undeath_policy(policy)?;
    validate_token("request.ritual_id", &request.ritual_id)?;
    validate_token("request.caster_class_id", &request.caster_class_id)?;
    if !policy
        .allowed_dark_caster_class_ids
        .contains(&request.caster_class_id)
    {
        return invalid(
            "request.caster_class_id",
            format!(
                "{} is not an allowed dark-caster class",
                request.caster_class_id
            ),
        );
    }
    if request.caster_level < policy.minimum_caster_level {
        return invalid(
            "request.caster_level",
            format!("must be >= {}", policy.minimum_caster_level),
        );
    }
    validate_token_vec("request.discovery_refs", &request.discovery_refs, 32)?;
    if request.discovery_refs.len() < policy.required_discovery_ref_count {
        return invalid(
            "request.discovery_refs",
            format!(
                "requires at least {} refs",
                policy.required_discovery_ref_count
            ),
        );
    }
    validate_token_vec("request.ritual_components", &request.ritual_components, 32)?;
    let supplied = request
        .ritual_components
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for component in &policy.required_components {
        if !supplied.contains(component.as_str()) {
            return invalid(
                "request.ritual_components",
                format!("missing required component {component}"),
            );
        }
    }
    validate_token("request.prepared_focus_id", &request.prepared_focus_id)?;
    if !request.accepted_irreversible_cost {
        return invalid(
            "request.accepted_irreversible_cost",
            "the Withering requires explicit irreversible-cost acceptance",
        );
    }
    Ok(())
}

pub fn validate_undead_state(
    state: &UndeadCharacterState,
    mortality_policy: &MortalityPolicy,
    undeath_policy: &UndeathPolicy,
) -> Result<(), EngineError> {
    validate_undeath_policy(undeath_policy)?;
    validate_mortality_state(&state.mortality_state, mortality_policy)?;
    if state.schema != RPG_UNDEATH_SCHEMA {
        return invalid("state.schema", format!("expected {RPG_UNDEATH_SCHEMA}"));
    }
    validate_token("state.character_id", &state.character_id)?;
    validate_text("state.character_name", &state.character_name, 128)?;
    validate_token("state.original_race_id", &state.original_race_id)?;
    if state.character_id != state.mortality_state.character_id
        || state.character_name != state.mortality_state.character_name
        || state.original_race_id != state.mortality_state.race_id
    {
        return invalid(
            "state.mortality_state",
            "undead wrapper must match mortality character id, name, and race",
        );
    }
    let form = form_spec(undeath_policy, state.form)?;
    if state.focus_id != form.required_focus_id {
        return invalid(
            "state.focus_id",
            format!("expected {}", form.required_focus_id),
        );
    }
    if state.mortality_state.current_constitution
        > undeath_policy.fixed_constitution_after_withering
    {
        return invalid(
            "state.mortality_state.current_constitution",
            "undead CON cannot exceed the Withering reset value",
        );
    }
    if state.unlocked_rituals != vec![form.ritual.clone()] {
        return invalid(
            "state.unlocked_rituals",
            "must contain exactly the form-specific undead ritual",
        );
    }
    if state.transformed_at_age_years > state.mortality_state.age_years {
        return invalid(
            "state.transformed_at_age_years",
            "cannot be after current chronological age",
        );
    }
    if !state.aging_death_immune
        || !state.hunger_thirst_immune
        || !state.physical_kill_only
        || !state.irreversible
        || !state.normal_resurrection_forbidden
        || state.source_issue != undeath_policy.source_issue
    {
        return invalid(
            "state.flags",
            "undead state must preserve Withering policy invariants",
        );
    }
    Ok(())
}

pub fn undeath_state_hash(state: &UndeadCharacterState) -> Result<String, EngineError> {
    serialize_sha256(state)
}

pub fn undeath_policy_hash(policy: &UndeathPolicy) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

fn form_spec(policy: &UndeathPolicy, form: UndeadForm) -> Result<&UndeadFormSpec, EngineError> {
    policy
        .forms
        .iter()
        .find(|spec| spec.form == form)
        .ok_or_else(|| invalid_error("form", format!("missing {}", form.label())))
}

fn validate_form_spec(spec: &UndeadFormSpec) -> Result<(), EngineError> {
    validate_token("form.required_focus_id", &spec.required_focus_id)?;
    validate_token("form.ritual.ability_id", &spec.ritual.ability_id)?;
    validate_text("form.ritual.display_name", &spec.ritual.display_name, 64)?;
    validate_text("form.ritual.description", &spec.ritual.description, 180)
}

fn validate_token_vec(field: &str, values: &[String], max_len: usize) -> Result<(), EngineError> {
    if values.is_empty() || values.len() > max_len {
        return invalid(field, format!("must contain 1..={max_len} values"));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_token(field, value)?;
        if !seen.insert(value.as_str()) {
            return invalid(field, format!("duplicate {value}"));
        }
    }
    Ok(())
}

fn withering_fingerprint(readback: &WitheringReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "ritual_id": readback.ritual_id,
        "character_id": readback.character_id,
        "chosen_form": readback.chosen_form,
        "constitution_before": readback.constitution_before,
        "constitution_after": readback.constitution_after,
        "focus_id": readback.focus_id,
        "unlocked_rituals": readback.unlocked_rituals,
        "before_mortality_sha256": readback.before_mortality_sha256,
        "undead_state_sha256": readback.undead_state_sha256,
        "policy_sha256": readback.policy_sha256,
    })
}

fn undead_mortality_fingerprint(readback: &UndeadMortalityReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "undead_character_id": readback.undead_character_id,
        "form": readback.form,
        "aging": readback.aging,
        "physical_kill_only": readback.physical_kill_only,
        "mortality_result_sha256": readback.mortality.result_sha256,
        "undead_before_sha256": readback.undead_before_sha256,
        "undead_after_sha256": readback.undead_after_sha256,
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
        field: format!("undeath.{}", field.into()),
        reason: reason.to_string(),
    }
}
