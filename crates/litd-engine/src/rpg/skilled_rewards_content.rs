use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    RPG_SKILLED_REWARD_SCHEMA, SkilledRewardPolicy, SkilledRewardReadback,
    rpg_authored_payload_without_nulls, validate_rpg_authored_content_records,
    validate_skilled_reward_readback,
};

pub fn skilled_reward_content_records(
    policy: &SkilledRewardPolicy,
    readback: &SkilledRewardReadback,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_skilled_reward_readback(policy, readback)?;
    let mut records = vec![
        content_record(
            EngineContentKind::Lore,
            &policy.server_policy_id,
            "Skilled Play Reward Policy",
            json!({
                "schema": RPG_SKILLED_REWARD_SCHEMA,
                "source_issue": 1047,
                "content_migration_issue": 1455,
                "surface_kind": "reward_policy",
                "policy": policy,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Story,
            &format!("ledger.skill_reward.{}", readback.candidate_id),
            "Skilled Play Reward Ledger",
            json!({
                "schema": RPG_SKILLED_REWARD_SCHEMA,
                "source_issue": 1047,
                "content_migration_issue": 1455,
                "surface_kind": "reward_ledger",
                "readback": readback,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
    ];
    for effect in &readback.lowered_effects {
        records.push(content_record(
            EngineContentKind::Trigger,
            &effect.reward_id,
            "Skilled Play Reward Effect",
            json!({
                "schema": RPG_SKILLED_REWARD_SCHEMA,
                "source_issue": 1047,
                "content_migration_issue": 1455,
                "surface_kind": "reward_effect",
                "effect": effect,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "skilled_rewards.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn content_record(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/skilled_rewards/{id}"),
            author: "editor_authoring.issue1455.skilled_rewards".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
