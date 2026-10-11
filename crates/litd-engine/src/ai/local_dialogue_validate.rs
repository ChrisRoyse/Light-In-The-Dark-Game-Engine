use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::EngineError;

use super::{
    local_dialogue_hash::{lowered_order_fingerprint, readback_fingerprint},
    local_dialogue_types::*,
    model_tiering::validate_agent_model_selection_readback,
    model_tiering_types::{AgentModelRole, AgentModelTier},
};

pub fn validate_local_npc_dialogue_readback(
    readback: &LocalNpcDialogueReadback,
) -> Result<(), EngineError> {
    if readback.schema != AI_LOCAL_NPC_DIALOGUE_SCHEMA {
        return invalid("readback.schema", "unsupported local NPC dialogue schema");
    }
    validate_id("readback.run_id", &readback.run_id)?;
    validate_id("readback.prompt_request_id", &readback.prompt_request_id)?;
    validate_id("readback.npc_id", &readback.npc_id)?;
    validate_text("readback.dialogue", &readback.dialogue)?;
    if !readback.accepted {
        return invalid(
            "readback.accepted",
            "validated dialogue readbacks must be accepted",
        );
    }
    if readback.hot_path_policy != AI_LOCAL_NPC_DIALOGUE_HOT_PATH_POLICY {
        return invalid("readback.hot_path_policy", "unexpected hot-path policy");
    }
    if readback.calyx_live_query_allowed_in_hot_path {
        return invalid(
            "readback.calyx_live_query_allowed_in_hot_path",
            "live Calyx queries are forbidden in deterministic sim ticks",
        );
    }
    if !readback.lowered_order.deterministic {
        return invalid(
            "readback.lowered_order",
            "lowered order must be deterministic",
        );
    }
    let expected_order = lowered_order_fingerprint(&readback.lowered_order)?;
    if readback.lowered_order.fingerprint != expected_order {
        return Err(EngineError::AiReadbackMismatch {
            field: "local_dialogue.lowered_order.fingerprint".to_string(),
            expected: readback.lowered_order.fingerprint.clone(),
            actual: expected_order,
        });
    }
    let expected = readback_fingerprint(readback)?;
    if readback.readback_fingerprint != expected {
        return Err(EngineError::AiReadbackMismatch {
            field: "local_dialogue.readback_fingerprint".to_string(),
            expected: readback.readback_fingerprint.clone(),
            actual: expected,
        });
    }
    Ok(())
}

pub(super) fn validate_input(input: &LocalNpcDialogueInput) -> Result<(), EngineError> {
    validate_id("input.run_id", &input.run_id)?;
    validate_id("input.prompt_request_id", &input.prompt_request_id)?;
    validate_id("input.npc_id", &input.npc_id)?;
    validate_id("input.world_id", &input.world_id)?;
    validate_id("input.prompt_profile_id", &input.prompt_profile_id)?;
    validate_text("input.display_name", &input.display_name)?;
    validate_text("input.role_profile", &input.role_profile)?;
    validate_text("input.player_utterance", &input.player_utterance)?;
    validate_text("input.signal_summary", &input.signal_summary)?;
    validate_tag_list("input.ward_tags", &input.ward_tags)?;
    validate_term_list("input.blocked_terms", &input.blocked_terms)?;
    validate_text("input.serving_model", &input.serving_model)?;
    validate_endpoint(&input.serving_endpoint)?;
    validate_intents(input)?;
    validate_facts(input)?;
    if input.max_dialogue_chars == 0 || input.max_dialogue_chars > 512 {
        return invalid(
            "input.max_dialogue_chars",
            "must be within the bounded 1..=512 dialogue budget",
        );
    }
    validate_agent_model_selection_readback(&input.model_selection)?;
    if input.model_selection.role != AgentModelRole::NpcDialogue {
        return invalid(
            "input.model_selection.role",
            "must select an NPC dialogue model",
        );
    }
    if input.model_selection.selected_tier != AgentModelTier::NpcDialogue {
        return invalid(
            "input.model_selection.selected_tier",
            "must select the NPC dialogue tier",
        );
    }
    Ok(())
}

pub(super) fn validate_telemetry(
    input: &LocalNpcDialogueInput,
    telemetry: &LocalNpcDialogueTelemetry,
) -> Result<(), EngineError> {
    if telemetry.model_name != input.serving_model {
        return invalid(
            "telemetry.model_name",
            format!(
                "expected serving model {}, got {}",
                input.serving_model, telemetry.model_name
            ),
        );
    }
    validate_text("telemetry.model_digest", &telemetry.model_digest)?;
    validate_text("telemetry.done_reason", &telemetry.done_reason)?;
    if telemetry.total_duration_ms == 0 || telemetry.eval_count == 0 {
        return invalid(
            "telemetry.duration",
            "model call must report real generation work",
        );
    }
    if !telemetry.warm_pool_loaded {
        return invalid("telemetry.warm_pool_loaded", "model must be warm-pooled");
    }
    Ok(())
}

pub(super) fn parse_model_response(
    raw: &str,
) -> Result<LocalNpcDialogueModelResponse, EngineError> {
    validate_text("model_response.raw", raw)?;
    let value: Value = serde_json::from_str(raw).map_err(|error| EngineError::AiInvalidConfig {
        field: "local_dialogue.model_response.json".to_string(),
        reason: error.to_string(),
    })?;
    let Some(object) = value.as_object() else {
        return invalid("model_response", "must be a JSON object");
    };
    let expected = BTreeSet::from([
        "dialogue",
        "intent",
        "voice",
        "grounded_fact_ids",
        "ward_tags",
    ]);
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if actual != expected {
        return invalid(
            "model_response.keys",
            format!("expected exactly {:?}, got {:?}", expected, actual),
        );
    }
    serde_json::from_value(value).map_err(|error| EngineError::AiInvalidConfig {
        field: "local_dialogue.model_response.schema".to_string(),
        reason: error.to_string(),
    })
}

pub(super) fn validate_model_response(
    input: &LocalNpcDialogueInput,
    response: &LocalNpcDialogueModelResponse,
) -> Result<(), EngineError> {
    validate_response_shape(input, response)?;
    for term in &input.blocked_terms {
        if contains_term(&response.dialogue, term) {
            return invalid(
                "model_response.dialogue",
                format!("dialogue mentioned blocked out-of-distribution term {term}"),
            );
        }
    }
    let facts = fact_index(&input.grounding_facts);
    let mut cited = BTreeSet::new();
    for fact_id in &response.grounded_fact_ids {
        validate_id("model_response.grounded_fact_ids", fact_id)?;
        if !cited.insert(fact_id.as_str()) {
            return invalid("model_response.grounded_fact_ids", "must be unique");
        }
        validate_cited_fact(input, response, &facts, fact_id)?;
    }
    Ok(())
}

fn validate_intents(input: &LocalNpcDialogueInput) -> Result<(), EngineError> {
    if input.allowed_intents.is_empty() {
        return invalid("input.allowed_intents", "must not be empty");
    }
    let mut intents = BTreeSet::new();
    for intent in &input.allowed_intents {
        if !intents.insert(*intent) {
            return invalid("input.allowed_intents", "must be unique");
        }
    }
    Ok(())
}

fn validate_facts(input: &LocalNpcDialogueInput) -> Result<(), EngineError> {
    if input.grounding_facts.is_empty() {
        return invalid("input.grounding_facts", "must not be empty");
    }
    let mut fact_ids = BTreeSet::new();
    for fact in &input.grounding_facts {
        validate_fact(fact, &input.ward_tags)?;
        if !fact_ids.insert(fact.fact_id.as_str()) {
            return invalid("input.grounding_facts.fact_id", "must be unique");
        }
    }
    Ok(())
}

fn validate_fact(fact: &CalyxDialogueFact, ward_tags: &[String]) -> Result<(), EngineError> {
    validate_id("fact.fact_id", &fact.fact_id)?;
    validate_text("fact.text", &fact.text)?;
    validate_tag_list("fact.ward_tags", &fact.ward_tags)?;
    validate_ref("fact.provenance_ref", &fact.provenance_ref)?;
    validate_term_list(
        "fact.required_dialogue_terms",
        &fact.required_dialogue_terms,
    )?;
    for tag in &fact.ward_tags {
        if !ward_tags.contains(tag) {
            return invalid(
                "fact.ward_tags",
                format!(
                    "fact {} uses Ward tag {tag} outside NPC envelope",
                    fact.fact_id
                ),
            );
        }
    }
    Ok(())
}

fn validate_response_shape(
    input: &LocalNpcDialogueInput,
    response: &LocalNpcDialogueModelResponse,
) -> Result<(), EngineError> {
    validate_text("model_response.dialogue", &response.dialogue)?;
    if response.dialogue.chars().count() > input.max_dialogue_chars {
        return invalid(
            "model_response.dialogue",
            "exceeds dialogue character budget",
        );
    }
    if response.voice != "in_character" {
        return invalid("model_response.voice", "must be in_character");
    }
    if !input.allowed_intents.contains(&response.intent) {
        return invalid("model_response.intent", "intent is outside the allowed set");
    }
    validate_tag_list("model_response.ward_tags", &response.ward_tags)?;
    for tag in &response.ward_tags {
        if !input.ward_tags.contains(tag) {
            return invalid(
                "model_response.ward_tags",
                format!("Ward tag {tag} is outside NPC envelope"),
            );
        }
    }
    if response.intent != LocalNpcDialogueIntent::RefuseOutOfDistribution
        && response.grounded_fact_ids.is_empty()
    {
        return invalid(
            "model_response.grounded_fact_ids",
            "non-refusal dialogue must cite at least one Calyx fact",
        );
    }
    Ok(())
}

fn validate_cited_fact(
    _input: &LocalNpcDialogueInput,
    response: &LocalNpcDialogueModelResponse,
    facts: &BTreeMap<&str, &CalyxDialogueFact>,
    fact_id: &str,
) -> Result<(), EngineError> {
    let Some(fact) = facts.get(fact_id) else {
        return invalid(
            "model_response.grounded_fact_ids",
            format!("{fact_id} is not present in Calyx grounding facts"),
        );
    };
    for tag in &fact.ward_tags {
        if !response.ward_tags.contains(tag) {
            return invalid(
                "model_response.ward_tags",
                format!("cited fact {fact_id} requires Ward tag {tag}"),
            );
        }
    }
    for term in &fact.required_dialogue_terms {
        if !contains_term(&response.dialogue, term) {
            return invalid(
                "model_response.dialogue",
                format!("dialogue did not include required grounded term {term}"),
            );
        }
    }
    Ok(())
}

fn fact_index(facts: &[CalyxDialogueFact]) -> BTreeMap<&str, &CalyxDialogueFact> {
    facts
        .iter()
        .map(|fact| (fact.fact_id.as_str(), fact))
        .collect()
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    validate_text(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return invalid(field, "must contain only stable ASCII id characters");
    }
    Ok(())
}

fn validate_tag_list(field: &str, tags: &[String]) -> Result<(), EngineError> {
    if tags.is_empty() {
        return invalid(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for tag in tags {
        validate_id(field, tag)?;
        if !seen.insert(tag.as_str()) {
            return invalid(field, "must be unique");
        }
    }
    Ok(())
}

fn validate_term_list(field: &str, terms: &[String]) -> Result<(), EngineError> {
    if terms.is_empty() {
        return invalid(field, "must not be empty");
    }
    let mut seen = BTreeSet::new();
    for term in terms {
        validate_text(field, term)?;
        if !seen.insert(term.to_ascii_lowercase()) {
            return invalid(field, "must be unique ignoring ASCII case");
        }
    }
    Ok(())
}

fn validate_ref(field: &str, value: &str) -> Result<(), EngineError> {
    validate_text(field, value)?;
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'#')
    }) {
        return invalid(field, "must contain only stable ASCII reference characters");
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<(), EngineError> {
    validate_text("input.serving_endpoint", value)?;
    if !value.starts_with("http://127.0.0.1:") && !value.starts_with("http://localhost:") {
        return invalid(
            "input.serving_endpoint",
            "local dialogue serving must use an explicit localhost endpoint",
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.chars().any(|ch| ch.is_control() && ch != '\n') {
        return invalid(
            field,
            "must not contain control characters other than newline",
        );
    }
    Ok(())
}

fn contains_term(text: &str, term: &str) -> bool {
    text.to_ascii_lowercase()
        .contains(&term.to_ascii_lowercase())
}

fn invalid<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::AiInvalidConfig {
        field: format!("local_dialogue.{field}"),
        reason: reason.into(),
    })
}
