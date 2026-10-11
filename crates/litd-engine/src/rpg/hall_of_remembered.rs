use super::serialize_sha256;
use serde_json::json;

use crate::EngineError;

use super::{
    HallMemorialDossier, HallMemorialReadback, HallMemorialRecord, HallOfRememberedPolicy,
    HallTribute, MortalityEventReadback, MortalityPolicy, RPG_HALL_OF_REMEMBERED_SCHEMA,
    hall_of_remembered_validate::{
        invalid, invalid_error, validate_dossier, validate_hall_memorial_readback,
        validate_hall_memorial_record, validate_hall_of_remembered_policy,
        validate_mortality_readback, validate_tributes,
    },
    mortality_policy_hash, validate_mortality_policy, validate_mortality_state,
};

pub fn record_hall_memorial(
    policy: &HallOfRememberedPolicy,
    mortality_policy: &MortalityPolicy,
    death: &MortalityEventReadback,
    dossier: &HallMemorialDossier,
    tributes: &[HallTribute],
) -> Result<HallMemorialReadback, EngineError> {
    validate_hall_of_remembered_policy(policy)?;
    validate_mortality_policy(mortality_policy)?;
    validate_mortality_state(&death.after_state, mortality_policy)?;
    validate_mortality_readback(death)?;
    validate_dossier(policy, dossier)?;
    validate_tributes(policy, tributes)?;

    let entry = death.mourning_wall_entry.as_ref().ok_or_else(|| {
        invalid_error(
            "death.mourning_wall_entry",
            "permanent death must include Mourning Wall handoff",
        )
    })?;
    if dossier.character_id != entry.character_id
        || dossier.character_name != entry.carved_name
        || dossier.race_id != entry.race_id
        || dossier.lifespan_years != entry.age_years
    {
        return invalid(
            "dossier",
            "dossier identity and lifespan must match the Mourning Wall entry",
        );
    }

    let memorial_id = format!(
        "memorial.hall.{}.{}",
        entry.character_id, entry.total_deaths
    );
    let record = HallMemorialRecord {
        memorial_id: memorial_id.clone(),
        mourning_wall_entry_id: entry.entry_id.clone(),
        character_id: entry.character_id.clone(),
        character_name: entry.carved_name.clone(),
        race_id: entry.race_id.clone(),
        class_id: dossier.class_id.clone(),
        faction_id: dossier.faction_id.clone(),
        lifespan_years: entry.age_years,
        final_constitution: entry.final_constitution,
        total_deaths: entry.total_deaths,
        cause: entry.cause,
        witness_id: entry.witness_id.clone(),
        epitaph: entry.epitaph.clone(),
        titles: dossier.titles.clone(),
        deeds: dossier.deeds.clone(),
        tributes: tributes.to_vec(),
        memorable_quotes: dossier.memorable_quotes.clone(),
        surfaced_in_galdmere: policy.browsable_in_galdmere,
        browsable_online: policy.browsable_online,
        persistent_across_server: policy.persistent_across_server,
        built_in_map_required: false,
        grants_map_or_minimap: false,
        grants_gps_markers: false,
        source_issue: policy.source_issue,
    };
    validate_hall_memorial_record(policy, &record)?;

    let mut readback = HallMemorialReadback {
        schema: RPG_HALL_OF_REMEMBERED_SCHEMA.to_string(),
        server_policy_id: policy.server_policy_id.clone(),
        source_issue: policy.source_issue,
        memorial_id,
        mourning_wall_entry_id: record.mourning_wall_entry_id.clone(),
        character_id: record.character_id.clone(),
        character_name: record.character_name.clone(),
        race_id: record.race_id.clone(),
        class_id: record.class_id.clone(),
        faction_id: record.faction_id.clone(),
        lifespan_years: record.lifespan_years,
        deed_count: record.deeds.len(),
        title_count: record.titles.len(),
        tribute_count: record.tributes.len(),
        memorable_quote_count: record.memorable_quotes.len(),
        surfaced_in_galdmere: record.surfaced_in_galdmere,
        browsable_online: record.browsable_online,
        persistent_across_server: record.persistent_across_server,
        built_in_map_required: record.built_in_map_required,
        grants_map_or_minimap: record.grants_map_or_minimap,
        grants_gps_markers: record.grants_gps_markers,
        mortality_result_sha256: death.result_sha256.clone(),
        mortality_policy_sha256: mortality_policy_hash(mortality_policy)?,
        policy_sha256: hall_of_remembered_policy_hash(policy)?,
        dossier_sha256: hall_memorial_dossier_hash(dossier)?,
        tributes_sha256: hall_tributes_hash(tributes)?,
        result_sha256: String::new(),
        record,
    };
    readback.result_sha256 = serialize_sha256(&readback_fingerprint(&readback))?;
    validate_hall_memorial_readback(policy, &readback)?;
    Ok(readback)
}

pub fn hall_of_remembered_policy_hash(
    policy: &HallOfRememberedPolicy,
) -> Result<String, EngineError> {
    serialize_sha256(policy)
}

pub fn hall_memorial_dossier_hash(dossier: &HallMemorialDossier) -> Result<String, EngineError> {
    serialize_sha256(dossier)
}

pub fn hall_tributes_hash(tributes: &[HallTribute]) -> Result<String, EngineError> {
    serialize_sha256(&tributes)
}

fn readback_fingerprint(readback: &HallMemorialReadback) -> serde_json::Value {
    json!({
        "schema": readback.schema,
        "server_policy_id": readback.server_policy_id,
        "source_issue": readback.source_issue,
        "memorial_id": readback.memorial_id,
        "mourning_wall_entry_id": readback.mourning_wall_entry_id,
        "character_id": readback.character_id,
        "class_id": readback.class_id,
        "faction_id": readback.faction_id,
        "lifespan_years": readback.lifespan_years,
        "deed_count": readback.deed_count,
        "title_count": readback.title_count,
        "tribute_count": readback.tribute_count,
        "memorable_quote_count": readback.memorable_quote_count,
        "surfaced_in_galdmere": readback.surfaced_in_galdmere,
        "browsable_online": readback.browsable_online,
        "persistent_across_server": readback.persistent_across_server,
        "built_in_map_required": readback.built_in_map_required,
        "grants_map_or_minimap": readback.grants_map_or_minimap,
        "grants_gps_markers": readback.grants_gps_markers,
        "mortality_result_sha256": readback.mortality_result_sha256,
        "mortality_policy_sha256": readback.mortality_policy_sha256,
        "policy_sha256": readback.policy_sha256,
        "dossier_sha256": readback.dossier_sha256,
        "tributes_sha256": readback.tributes_sha256,
    })
}
