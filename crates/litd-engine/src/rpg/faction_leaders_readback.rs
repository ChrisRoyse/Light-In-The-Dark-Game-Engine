use serde_json::json;

use super::{FactionLeaderDecisionReadback, FactionLeaderLoweredEffect};

pub(super) fn ledger_payload(readback: &FactionLeaderDecisionReadback) -> serde_json::Value {
    json!({
        "decision_id": readback.decision_id,
        "accepted": readback.accepted,
        "grade": readback.grade,
        "action_kind": readback.action_kind,
        "target_rank": readback.target_rank,
        "lowered_effects": readback.lowered_effects,
        "evidence_refs": readback.evidence_refs,
    })
}

pub(super) fn decision_payload(readback: &FactionLeaderDecisionReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "decision_id": readback.decision_id,
        "organization_id": readback.organization_id,
        "leader_agent_id": readback.leader_agent_id,
        "signal_id": readback.signal_id,
        "action_kind": readback.action_kind,
        "accepted": readback.accepted,
        "grade": readback.grade,
        "reason": readback.reason,
        "target_rank": readback.target_rank,
        "effect_count": readback.effect_count,
        "catalog_sha256": readback.catalog_sha256,
        "signal_sha256": readback.signal_sha256,
        "rules_sha256": readback.rules_sha256,
        "theme_result_sha256": readback.theme_result_sha256,
        "decision_ledger_sha256": readback.decision_ledger_sha256,
    })
}

pub(super) fn effect_payload(effect: &FactionLeaderLoweredEffect) -> serde_json::Value {
    json!({
        "effect_id": effect.effect_id,
        "kind": effect.kind,
        "subject_character_id": effect.subject_character_id,
        "ledger_entry": effect.ledger_entry,
        "recruit_invitation_id": effect.recruit_invitation_id,
        "membership_status": effect.membership_status,
        "objective_id": effect.objective_id,
        "raid_order_id": effect.raid_order_id,
        "reward_id": effect.reward_id,
        "rank_after": effect.rank_after,
        "unlocked_power_ids": effect.unlocked_power_ids,
        "leader_order": effect.leader_order,
    })
}
