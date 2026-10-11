use litd_hash::sha256_hex;
use serde::Serialize;

use crate::EngineError;

use super::local_dialogue_types::{
    LocalNpcDialogueIntent, LocalNpcDialogueLoweredOrder, LocalNpcDialogueReadback,
};

pub(super) fn lowered_order_fingerprint(
    order: &LocalNpcDialogueLoweredOrder,
) -> Result<String, EngineError> {
    #[derive(Serialize)]
    struct Payload<'a> {
        order_id: &'a str,
        source: &'a str,
        npc_id: &'a str,
        prompt_request_id: &'a str,
        text: &'a str,
        intent: LocalNpcDialogueIntent,
        grounded_fact_ids: &'a [String],
        provenance_refs: &'a [String],
        deterministic: bool,
    }
    sha256_json(&Payload {
        order_id: &order.order_id,
        source: &order.source,
        npc_id: &order.npc_id,
        prompt_request_id: &order.prompt_request_id,
        text: &order.text,
        intent: order.intent,
        grounded_fact_ids: &order.grounded_fact_ids,
        provenance_refs: &order.provenance_refs,
        deterministic: order.deterministic,
    })
}

pub(super) fn readback_fingerprint(
    readback: &LocalNpcDialogueReadback,
) -> Result<String, EngineError> {
    #[derive(Serialize)]
    struct Payload<'a> {
        schema: &'a str,
        run_id: &'a str,
        prompt_request_id: &'a str,
        npc_id: &'a str,
        selected_model_id: &'a str,
        serving_model: &'a str,
        model_digest: &'a str,
        dialogue: &'a str,
        intent: LocalNpcDialogueIntent,
        grounded_fact_ids: &'a [String],
        ward_tags: &'a [String],
        provenance_refs: &'a [String],
        prompt_sha256: &'a str,
        raw_response_sha256: &'a str,
        hot_path_policy: &'a str,
        lowered_order_fingerprint: &'a str,
    }
    sha256_json(&Payload {
        schema: &readback.schema,
        run_id: &readback.run_id,
        prompt_request_id: &readback.prompt_request_id,
        npc_id: &readback.npc_id,
        selected_model_id: &readback.selected_model_id,
        serving_model: &readback.serving_model,
        model_digest: &readback.model_digest,
        dialogue: &readback.dialogue,
        intent: readback.intent,
        grounded_fact_ids: &readback.grounded_fact_ids,
        ward_tags: &readback.ward_tags,
        provenance_refs: &readback.provenance_refs,
        prompt_sha256: &readback.prompt_sha256,
        raw_response_sha256: &readback.raw_response_sha256,
        hot_path_policy: &readback.hot_path_policy,
        lowered_order_fingerprint: &readback.lowered_order.fingerprint,
    })
}

fn sha256_json(value: &impl Serialize) -> Result<String, EngineError> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "local-npc-dialogue-fingerprint-json",
            reason: error.to_string(),
        })
}
