use std::collections::BTreeSet;

use litd_hash::sha256_hex;

use crate::EngineError;

use super::area_specs_types::*;

pub fn area_spec_readback(spec: &AreaSpec) -> Result<AreaSpecReadback, EngineError> {
    validate_area_spec(spec)?;
    let gate_count = spec
        .rooms
        .iter()
        .filter(|room| room.room_kind == RoomKind::Gate)
        .count();
    let guildhall_count = spec
        .rooms
        .iter()
        .filter(|room| room.room_kind == RoomKind::Guildhall)
        .count();
    let has_sewers = spec
        .rooms
        .iter()
        .any(|room| room.room_kind == RoomKind::Sewer);
    let external_connection_count = spec
        .connections
        .iter()
        .filter(|conn| conn.to_area_id != spec.area_id)
        .count();
    let hidden_connection_count = spec
        .connections
        .iter()
        .filter(|conn| conn.discovery != ConnectionDiscovery::Visible)
        .count();
    Ok(AreaSpecReadback {
        schema: spec.schema.clone(),
        area_id: spec.area_id.clone(),
        display_name: spec.display_name.clone(),
        player_facing_canon_name: spec.player_facing_canon_name.clone(),
        area_kind: spec.area_kind,
        source_issue: spec.source_issue,
        level_band_min: spec.level_band.min_level,
        level_band_max: spec.level_band.max_level,
        room_count: spec.rooms.len(),
        connection_count: spec.connections.len(),
        gate_count,
        guildhall_count,
        external_connection_count,
        hidden_connection_count,
        has_sewers,
        is_hometown_option: spec.recall.is_hometown_option,
        has_academy: spec.recall.has_academy,
        has_recall_point: spec.recall.recall_room_id.is_some(),
        visual_renderer: spec.visual.renderer.clone(),
        prop_count: spec.visual.required_props.len(),
        variant_of_area_id: spec.variant_of_area_id.clone(),
        variant_label: spec.variant_label.clone(),
        area_spec_sha256: area_spec_hash(spec)?,
    })
}

pub fn validate_area_spec(spec: &AreaSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_AREA_SPEC_SCHEMA {
        return invalid("schema", format!("expected {RPG_AREA_SPEC_SCHEMA}"));
    }
    validate_token("area_id", &spec.area_id)?;
    validate_text("display_name", &spec.display_name)?;
    validate_text("player_facing_canon_name", &spec.player_facing_canon_name)?;
    if spec.source_issue == 0 {
        return invalid("source_issue", "must be non-zero");
    }
    if spec.canon_issues.is_empty() || !spec.canon_issues.contains(&spec.source_issue) {
        return invalid("canon_issues", "must include the source issue");
    }
    if !spec
        .mechanics_source_url
        .starts_with("https://wiki.carrionfields.net/")
    {
        return invalid(
            "mechanics_source_url",
            "must point to the approved factual mechanics source",
        );
    }
    validate_text("source_history_label", &spec.source_history_label)?;
    validate_text("summary", &spec.summary)?;
    validate_level_band(&spec.level_band)?;
    let room_ids = validate_rooms(&spec.rooms)?;
    validate_connections(spec, &room_ids)?;
    validate_reset(&spec.resets)?;
    validate_repop(&spec.repop)?;
    validate_recall(&spec.recall, &room_ids)?;
    validate_visual(&spec.visual)?;
    validate_variant_link(spec)?;
    if spec.implementation_notes.is_empty() {
        return invalid("implementation_notes", "must include source notes");
    }
    Ok(())
}

/// Cross-area referential integrity. `validate_area_spec` can only prove
/// intra-area soundness (from/to rooms exist in THIS area); there is no standalone
/// area registry yet (the #1631-style gap, tracked for #1632's world graph). When a
/// caller has assembled the set of known area ids, this pass flags any external
/// connection whose `to_area_id` is dangling.
pub fn validate_area_connections_against_registry(
    spec: &AreaSpec,
    known_area_ids: &BTreeSet<&str>,
) -> Result<(), EngineError> {
    validate_area_spec(spec)?;
    for conn in &spec.connections {
        if conn.to_area_id == spec.area_id {
            continue;
        }
        if !known_area_ids.contains(conn.to_area_id.as_str()) {
            return invalid(
                "connections.to_area_id",
                format!(
                    "connection {} references unknown area {}",
                    conn.connection_id, conn.to_area_id
                ),
            );
        }
    }
    Ok(())
}

fn validate_level_band(band: &AreaLevelBand) -> Result<(), EngineError> {
    if band.min_level == 0 || band.max_level == 0 {
        return invalid("level_band", "levels must be >= 1");
    }
    if band.min_level > band.max_level {
        return invalid("level_band", "min_level must be <= max_level");
    }
    if band.max_level > 200 {
        return invalid("level_band.max_level", "must be <= 200");
    }
    Ok(())
}

fn validate_rooms(rooms: &[AreaRoom]) -> Result<BTreeSet<String>, EngineError> {
    if rooms.is_empty() {
        return invalid("rooms", "at least one room is required");
    }
    let mut seen = BTreeSet::new();
    for room in rooms {
        validate_token("rooms.room_id", &room.room_id)?;
        validate_text("rooms.display_name", &room.display_name)?;
        validate_text("rooms.summary", &room.summary)?;
        for npc in &room.npc_population_ids {
            validate_token("rooms.npc_population_ids", npc)?;
        }
        if !seen.insert(room.room_id.clone()) {
            return invalid("rooms.room_id", format!("duplicate room {}", room.room_id));
        }
    }
    Ok(seen)
}

fn validate_connections(spec: &AreaSpec, room_ids: &BTreeSet<String>) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for conn in &spec.connections {
        validate_token("connections.connection_id", &conn.connection_id)?;
        validate_token("connections.to_area_id", &conn.to_area_id)?;
        if !room_ids.contains(&conn.from_room_id) {
            return invalid(
                "connections.from_room_id",
                format!(
                    "connection {} leaves from unknown room {}",
                    conn.connection_id, conn.from_room_id
                ),
            );
        }
        if let Some(to_room) = &conn.to_room_id {
            validate_token("connections.to_room_id", to_room)?;
            // Internal moves must land on a room that exists here.
            if conn.to_area_id == spec.area_id && !room_ids.contains(to_room) {
                return invalid(
                    "connections.to_room_id",
                    format!(
                        "connection {} lands on unknown local room {to_room}",
                        conn.connection_id
                    ),
                );
            }
        }
        if let Some(lock) = &conn.locked_by_ref {
            validate_token("connections.locked_by_ref", lock)?;
        }
        if !seen.insert(conn.connection_id.clone()) {
            return invalid(
                "connections.connection_id",
                format!("duplicate connection {}", conn.connection_id),
            );
        }
    }
    Ok(())
}

fn validate_reset(reset: &AreaResetProfile) -> Result<(), EngineError> {
    validate_token("resets.reset_id", &reset.reset_id)?;
    if reset.interval_minutes == 0 {
        return invalid("resets.interval_minutes", "must be non-zero");
    }
    Ok(())
}

fn validate_repop(repop: &AreaRepopProfile) -> Result<(), EngineError> {
    validate_token("repop.repop_id", &repop.repop_id)?;
    if repop.repop_interval_minutes == 0 {
        return invalid("repop.repop_interval_minutes", "must be non-zero");
    }
    Ok(())
}

fn validate_recall(
    recall: &AreaRecallProfile,
    room_ids: &BTreeSet<String>,
) -> Result<(), EngineError> {
    if let Some(room) = &recall.recall_room_id {
        validate_token("recall.recall_room_id", room)?;
        if !room_ids.contains(room) {
            return invalid(
                "recall.recall_room_id",
                format!("recall point {room} is not a room in this area"),
            );
        }
    }
    if let AreaAccessRule::Restricted { values } = &recall.hometown_access {
        if values.is_empty() {
            return invalid(
                "recall.hometown_access",
                "restricted access requires values",
            );
        }
        for value in values {
            validate_token("recall.hometown_access", value)?;
        }
    }
    Ok(())
}

fn validate_visual(visual: &AreaVisualContract) -> Result<(), EngineError> {
    if visual.renderer != "Rust/wgpu" {
        return invalid("visual.renderer", "must target Rust/wgpu");
    }
    validate_token("visual.tileset_id", &visual.tileset_id)?;
    validate_token("visual.model_id", &visual.model_id)?;
    if visual.required_props.len() < 3 {
        return invalid("visual.required_props", "must include core prop set");
    }
    let mut seen = BTreeSet::new();
    for prop in &visual.required_props {
        validate_token("visual.required_props", prop)?;
        if !seen.insert(prop.as_str()) {
            return invalid("visual.required_props", format!("duplicate {prop}"));
        }
    }
    validate_text("visual.silhouette", &visual.silhouette)?;
    validate_text("visual.material_notes", &visual.material_notes)?;
    Ok(())
}

/// Intra-area soundness of the #1725 region-variant link. The link and its label
/// are co-required (a variant must say what kind it is), the base id must be a
/// well-formed token, and an area may not be a variant of itself. The referential
/// check -- that the base area is actually authored -- is a CROSS-area property
/// and lives in the corpus registry (`area_registry.rs`), exactly as
/// cross-area connection targets are resolved there rather than here.
fn validate_variant_link(spec: &AreaSpec) -> Result<(), EngineError> {
    match (&spec.variant_of_area_id, &spec.variant_label) {
        (None, None) => Ok(()),
        (Some(base), Some(label)) => {
            validate_token("variant_of_area_id", base)?;
            validate_text("variant_label", label)?;
            if base == &spec.area_id {
                return invalid(
                    "variant_of_area_id",
                    "an area cannot be a variant of itself",
                );
            }
            Ok(())
        }
        (Some(_), None) => invalid(
            "variant_label",
            "variant_of_area_id requires a variant_label",
        ),
        (None, Some(_)) => invalid(
            "variant_of_area_id",
            "variant_label requires a variant_of_area_id",
        ),
    }
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

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    Ok(())
}

pub(crate) fn area_spec_hash(spec: &AreaSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize area spec: {error}"),
        })
}

/// Canonical normalization of a CF place-name into the local segment of an
/// `area.thera.*` id (#1749). ONE rule every authored batch must agree on, so a
/// place-name maps to the same id no matter which batch references it:
///
/// - ASCII letters are lowercased.
/// - Every non-alphanumeric character (apostrophe `'`/`’`, hyphen, whitespace,
///   comma, parenthesis, ...) is a token boundary that collapses to a single `_`.
///   (`Hamsah Mu'tazz` -> `hamsah_mu_tazz`, never `hamsah_mutazz`; `Delar-Tol` ->
///   `delar_tol`.)
/// - Consecutive boundaries collapse to one `_`; leading/trailing `_` are trimmed.
///
/// This is the guard against the #1735 drift (apostrophe silently dropped, or a
/// standalone area over-namespaced under a city). Batches that build ids via this
/// helper cannot reintroduce it. It normalizes only the local segment; callers
/// prepend the stable `area.thera.` prefix (a standalone area gets NO city
/// prefix).
pub fn canonical_area_local_id(source_name: &str) -> String {
    let mut out = String::with_capacity(source_name.len());
    let mut pending_sep = false;
    for ch in source_name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('_');
            }
            pending_sep = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            // Apostrophe, hyphen, whitespace, or any other punctuation: a token
            // boundary. Deferred so trailing boundaries never emit a stray `_`.
            pending_sep = true;
        }
    }
    out
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
