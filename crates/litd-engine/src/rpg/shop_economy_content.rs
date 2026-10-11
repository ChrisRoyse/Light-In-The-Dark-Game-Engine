//! Editor-authored Calyx content emitter for the CF shopkeeper economy (#1687).
//!
//! Mirrors `item_specs_content.rs`; a shopkeeper is an NPC merchant agent, so it
//! persists under `EngineContentKind::Agent`.

use serde_json::{Value, json};

use super::serialize_sha256;

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::shop_economy::shopkeeper_readback;
use super::shop_economy_types::{RPG_SHOPKEEPER_SCHEMA, ShopkeeperSpec};
use super::{rpg_authored_payload_without_nulls, validate_rpg_authored_content_records};

pub fn shopkeeper_content_records(
    spec: &ShopkeeperSpec,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    let readback = shopkeeper_readback(spec)?;
    let records = vec![content_record(
        &spec.shopkeeper_id,
        &format!("Shopkeeper: {}", spec.display_name),
        json!({
            "schema": RPG_SHOPKEEPER_SCHEMA,
            "source_issue": spec.source_issue,
            "content_migration_issue": 1455,
            "surface_kind": "shopkeeper",
            "shopkeeper_id": spec.shopkeeper_id,
            "display_name": spec.display_name,
            "city_area_id": spec.city_area_id,
            "is_city": spec.is_city,
            "buy_markup_percent": spec.buy_markup_percent,
            "sell_payout_percent": spec.sell_payout_percent,
            "wealth_limit_coin": spec.wealth_limit_coin,
            "stock": spec.stock,
            "haggle": spec.haggle,
            "fence": spec.fence,
            "barter": spec.barter,
            "hours": spec.hours,
            "is_cartographer": spec.is_cartographer,
            "commerce_reputation": spec.commerce_reputation,
            "canon_issues": spec.canon_issues,
            "mechanics_source_url": spec.mechanics_source_url,
            "implementation_notes": spec.implementation_notes,
            "shopkeeper_sha256": readback.shopkeeper_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "shopkeeper.authored_content".to_string(),
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
        kind: EngineContentKind::Agent,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/shopkeepers/{id}"),
            author: "editor_authoring.issue1687.shopkeepers".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
