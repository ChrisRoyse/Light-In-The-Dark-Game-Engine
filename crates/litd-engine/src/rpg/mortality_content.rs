use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    MortalityPolicy, RPG_MORTALITY_SCHEMA, RaceLifespan, mortality_policy_hash,
    rpg_authored_payload_without_nulls, validate_mortality_policy,
    validate_rpg_authored_content_records,
};

pub fn mortality_content_records(
    policy: &MortalityPolicy,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_mortality_policy(policy)?;
    let policy_sha256 = mortality_policy_hash(policy)?;
    let mut records = vec![content_record(
        EngineContentKind::Lore,
        &policy.server_policy_id,
        "Ashen Reach Mortality Policy",
        json!({
            "schema": RPG_MORTALITY_SCHEMA,
            "source_issue": 1060,
            "content_migration_issue": 1455,
            "surface_kind": "mortality_policy",
            "server_policy_id": policy.server_policy_id,
            "deaths_per_constitution_loss": policy.deaths_per_constitution_loss,
            "permanent_death_constitution_threshold": policy.permanent_death_constitution_threshold,
            "constitution_hit_points": policy.constitution_hit_points,
            "constitution_regen_per_tick": policy.constitution_regen_per_tick,
            "death_threshold_base": policy.death_threshold_base,
            "death_threshold_per_constitution": policy.death_threshold_per_constitution,
            "mourning_wall_required": policy.mourning_wall_required,
            "race_lifespan_count": policy.race_lifespans.len(),
            "policy_sha256": policy_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?];
    for lifespan in &policy.race_lifespans {
        records.push(lifespan_record(lifespan, &policy.server_policy_id)?);
    }
    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgStatsInvalidConfig {
            field: "mortality.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn lifespan_record(
    lifespan: &RaceLifespan,
    server_policy_id: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Unit,
        &format!("mortality.lifespan.{}", lifespan.race_id),
        &format!("Mortality Lifespan: {}", lifespan.display_name),
        json!({
            "schema": RPG_MORTALITY_SCHEMA,
            "source_issue": 1060,
            "content_migration_issue": 1455,
            "surface_kind": "mortality_race_lifespan",
            "server_policy_id": server_policy_id,
            "race_id": lifespan.race_id,
            "display_name": lifespan.display_name,
            "adulthood_years": lifespan.adulthood_years,
            "expected_lifespan_years": lifespan.expected_lifespan_years,
            "maximum_lifespan_years": lifespan.maximum_lifespan_years,
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
            path: format!("editor://rpg/ashen_reach/mortality/{id}"),
            author: "editor_authoring.issue1455.mortality".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
