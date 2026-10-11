//! Editor-authored Calyx content emitter for the newbie OUTFIT kit (#1687).
//!
//! Mirrors `item_specs_content.rs`. The lowbie kit grants maps (helpfile 314);
//! those follow the #944 acquired-map doctrine -- an `archive_copy` acquired map
//! source with every HUD/marker/default-available flag false, so the outfit can
//! never mint free runtime map/GPS access.

use serde_json::{Value, json};

use super::serialize_sha256;

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::outfit_kit::outfit_kit_readback;
use super::outfit_kit_types::{OutfitBand, OutfitKitSpec, RPG_OUTFIT_KIT_SCHEMA};
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

pub fn outfit_kit_content_records(
    spec: &OutfitKitSpec,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = outfit_kit_readback(spec)?;
    let mut payload = json!({
        "schema": RPG_OUTFIT_KIT_SCHEMA,
        "source_issue": spec.source_issue,
        "content_migration_issue": 1455,
        "surface_kind": "outfit_kit",
        "outfit_id": spec.outfit_id,
        "display_name": spec.display_name,
        "band": spec.band.as_id(),
        "guild_gated": spec.band.guild_gated(),
        "gear": spec.gear,
        "provisions": spec.provisions,
        "maps": spec.maps,
        "only_fills_empty_slots": spec.only_fills_empty_slots,
        "breakage_prone": spec.breakage_prone,
        "recovery_floor_issue": spec.recovery_floor_issue,
        "canon_issues": spec.canon_issues,
        "mechanics_source_url": spec.mechanics_source_url,
        "implementation_notes": spec.implementation_notes,
        "outfit_kit_sha256": readback.outfit_kit_sha256,
        "built_in_map_required": false,
        "grants_map_or_minimap": false,
        "grants_gps_markers": false,
    });
    // The lowbie kit hands out physical map copies (helpfile 314); represent them
    // via the #944 acquired-map doctrine as diegetic archive copies, never a HUD.
    if spec.band == OutfitBand::LowbieAnywhere && !spec.maps.is_empty() {
        payload["acquired_map_source"] = json!({
            "source_kind": "archive_copy",
            "scope_id": format!("{}.starter_maps", spec.outfit_id),
            "requires_possession_or_knowledge": true,
            "default_available": false,
            "player_position_indicator": false,
            "quest_markers": false,
            "route_lines": false,
            "minimap_hud": false,
        });
    }
    let records = vec![content_record(
        &spec.outfit_id,
        &format!("Outfit Kit: {}", spec.display_name),
        payload,
    )?];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "outfit_kit.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn content_record(
    id: &str,
    display_name: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind: EngineContentKind::Item,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/outfit_kits/{id}"),
            author: "editor_authoring.issue1687.outfit_kits".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
