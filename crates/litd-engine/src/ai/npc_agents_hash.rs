use litd_hash::Fnv1a64;

use super::npc_agents_types::{
    NpcAgentActivationDecision, NpcAgentActivationReadback, NpcAgentIntent, NpcWorldSignal,
};

pub fn compute_npc_agent_activation_fnv1a64(readback: &NpcAgentActivationReadback) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &readback.schema);
    update_str(&mut hasher, &readback.runtime_id);
    update_str(&mut hasher, &readback.catalog_id);
    hasher.update_u64_le(readback.npc_count as u64);
    hasher.update_u64_le(readback.default_agent_count as u64);
    hasher.update_u64_le(readback.signal_count as u64);
    hasher.update_u64_le(readback.decision_count as u64);
    hasher.update_u64_le(readback.accepted_count as u64);
    hasher.update_u64_le(readback.skipped_count as u64);
    hasher.update_u64_le(readback.prompt_count as u64);
    hasher.update_u64_le(readback.order_count as u64);
    hasher.update_u64_le(readback.spawn_request_count as u64);
    update_str(&mut hasher, &readback.hot_path_policy);
    hasher.update_u8(u8::from(readback.calyx_live_query_allowed_in_hot_path));
    for decision in &readback.decisions {
        hash_decision(&mut hasher, decision);
    }
    for order in &readback.orders {
        hasher.update_u64_le(order.fingerprint());
    }
    hasher.finish_hex()
}

pub(super) fn activation_id(
    runtime_id: &str,
    npc_id: &str,
    signal: &NpcWorldSignal,
    intent: NpcAgentIntent,
) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, runtime_id);
    update_str(&mut hasher, npc_id);
    update_str(&mut hasher, &signal.signal_id);
    hasher.update_u64_le(signal.tick);
    update_str(&mut hasher, intent.as_str());
    hasher.finish_hex()
}

fn hash_decision(hasher: &mut Fnv1a64, decision: &NpcAgentActivationDecision) {
    update_str(hasher, &decision.activation_id);
    update_str(hasher, &decision.signal_id);
    hash_optional_text(hasher, &decision.npc_id);
    hash_optional_u32(hasher, decision.default_agent_id);
    hash_optional_text(hasher, &decision.prompt_profile_id);
    update_str(hasher, decision.intent.as_str());
    hasher.update_u8(u8::from(decision.accepted));
    hasher.update_u8(decision.grade as u8);
    update_str(hasher, &decision.reason);
    hasher.update_u64_le(decision.relevance_score as u64);
    hasher.update_u8(u8::from(decision.spawn_requested));
    hash_optional_text(hasher, &decision.prompt_request_id);
    hash_optional_text(hasher, &decision.prompt);
    hash_optional_u64(hasher, decision.execute_tick);
    hash_optional_u64(hasher, decision.order_fingerprint);
    hasher.update_u64_le(decision.provenance_refs.len() as u64);
    for reference in &decision.provenance_refs {
        update_str(hasher, reference);
    }
}

fn hash_optional_text(hasher: &mut Fnv1a64, value: &Option<String>) {
    match value {
        Some(value) => {
            hasher.update_u8(1);
            update_str(hasher, value);
        }
        None => {
            hasher.update_u8(0);
        }
    }
}

fn hash_optional_u32(hasher: &mut Fnv1a64, value: Option<u32>) {
    match value {
        Some(value) => {
            hasher.update_u8(1);
            hasher.update_u64_le(value as u64);
        }
        None => {
            hasher.update_u8(0);
        }
    }
}

fn hash_optional_u64(hasher: &mut Fnv1a64, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update_u8(1);
            hasher.update_u64_le(value);
        }
        None => {
            hasher.update_u8(0);
        }
    }
}

fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}
