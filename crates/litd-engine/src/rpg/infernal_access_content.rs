use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    InfernalAccessCostKind, InfernalAccessPolicy, RPG_INFERNAL_ACCESS_SCHEMA,
    infernal_access_policy_hash, rpg_authored_payload_without_nulls,
    validate_infernal_access_policy, validate_rpg_authored_content_records,
};

pub fn infernal_access_content_records(
    policy: &InfernalAccessPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_infernal_access_policy(policy)?;
    let policy_sha256 = infernal_access_policy_hash(policy)?;
    let mut records = Vec::with_capacity(7);
    records.push(content_record(
        EngineContentKind::Trigger,
        &policy.server_policy_id,
        "Infernal Access Policy",
        json!({
            "schema": RPG_INFERNAL_ACCESS_SCHEMA,
            "source_issue": 1066,
            "content_migration_issue": 1455,
            "surface_kind": "infernal_access_policy",
            "destination_circle": policy.destination_circle,
            "no_recall_inside": policy.no_recall_inside,
            "required_gate_id": policy.required_gate_id,
            "required_long_descent_ref": policy.required_long_descent_ref,
            "required_earned_cindermark_ref": policy.required_earned_cindermark_ref,
            "required_hollow_pact_faction_id": policy.required_hollow_pact_faction_id,
            "required_hollow_pact_power_id": policy.required_hollow_pact_power_id,
            "entry_rule_count": policy.entry_rules.len(),
            "denied_escape_kind_count": policy.denied_escape_kinds.len(),
            "policy_sha256": policy_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);
    for rule in &policy.entry_rules {
        records.push(content_record(
            EngineContentKind::Trigger,
            &format!(
                "server.ashen_reach.infernal_access.entry.{}",
                rule.path.label()
            ),
            &format!("Infernal Access Entry: {}", rule.path.label()),
            json!({
                "schema": RPG_INFERNAL_ACCESS_SCHEMA,
                "source_issue": 1066,
                "content_migration_issue": 1455,
                "surface_kind": "infernal_access_entry_rule",
                "entry_path": rule.path,
                "cost_kind": rule.cost_kind,
                "applies_hidden_debt": applies_hidden_debt(rule.cost_kind),
                "destination_circle": policy.destination_circle,
                "no_recall_inside": policy.no_recall_inside,
                "policy_sha256": policy_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?);
    }
    records.push(content_record(
        EngineContentKind::Trigger,
        "server.ashen_reach.infernal_access.escape_denials",
        "Infernal Access Escape Denials",
        json!({
            "schema": RPG_INFERNAL_ACCESS_SCHEMA,
            "source_issue": 1066,
            "content_migration_issue": 1455,
            "surface_kind": "infernal_access_escape_denials",
            "denied_escape_kinds": policy.denied_escape_kinds,
            "denial_reason": "infernal_no_recall_lock",
            "no_recall_inside": policy.no_recall_inside,
            "policy_sha256": policy_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "infernal_access.authored_content".to_string(),
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
            path: format!("editor://rpg/ashen_reach/infernal_access/{id}"),
            author: "editor_authoring.issue1455.infernal_access".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}

fn applies_hidden_debt(cost_kind: InfernalAccessCostKind) -> bool {
    !matches!(cost_kind, InfernalAccessCostKind::CindermarkTrialScar)
}
