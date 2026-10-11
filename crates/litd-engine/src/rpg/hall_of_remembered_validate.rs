use std::collections::BTreeSet;

use crate::EngineError;

use super::{
    HallDeed, HallMemorialDossier, HallMemorialReadback, HallMemorialRecord,
    HallOfRememberedPolicy, HallTribute, MortalityEventReadback, RPG_HALL_OF_REMEMBERED_SCHEMA,
    RPG_MORTALITY_SCHEMA,
};

pub fn validate_hall_of_remembered_policy(
    policy: &HallOfRememberedPolicy,
) -> Result<(), EngineError> {
    if policy.schema != RPG_HALL_OF_REMEMBERED_SCHEMA {
        return invalid(
            "policy.schema",
            format!("expected {RPG_HALL_OF_REMEMBERED_SCHEMA}"),
        );
    }
    validate_token("policy.server_policy_id", &policy.server_policy_id)?;
    if policy.source_issue != 1065 {
        return invalid("policy.source_issue", "must point at issue 1065");
    }
    if policy.built_in_map_required || policy.grants_map_or_minimap || policy.grants_gps_markers {
        return invalid(
            "policy.map_flags",
            "Hall memorial browsing cannot require or grant built-in maps/minimaps/GPS",
        );
    }
    if !policy.persistent_across_server
        || !policy.auto_populate_from_character_record
        || !policy.opt_in_player_tributes_only
        || !policy.browsable_in_galdmere
        || !policy.browsable_online
    {
        return invalid(
            "policy.browse_persistence_flags",
            "Hall records must persist, auto-populate, use opt-in tributes, and browse in Galdmere plus online",
        );
    }
    validate_token("policy.galdmere_surface_id", &policy.galdmere_surface_id)?;
    validate_token("policy.online_surface_id", &policy.online_surface_id)?;
    if policy.galdmere_surface_id != "surface.galdmere.mourning_wall" {
        return invalid(
            "policy.galdmere_surface_id",
            "Hall must surface through the Galdmere Mourning Wall",
        );
    }
    if policy.online_surface_id != "surface.online.hall_of_remembered" {
        return invalid(
            "policy.online_surface_id",
            "Hall must expose the online memorial browser",
        );
    }
    if !(1..=16).contains(&policy.minimum_deed_count) {
        return invalid("policy.minimum_deed_count", "must be inside 1..=16");
    }
    if !(1..=128).contains(&policy.maximum_tribute_count) {
        return invalid("policy.maximum_tribute_count", "must be inside 1..=128");
    }
    Ok(())
}

pub fn validate_hall_memorial_readback(
    policy: &HallOfRememberedPolicy,
    readback: &HallMemorialReadback,
) -> Result<(), EngineError> {
    validate_hall_of_remembered_policy(policy)?;
    if readback.schema != RPG_HALL_OF_REMEMBERED_SCHEMA {
        return invalid(
            "readback.schema",
            format!("expected {RPG_HALL_OF_REMEMBERED_SCHEMA}"),
        );
    }
    if readback.server_policy_id != policy.server_policy_id || readback.source_issue != 1065 {
        return invalid(
            "readback.policy_ref",
            "readback must point at the active Hall policy",
        );
    }
    validate_hall_memorial_record(policy, &readback.record)?;
    if readback.memorial_id != readback.record.memorial_id
        || readback.character_id != readback.record.character_id
        || readback.mourning_wall_entry_id != readback.record.mourning_wall_entry_id
        || readback.deed_count != readback.record.deeds.len()
        || readback.title_count != readback.record.titles.len()
        || readback.tribute_count != readback.record.tributes.len()
        || readback.memorable_quote_count != readback.record.memorable_quotes.len()
    {
        return invalid(
            "readback.record",
            "summary fields must match the memorial record",
        );
    }
    if readback.built_in_map_required
        || readback.grants_map_or_minimap
        || readback.grants_gps_markers
    {
        return invalid(
            "readback.map_flags",
            "Hall readback cannot expose map/minimap/GPS grants",
        );
    }
    for (field, value) in [
        (
            "readback.mortality_result_sha256",
            &readback.mortality_result_sha256,
        ),
        (
            "readback.mortality_policy_sha256",
            &readback.mortality_policy_sha256,
        ),
        ("readback.policy_sha256", &readback.policy_sha256),
        ("readback.dossier_sha256", &readback.dossier_sha256),
        ("readback.tributes_sha256", &readback.tributes_sha256),
        ("readback.result_sha256", &readback.result_sha256),
    ] {
        validate_hash(field, value)?;
    }
    Ok(())
}

pub fn validate_hall_memorial_record(
    policy: &HallOfRememberedPolicy,
    record: &HallMemorialRecord,
) -> Result<(), EngineError> {
    validate_token("record.memorial_id", &record.memorial_id)?;
    validate_token(
        "record.mourning_wall_entry_id",
        &record.mourning_wall_entry_id,
    )?;
    validate_token("record.character_id", &record.character_id)?;
    validate_text("record.character_name", &record.character_name, 128)?;
    validate_token("record.race_id", &record.race_id)?;
    validate_token("record.class_id", &record.class_id)?;
    validate_token("record.faction_id", &record.faction_id)?;
    if record.lifespan_years == 0 || record.lifespan_years > 10_000 {
        return invalid("record.lifespan_years", "must be inside 1..=10000");
    }
    if !(1..=100).contains(&record.final_constitution) {
        return invalid("record.final_constitution", "must be inside 1..=100");
    }
    validate_text("record.epitaph", &record.epitaph, 240)?;
    if record.source_issue != 1065 {
        return invalid("record.source_issue", "must point at issue 1065");
    }
    if record.built_in_map_required || record.grants_map_or_minimap || record.grants_gps_markers {
        return invalid("record.map_flags", "memorial records cannot grant maps/GPS");
    }
    if !record.surfaced_in_galdmere || !record.browsable_online || !record.persistent_across_server
    {
        return invalid(
            "record.surface_flags",
            "memorial must persist and be browsable in Galdmere plus online",
        );
    }
    validate_titles(&record.titles)?;
    validate_deeds(policy, &record.deeds)?;
    validate_tributes(policy, &record.tributes)?;
    validate_text_vec("record.memorable_quotes", &record.memorable_quotes, 16, 180)
}

pub(super) fn validate_mortality_readback(
    readback: &MortalityEventReadback,
) -> Result<(), EngineError> {
    if readback.schema != RPG_MORTALITY_SCHEMA {
        return invalid(
            "death.schema",
            format!("expected mortality schema {RPG_MORTALITY_SCHEMA}"),
        );
    }
    if !readback.permanently_dead || readback.permanent_death_reason.is_none() {
        return invalid(
            "death.permanently_dead",
            "Hall records are created only from permanent deaths",
        );
    }
    let Some(entry) = readback.mourning_wall_entry.as_ref() else {
        return invalid(
            "death.mourning_wall_entry",
            "permanent death must include Mourning Wall handoff",
        );
    };
    if readback.after_state.mourning_wall_entry_id.as_deref() != Some(entry.entry_id.as_str())
        || readback.after_state.character_id != entry.character_id
        || readback.after_state.character_name != entry.carved_name
        || readback.after_state.race_id != entry.race_id
        || readback.total_deaths_after != entry.total_deaths
        || entry.source_issue != 1060
    {
        return invalid(
            "death.mourning_wall_entry",
            "Mourning Wall entry must match the mortality readback",
        );
    }
    validate_hash("death.result_sha256", &readback.result_sha256)
}

pub(super) fn validate_dossier(
    policy: &HallOfRememberedPolicy,
    dossier: &HallMemorialDossier,
) -> Result<(), EngineError> {
    validate_token("dossier.character_id", &dossier.character_id)?;
    validate_text("dossier.character_name", &dossier.character_name, 128)?;
    validate_token("dossier.race_id", &dossier.race_id)?;
    validate_token("dossier.class_id", &dossier.class_id)?;
    validate_token("dossier.faction_id", &dossier.faction_id)?;
    if dossier.lifespan_years == 0 || dossier.lifespan_years > 10_000 {
        return invalid("dossier.lifespan_years", "must be inside 1..=10000");
    }
    validate_titles(&dossier.titles)?;
    validate_deeds(policy, &dossier.deeds)?;
    validate_text_vec(
        "dossier.memorable_quotes",
        &dossier.memorable_quotes,
        16,
        180,
    )
}

pub(super) fn validate_tributes(
    policy: &HallOfRememberedPolicy,
    tributes: &[HallTribute],
) -> Result<(), EngineError> {
    if tributes.len() > policy.maximum_tribute_count {
        return invalid("tributes", "too many public tributes");
    }
    let mut seen = BTreeSet::new();
    for tribute in tributes {
        validate_token("tributes.tribute_id", &tribute.tribute_id)?;
        validate_token("tributes.author_player_id", &tribute.author_player_id)?;
        validate_token("tributes.author_character_id", &tribute.author_character_id)?;
        validate_text("tributes.text", &tribute.text, 240)?;
        validate_token("tributes.source_ref", &tribute.source_ref)?;
        if !tribute.opted_in_publicly {
            return invalid(
                "tributes.opted_in_publicly",
                "player-written tributes must be opt-in before becoming public",
            );
        }
        unique(
            &mut seen,
            tribute.tribute_id.as_str(),
            "tributes.tribute_id",
        )?;
    }
    Ok(())
}

fn validate_titles(titles: &[String]) -> Result<(), EngineError> {
    validate_text_vec("titles", titles, 32, 96)
}

fn validate_deeds(policy: &HallOfRememberedPolicy, deeds: &[HallDeed]) -> Result<(), EngineError> {
    if deeds.len() < policy.minimum_deed_count || deeds.len() > 64 {
        return invalid(
            "deeds",
            format!("must contain {}..=64 deeds", policy.minimum_deed_count),
        );
    }
    let mut seen = BTreeSet::new();
    for deed in deeds {
        validate_token("deeds.deed_id", &deed.deed_id)?;
        validate_text("deeds.summary", &deed.summary, 180)?;
        validate_token("deeds.source_ref", &deed.source_ref)?;
        if deed.prestige_milli == 0 || deed.prestige_milli > 1000 {
            return invalid("deeds.prestige_milli", "must be inside 1..=1000");
        }
        unique(&mut seen, deed.deed_id.as_str(), "deeds.deed_id")?;
    }
    Ok(())
}

fn validate_text_vec(
    field: &str,
    values: &[String],
    max_count: usize,
    max_len: usize,
) -> Result<(), EngineError> {
    if values.len() > max_count {
        return invalid(field, format!("must contain at most {max_count} values"));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_text(field, value, max_len)?;
        unique(&mut seen, value.as_str(), field)?;
    }
    Ok(())
}

fn unique<T: Ord + Copy>(seen: &mut BTreeSet<T>, value: T, field: &str) -> Result<(), EngineError> {
    if !seen.insert(value) {
        return invalid(field, "duplicate value");
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn validate_hash(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid(field, "must be a SHA-256 hex string");
    }
    Ok(())
}

fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > max_len {
        return invalid(field, format!("must be <= {max_len} bytes"));
    }
    Ok(())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(invalid_error(field, reason))
}

pub(super) fn invalid_error(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("hall_of_remembered.{}", field.into()),
        reason: reason.to_string(),
    }
}
