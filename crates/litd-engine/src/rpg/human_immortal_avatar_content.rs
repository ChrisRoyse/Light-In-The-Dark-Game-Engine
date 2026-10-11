use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    HumanImmortalAvatarEffect, HumanImmortalAvatarPolicy, HumanImmortalAvatarReadback,
    RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA, rpg_authored_payload_without_nulls,
    validate_human_immortal_avatar_readback, validate_rpg_authored_content_records,
};

pub fn human_immortal_avatar_content_records(
    policy: &HumanImmortalAvatarPolicy,
    readback: &HumanImmortalAvatarReadback,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_human_immortal_avatar_readback(policy, readback)?;
    let mut records = vec![
        content_record(
            EngineContentKind::Agent,
            &readback.session_id,
            &format!("Human Immortal Avatar: {}", readback.public_identity),
            json!({
                "schema": RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA,
                "source_issue": 1045,
                "content_migration_issue": 1455,
                "surface_kind": "human_immortal_avatar_session",
                "readback": readback,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
        content_record(
            EngineContentKind::Story,
            &format!("ledger.human_immortal_avatar.{}", readback.session_id),
            "Human Immortal Avatar Audit Ledger",
            json!({
                "schema": RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA,
                "source_issue": 1045,
                "content_migration_issue": 1455,
                "surface_kind": "human_immortal_avatar_audit_ledger",
                "session_id": readback.session_id,
                "admin_id": readback.admin_id,
                "public_identity": readback.public_identity,
                "command_count": readback.command_count,
                "audit_ledger_sha256": readback.audit_ledger_sha256,
                "built_in_map_required": false,
                "grants_map_or_minimap": false,
                "grants_gps_markers": false,
            }),
        )?,
    ];
    for effect in &readback.lowered_effects {
        records.push(effect_record(effect)?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgImmortalInvalidConfig {
            field: "human_immortal_avatar.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn effect_record(effect: &HumanImmortalAvatarEffect) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Trigger,
        &effect.effect_id,
        "Human Immortal Avatar Effect",
        json!({
            "schema": RPG_HUMAN_IMMORTAL_AVATAR_SCHEMA,
            "source_issue": 1045,
            "content_migration_issue": 1455,
            "surface_kind": "human_immortal_avatar_effect",
            "effect": effect,
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
            path: format!("editor://rpg/ashen_reach/human_immortal_avatar/{id}"),
            author: "editor_authoring.issue1455.human_immortal_avatar".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
