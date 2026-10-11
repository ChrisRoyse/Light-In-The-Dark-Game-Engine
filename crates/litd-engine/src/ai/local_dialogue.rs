use std::collections::{BTreeMap, BTreeSet};

use litd_hash::sha256_hex;

use crate::EngineError;

use super::{
    local_dialogue_hash::{lowered_order_fingerprint, readback_fingerprint},
    local_dialogue_types::*,
    local_dialogue_validate::{
        parse_model_response, validate_input, validate_model_response, validate_telemetry,
    },
};

pub fn build_local_npc_dialogue_prompt(
    input: &LocalNpcDialogueInput,
) -> Result<String, EngineError> {
    validate_input(input)?;
    let facts = input
        .grounding_facts
        .iter()
        .map(|fact| format!("- {}: {}", fact.fact_id, fact.text))
        .collect::<Vec<_>>()
        .join("\n");
    let allowed_intents = input
        .allowed_intents
        .iter()
        .map(|intent| intent.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let required_terms = input
        .grounding_facts
        .iter()
        .map(|fact| {
            format!(
                "- {}: {}",
                fact.fact_id,
                fact.required_dialogue_terms.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let natural_sentences = input
        .grounding_facts
        .iter()
        .map(|fact| format!("- {}: {}", fact.fact_id, fact.text))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "You are {display_name}, an in-character NPC in world {world_id}.\n\
         Role profile: {role_profile}\n\
         Return only valid JSON with exactly these keys: dialogue, intent, voice, \
         grounded_fact_ids, ward_tags.\n\
         Allowed intents: {allowed_intents}.\n\
         The JSON value for voice must be exactly \"in_character\".\n\
         Allowed Ward tags: {ward_tags}.\n\
         Use only these Calyx facts:\n{facts}\n\
         Preserve these natural fact sentences when answering:\n{natural_sentences}\n\
         Required dialogue terms for cited facts:\n{required_terms}\n\
         Player says: {player_utterance}\n\
         Signal: {signal_summary}\n\
         Rules:\n\
         - dialogue must be one short line, no more than {max_dialogue_chars} characters.\n\
         - dialogue must directly answer the player in a natural sentence, not as a \
         comma-separated term list.\n\
         - If answering with a fact, intent must be share_grounded_fact and grounded_fact_ids \
         must cite the exact fact ids used.\n\
         - grounded_fact_ids must copy the exact ids above, including prefixes and punctuation; \
         never shorten fact ids.\n\
         - For every cited fact, dialogue must include every required term listed for that fact.\n\
         - Do not mention blocked terms: {blocked_terms}.\n\
         - Do not invent locations, factions, items, abilities, times, or events outside the \
         listed Calyx facts.\n",
        display_name = input.display_name,
        world_id = input.world_id,
        role_profile = input.role_profile,
        allowed_intents = allowed_intents,
        ward_tags = input.ward_tags.join(", "),
        facts = facts,
        natural_sentences = natural_sentences,
        required_terms = required_terms,
        player_utterance = input.player_utterance,
        signal_summary = input.signal_summary,
        max_dialogue_chars = input.max_dialogue_chars,
        blocked_terms = input.blocked_terms.join(", "),
    ))
}

pub fn lower_local_npc_dialogue_response(
    input: &LocalNpcDialogueInput,
    prompt: &str,
    raw_model_response: &str,
    telemetry: &LocalNpcDialogueTelemetry,
) -> Result<LocalNpcDialogueReadback, EngineError> {
    validate_input(input)?;
    validate_telemetry(input, telemetry)?;
    let response = parse_model_response(raw_model_response)?;
    validate_model_response(input, &response)?;
    let provenance_refs = provenance_refs(input, &response);
    let mut lowered_order = LocalNpcDialogueLoweredOrder {
        order_id: format!(
            "dialogue.{}.{}.{}",
            input.run_id, input.npc_id, input.prompt_request_id
        ),
        source: "lowered_local_llm".to_string(),
        npc_id: input.npc_id.clone(),
        prompt_request_id: input.prompt_request_id.clone(),
        text: response.dialogue.clone(),
        intent: response.intent,
        grounded_fact_ids: response.grounded_fact_ids.clone(),
        provenance_refs: provenance_refs.clone(),
        deterministic: true,
        fingerprint: String::new(),
    };
    lowered_order.fingerprint = lowered_order_fingerprint(&lowered_order)?;
    let mut readback = LocalNpcDialogueReadback {
        schema: AI_LOCAL_NPC_DIALOGUE_SCHEMA.to_string(),
        run_id: input.run_id.clone(),
        prompt_request_id: input.prompt_request_id.clone(),
        npc_id: input.npc_id.clone(),
        display_name: input.display_name.clone(),
        world_id: input.world_id.clone(),
        selected_model_id: input.model_selection.selected_model_id.clone(),
        serving_model: input.serving_model.clone(),
        serving_endpoint: input.serving_endpoint.clone(),
        model_digest: telemetry.model_digest.clone(),
        accepted: true,
        dialogue: response.dialogue,
        intent: response.intent,
        voice: response.voice,
        grounded_fact_ids: response.grounded_fact_ids,
        ward_tags: response.ward_tags,
        provenance_refs,
        calyx_fact_count: input.grounding_facts.len(),
        prompt_sha256: sha256_hex(prompt.as_bytes()),
        raw_response_sha256: sha256_hex(raw_model_response.as_bytes()),
        total_duration_ms: telemetry.total_duration_ms,
        load_duration_ms: telemetry.load_duration_ms,
        prompt_eval_count: telemetry.prompt_eval_count,
        eval_count: telemetry.eval_count,
        warm_pool_loaded: telemetry.warm_pool_loaded,
        hot_path_policy: AI_LOCAL_NPC_DIALOGUE_HOT_PATH_POLICY.to_string(),
        calyx_live_query_allowed_in_hot_path: false,
        lowering_boundary:
            "local LLM output is validated out-of-band; sim/UI consume only fingerprinted dialogue"
                .to_string(),
        lowered_order,
        readback_fingerprint: String::new(),
    };
    readback.readback_fingerprint = readback_fingerprint(&readback)?;
    super::local_dialogue_validate::validate_local_npc_dialogue_readback(&readback)?;
    Ok(readback)
}

fn provenance_refs(
    input: &LocalNpcDialogueInput,
    response: &LocalNpcDialogueModelResponse,
) -> Vec<String> {
    let facts = fact_index(&input.grounding_facts);
    response
        .grounded_fact_ids
        .iter()
        .filter_map(|fact_id| facts.get(fact_id.as_str()))
        .map(|fact| fact.provenance_ref.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn fact_index(facts: &[CalyxDialogueFact]) -> BTreeMap<&str, &CalyxDialogueFact> {
    facts
        .iter()
        .map(|fact| (fact.fact_id.as_str(), fact))
        .collect()
}
