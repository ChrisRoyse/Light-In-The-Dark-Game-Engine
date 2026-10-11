use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    InfernalAttritionPolicy, RPG_INFERNAL_ATTRITION_SCHEMA, infernal_attrition_policy_hash,
    rpg_authored_payload_without_nulls, validate_infernal_attrition_policy,
    validate_rpg_authored_content_records,
};

pub fn infernal_attrition_content_records(
    policy: &InfernalAttritionPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_infernal_attrition_policy(policy)?;
    let policy_sha256 = infernal_attrition_policy_hash(policy)?;
    let mut records = Vec::with_capacity(8);
    records.push(content_record(
        EngineContentKind::Trigger,
        &policy.server_policy_id,
        "Infernal Attrition Policy",
        json!({
            "schema": RPG_INFERNAL_ATTRITION_SCHEMA,
            "source_issue": 1070,
            "content_migration_issue": 1455,
            "surface_kind": "infernal_attrition_policy",
            "circle_count": policy.circles.len(),
            "constitution_strain_per_loss": policy.constitution_strain_per_loss,
            "max_step_ticks": policy.max_step_ticks,
            "max_mitigation_percent": policy.max_mitigation_percent,
            "policy_sha256": policy_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);
    for profile in &policy.circles {
        records.push(content_record(
            EngineContentKind::Trigger,
            &format!(
                "server.ashen_reach.infernal_attrition.circle.{}",
                profile.circle_id.label()
            ),
            &format!("Infernal Attrition: {}", profile.circle_id.label()),
            json!({
                "schema": RPG_INFERNAL_ATTRITION_SCHEMA,
                "source_issue": 1070,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_attrition_circle_profile",
                "circle_id": profile.circle_id,
                "hazard": profile.hazard,
                "health_damage_per_tick": profile.health_damage_per_tick,
                "constitution_strain_per_tick": profile.constitution_strain_per_tick,
                "constitution_strain_per_loss": policy.constitution_strain_per_loss,
                "max_step_ticks": policy.max_step_ticks,
                "max_mitigation_percent": policy.max_mitigation_percent,
                "policy_sha256": policy_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "infernal_attrition.authored_content".to_string(),
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
            path: format!("editor://rpg/ashen_reach/infernal_attrition/{id}"),
            author: "editor_authoring.issue1455.infernal_attrition".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
