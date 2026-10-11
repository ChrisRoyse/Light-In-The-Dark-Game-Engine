use std::collections::BTreeSet;

use litd_engine::{EngineError, render::shader::shader_fingerprint};
use serde::{Deserialize, Serialize};

#[path = "content_trust_store.rs"]
mod content_trust_store;

pub use content_trust_store::{
    ContentTrustAsterReadback, parse_content_trust_ledger, read_content_trust_ledger,
    write_content_trust_ledger,
};

pub const CONTENT_TRUST_SCHEMA: &str = "litd.content_trust.ledger.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SharedContentOrigin {
    FirstParty,
    SharedProject,
    UserGenerated,
    ImportedArchive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentTrustStatus {
    Trusted,
    Provisional,
    Refused,
    Escalated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTrustPolicy {
    pub policy_id: String,
    pub allowed_purposes: Vec<String>,
    pub required_consent_scopes: Vec<String>,
    pub trusted_sources: Vec<String>,
    pub min_grounding_refs: usize,
    pub blocked_terms: Vec<String>,
    pub escalation_terms: Vec<String>,
    pub escalation_queue: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedContentImport {
    pub import_id: String,
    pub content_id: String,
    pub origin: SharedContentOrigin,
    pub creator_id: String,
    pub source_uri: String,
    pub declared_purpose: String,
    pub consent_scopes: Vec<String>,
    pub grounding_refs: Vec<String>,
    pub body: String,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTrustEscalation {
    pub queue: String,
    pub reason: String,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTrustDecision {
    pub schema: String,
    pub policy_id: String,
    pub import_id: String,
    pub content_id: String,
    pub status: ContentTrustStatus,
    pub code: String,
    pub reasons: Vec<String>,
    pub declared_purpose: String,
    pub consent_scopes: Vec<String>,
    pub missing_consent_scopes: Vec<String>,
    pub grounding_refs: Vec<String>,
    pub source_uri: String,
    pub creator_id: String,
    pub content_fingerprint: String,
    pub escalation: Option<ContentTrustEscalation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTrustLedger {
    pub schema: String,
    pub ledger_id: String,
    pub decision_count: usize,
    pub decisions: Vec<ContentTrustDecision>,
}

pub fn evaluate_content_trust(
    import: &SharedContentImport,
    policy: &ContentTrustPolicy,
) -> Result<ContentTrustDecision, EngineError> {
    validate_policy(policy)?;
    validate_import(import)?;
    let allowed_purposes = normalize(policy.allowed_purposes.clone());
    let required_consent_scopes = normalize(policy.required_consent_scopes.clone());
    let trusted_sources = normalize(policy.trusted_sources.clone());
    let consent_scopes = normalize(import.consent_scopes.clone());
    let grounding_refs = normalize(import.grounding_refs.clone());
    let declared_purpose = import.declared_purpose.trim().to_ascii_lowercase();
    let source_uri = import.source_uri.trim().to_ascii_lowercase();
    let missing_consent_scopes = required_consent_scopes
        .iter()
        .filter(|scope| !consent_scopes.contains(scope))
        .cloned()
        .collect::<Vec<_>>();
    let blocked_terms = matching_terms(&import.body, &policy.blocked_terms);
    let escalation_terms = matching_terms(&import.body, &policy.escalation_terms);
    let purpose_allowed = allowed_purposes.contains(&declared_purpose);
    let trusted_source =
        import.origin == SharedContentOrigin::FirstParty && trusted_sources.contains(&source_uri);
    let grounded = grounding_refs.len() >= policy.min_grounding_refs;

    let (status, code, reasons, escalation) = if !missing_consent_scopes.is_empty() {
        (
            ContentTrustStatus::Refused,
            "LITD-TRUST-CONSENT-MISSING",
            vec![format!(
                "missing consent scopes: {}",
                missing_consent_scopes.join("|")
            )],
            None,
        )
    } else if !purpose_allowed {
        (
            ContentTrustStatus::Refused,
            "LITD-TRUST-PURPOSE-DENIED",
            vec![format!("purpose {declared_purpose} is not allowed")],
            None,
        )
    } else if !blocked_terms.is_empty() {
        (
            ContentTrustStatus::Refused,
            "LITD-TRUST-BLOCKED-CONTENT",
            vec![format!("blocked terms: {}", blocked_terms.join("|"))],
            None,
        )
    } else if !escalation_terms.is_empty() {
        (
            ContentTrustStatus::Escalated,
            "LITD-TRUST-ESCALATION-REQUIRED",
            vec!["content requires trust-and-safety review before ecosystem sharing".to_string()],
            Some(ContentTrustEscalation {
                queue: policy.escalation_queue.clone(),
                reason: "matched escalation terms".to_string(),
                matched_terms: escalation_terms,
            }),
        )
    } else if trusted_source {
        (
            ContentTrustStatus::Trusted,
            "LITD-TRUST-TRUSTED-SOURCE",
            vec!["first-party trusted source with consent and allowed purpose".to_string()],
            None,
        )
    } else if grounded {
        (
            ContentTrustStatus::Trusted,
            "LITD-TRUST-GROUNDED-IMPORT",
            vec!["shared content has sufficient grounding refs".to_string()],
            None,
        )
    } else {
        (
            ContentTrustStatus::Provisional,
            "LITD-TRUST-PROVISIONAL-COLD-START",
            vec![format!(
                "untrusted content has {} grounding refs, needs {}",
                grounding_refs.len(),
                policy.min_grounding_refs
            )],
            None,
        )
    };

    Ok(ContentTrustDecision {
        schema: CONTENT_TRUST_SCHEMA.to_string(),
        policy_id: policy.policy_id.clone(),
        import_id: import.import_id.clone(),
        content_id: import.content_id.clone(),
        status,
        code: code.to_string(),
        reasons,
        declared_purpose,
        consent_scopes,
        missing_consent_scopes,
        grounding_refs,
        source_uri: import.source_uri.clone(),
        creator_id: import.creator_id.clone(),
        content_fingerprint: content_fingerprint(import),
        escalation,
    })
}

pub fn build_content_trust_ledger(
    ledger_id: impl Into<String>,
    decisions: Vec<ContentTrustDecision>,
) -> Result<ContentTrustLedger, EngineError> {
    let ledger = ContentTrustLedger {
        schema: CONTENT_TRUST_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        decision_count: decisions.len(),
        decisions,
    };
    validate_content_trust_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_content_trust_ledger(ledger: &ContentTrustLedger) -> Result<(), EngineError> {
    if ledger.schema != CONTENT_TRUST_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.decision_count == 0
        || ledger.decision_count != ledger.decisions.len()
    {
        return invalid_trust("ledger", "schema, id, and decision count must be valid");
    }
    let mut seen = BTreeSet::new();
    for decision in &ledger.decisions {
        validate_content_trust_decision(decision)?;
        if !seen.insert(decision.import_id.clone()) {
            return invalid_trust("ledger.duplicate", "duplicate import decision");
        }
    }
    Ok(())
}

pub fn validate_content_trust_decision(decision: &ContentTrustDecision) -> Result<(), EngineError> {
    if decision.schema != CONTENT_TRUST_SCHEMA
        || decision.policy_id.trim().is_empty()
        || decision.import_id.trim().is_empty()
        || decision.content_id.trim().is_empty()
        || decision.content_fingerprint.trim().is_empty()
        || decision.code.trim().is_empty()
        || decision.reasons.is_empty()
    {
        return invalid_trust("decision", "required trust decision fields are missing");
    }
    if decision.status == ContentTrustStatus::Refused
        && decision.code.ends_with("MISSING")
        && decision.missing_consent_scopes.is_empty()
    {
        return invalid_trust(
            "decision.consent",
            "missing-consent refusal must list missing scopes",
        );
    }
    if decision.status == ContentTrustStatus::Escalated && decision.escalation.is_none() {
        return invalid_trust(
            "decision.escalation",
            "escalation decision must carry route",
        );
    }
    match decision.status {
        ContentTrustStatus::Trusted => {
            if !matches!(
                decision.code.as_str(),
                "LITD-TRUST-TRUSTED-SOURCE" | "LITD-TRUST-GROUNDED-IMPORT"
            ) || decision.escalation.is_some()
            {
                return invalid_trust("decision.trusted", "trusted decision code is inconsistent");
            }
        }
        ContentTrustStatus::Provisional => {
            if decision.code != "LITD-TRUST-PROVISIONAL-COLD-START" || decision.escalation.is_some()
            {
                return invalid_trust(
                    "decision.provisional",
                    "provisional decision code is inconsistent",
                );
            }
        }
        ContentTrustStatus::Refused => {
            if !matches!(
                decision.code.as_str(),
                "LITD-TRUST-CONSENT-MISSING"
                    | "LITD-TRUST-PURPOSE-DENIED"
                    | "LITD-TRUST-BLOCKED-CONTENT"
            ) || decision.escalation.is_some()
            {
                return invalid_trust("decision.refused", "refused decision code is inconsistent");
            }
        }
        ContentTrustStatus::Escalated => {
            let Some(route) = decision.escalation.as_ref() else {
                return invalid_trust("decision.escalation", "escalation route is missing");
            };
            if decision.code != "LITD-TRUST-ESCALATION-REQUIRED"
                || route.queue.trim().is_empty()
                || route.reason.trim().is_empty()
                || route.matched_terms.is_empty()
            {
                return invalid_trust(
                    "decision.escalation",
                    "escalation decision route is incomplete",
                );
            }
        }
    }
    Ok(())
}

fn validate_policy(policy: &ContentTrustPolicy) -> Result<(), EngineError> {
    if policy.policy_id.trim().is_empty()
        || policy.allowed_purposes.is_empty()
        || policy.required_consent_scopes.is_empty()
        || policy.min_grounding_refs == 0
        || policy.escalation_queue.trim().is_empty()
    {
        return invalid_trust(
            "policy",
            "policy id, purposes, consent, grounding, and queue are required",
        );
    }
    validate_required_list("policy.allowed_purposes", &policy.allowed_purposes)?;
    validate_required_list(
        "policy.required_consent_scopes",
        &policy.required_consent_scopes,
    )?;
    validate_optional_list("policy.trusted_sources", &policy.trusted_sources)?;
    validate_optional_list("policy.blocked_terms", &policy.blocked_terms)?;
    validate_optional_list("policy.escalation_terms", &policy.escalation_terms)?;
    Ok(())
}

fn validate_import(import: &SharedContentImport) -> Result<(), EngineError> {
    if import.import_id.trim().is_empty()
        || import.content_id.trim().is_empty()
        || import.creator_id.trim().is_empty()
        || import.source_uri.trim().is_empty()
        || import.declared_purpose.trim().is_empty()
        || import.body.trim().is_empty()
        || import.requested_by.trim().is_empty()
    {
        return invalid_trust(
            "import",
            "import identity, source, purpose, body, and actor are required",
        );
    }
    validate_token("import.import_id", &import.import_id)?;
    validate_token("import.content_id", &import.content_id)?;
    validate_token("import.creator_id", &import.creator_id)?;
    validate_token("import.source_uri", &import.source_uri)?;
    validate_token("import.declared_purpose", &import.declared_purpose)?;
    validate_token("import.requested_by", &import.requested_by)?;
    validate_optional_list("import.consent_scopes", &import.consent_scopes)?;
    validate_optional_list("import.grounding_refs", &import.grounding_refs)?;
    Ok(())
}

fn validate_required_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.is_empty() {
        return invalid_trust(field, "required trust list must not be empty");
    }
    validate_optional_list(field, values)
}

fn validate_optional_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    for value in values {
        validate_token(field, value)?;
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_trust(field, "trust value must be non-empty and single-line");
    }
    Ok(())
}

fn normalize(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn matching_terms(body: &str, terms: &[String]) -> Vec<String> {
    let haystack = body.to_ascii_lowercase();
    terms
        .iter()
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty() && haystack.contains(term))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn content_fingerprint(import: &SharedContentImport) -> String {
    shader_fingerprint(&format!(
        "{:?}|{}|{}|{}|{}|{}",
        import.origin,
        import.content_id,
        import.creator_id,
        import.source_uri,
        import.declared_purpose,
        import.body
    ))
}

pub(crate) fn invalid_trust<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(trust_error(field, reason))
}

pub(crate) fn trust_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.content_trust.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "content_trust_tests.rs"]
mod tests;
