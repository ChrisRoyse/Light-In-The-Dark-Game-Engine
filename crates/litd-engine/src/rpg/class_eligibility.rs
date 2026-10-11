use crate::EngineError;

use super::class_specs::{validate_class_spec, validate_eligibility_policy};
use super::class_specs_types::{
    ClassEligibilityClause, ClassEligibilityPolicy, ClassRaceAccess, ClassSpec,
};

/// Evaluates the authored class-level OR-of-AND policy. The class race gate is
/// checked independently and must pass before any alignment/ethos clause can
/// admit the character.
pub fn class_spec_eligibility(
    spec: &ClassSpec,
    race_id: &str,
    alignment: &str,
    ethos: &str,
) -> Result<bool, EngineError> {
    validate_class_spec(spec)?;
    if let ClassRaceAccess::Restricted { race_ids } = &spec.race_access
        && !race_ids.iter().any(|allowed| allowed == race_id)
    {
        return Ok(false);
    }
    Ok(policy_matches(
        &spec.eligibility,
        race_id,
        alignment,
        ethos,
    ))
}

/// Evaluates class admission followed by an optional specialization policy.
/// A specialization without an authored policy imposes no additional gate;
/// it never weakens the class policy and never invents one from prose.
pub fn specialization_eligibility(
    spec: &ClassSpec,
    specialization_id: &str,
    race_id: &str,
    alignment: &str,
    ethos: &str,
) -> Result<bool, EngineError> {
    if !class_spec_eligibility(spec, race_id, alignment, ethos)? {
        return Ok(false);
    }
    let specialization = spec
        .specializations
        .iter()
        .find(|candidate| candidate.specialization_id == specialization_id)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: "specialization_id".to_string(),
            reason: format!("unknown specialization {specialization_id}"),
        })?;
    let Some(policy) = &specialization.eligibility else {
        return Ok(true);
    };
    validate_eligibility_policy("specialization.eligibility", policy, None)?;
    Ok(policy_matches(policy, race_id, alignment, ethos))
}

fn policy_matches(
    policy: &ClassEligibilityPolicy,
    race_id: &str,
    alignment: &str,
    ethos: &str,
) -> bool {
    policy
        .clauses
        .iter()
        .any(|clause| clause_matches(clause, race_id, alignment, ethos))
}

fn clause_matches(
    clause: &ClassEligibilityClause,
    race_id: &str,
    alignment: &str,
    ethos: &str,
) -> bool {
    clause.match_all
        || (clause
        .race_ids
        .as_ref()
        .is_none_or(|values| values.iter().any(|value| value == race_id))
        && clause
            .alignments
            .as_ref()
            .is_none_or(|values| values.iter().any(|value| value == alignment))
        && clause
            .ethoses
            .as_ref()
            .is_none_or(|values| values.iter().any(|value| value == ethos)))
}
