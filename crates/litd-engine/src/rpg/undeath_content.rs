use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    RPG_UNDEATH_SCHEMA, UndeadFormSpec, UndeathPolicy, rpg_authored_payload_without_nulls,
    undeath_policy_hash, validate_rpg_authored_content_records, validate_undeath_policy,
};

pub fn undeath_content_records(
    policy: &UndeathPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_undeath_policy(policy)?;
    let policy_sha256 = undeath_policy_hash(policy)?;
    let mut records = vec![
        content_record(
            EngineContentKind::Lore,
            &policy.server_policy_id,
            "Ashen Reach Undeath Policy",
            json!({
                "schema": RPG_UNDEATH_SCHEMA,
                "source_issue": 1061,
                "content_migration_issue": 1455,
                "surface_kind": "undeath_policy",
                "server_policy_id": policy.server_policy_id,
                "minimum_caster_level": policy.minimum_caster_level,
                "fixed_constitution_after_withering": policy.fixed_constitution_after_withering,
                "aging_death_immune": policy.aging_death_immune,
                "hunger_thirst_immune": policy.hunger_thirst_immune,
                "physical_kill_only": policy.physical_kill_only,
                "irreversible": policy.irreversible,
                "normal_resurrection_forbidden": policy.normal_resurrection_forbidden,
                "form_count": policy.forms.len(),
                "policy_sha256": policy_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Trigger,
            "server.ashen_reach.undeath.withering_requirements",
            "The Withering Requirements",
            json!({
                "schema": RPG_UNDEATH_SCHEMA,
                "source_issue": 1061,
                "content_migration_issue": 1455,
                "surface_kind": "withering_requirements",
                "server_policy_id": policy.server_policy_id,
                "allowed_dark_caster_class_ids": policy.allowed_dark_caster_class_ids,
                "required_discovery_ref_count": policy.required_discovery_ref_count,
                "required_components": policy.required_components,
                "accepted_irreversible_cost_required": true,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
    ];
    for form in &policy.forms {
        records.push(form_record(form, &policy.server_policy_id)?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "undeath.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn form_record(
    form: &UndeadFormSpec,
    server_policy_id: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Ability,
        &form.ritual.ability_id,
        &form.ritual.display_name,
        json!({
            "schema": RPG_UNDEATH_SCHEMA,
            "source_issue": 1061,
            "content_migration_issue": 1455,
            "surface_kind": "undeath_form_ritual",
            "server_policy_id": server_policy_id,
            "form": form.form,
            "required_focus_id": form.required_focus_id,
            "ritual": form.ritual,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
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
            path: format!("editor://rpg/ashen_reach/undeath/{id}"),
            author: "editor_authoring.issue1455.undeath".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
