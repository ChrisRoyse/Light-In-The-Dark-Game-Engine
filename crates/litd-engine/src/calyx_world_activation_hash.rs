use litd_hash::sha256_hex;
use serde_json::json;

use super::calyx_world_activation_types::{
    EngineWorldActivationIntent, EngineWorldActivationReadback, EngineWorldFact,
};

pub(super) fn activation_fingerprint(
    readback: &EngineWorldActivationReadback,
) -> Result<String, serde_json::Error> {
    serde_json::to_vec(&json!({
        "schema": readback.schema,
        "run_id": readback.run_id,
        "world_id": readback.world_id,
        "policy": readback.policy,
        "fact_count": readback.fact_count,
        "fact_kind_count": readback.fact_kind_count,
        "agent_count": readback.agent_count,
        "associations": readback.associations,
        "decisions": readback.decisions,
        "orders": readback.orders.iter().map(|order| order.fingerprint()).collect::<Vec<_>>(),
        "hot_path_policy": readback.hot_path_policy,
        "calyx_live_query_allowed_in_hot_path": readback.calyx_live_query_allowed_in_hot_path
    }))
    .map(|bytes| sha256_hex(&bytes))
}

pub(super) fn association_id(run_id: &str, agent_id: &str, facts: &[&EngineWorldFact]) -> String {
    let fact_ids = facts
        .iter()
        .map(|fact| fact.fact_id.as_str())
        .collect::<Vec<_>>()
        .join("|");
    sha256_hex(format!("{run_id}:{agent_id}:{fact_ids}").as_bytes())
}

pub(super) fn decision_id(
    run_id: &str,
    agent_id: &str,
    intent: EngineWorldActivationIntent,
) -> String {
    sha256_hex(format!("{run_id}:{agent_id}:{}", intent.as_str()).as_bytes())
}
