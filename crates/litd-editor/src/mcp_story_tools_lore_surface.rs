use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    LoreSurface, LoreSurfaceKind, invalid_mcp_story, mcp_story_tools_store::McpStoryState,
};

const MAX_TEXT_LEN: usize = 4096;
const MAX_RICH_TEXT_LEN: usize = 16384;
const MAX_LIST_LEN: usize = 128;

pub(super) fn validate_lore_surface_batch(
    state: &McpStoryState,
    surfaces: &[LoreSurface],
) -> Result<(), EngineError> {
    if surfaces.is_empty() {
        return Err(invalid_mcp_story(
            "lore_surface.surfaces",
            "at least one lore surface is required",
        ));
    }
    validate_lore_surface_collection(state, surfaces)
}

pub(super) fn validate_lore_surface_state(state: &McpStoryState) -> Result<(), EngineError> {
    validate_lore_surface_collection(state, &state.lore_surfaces)
}

fn validate_lore_surface_collection(
    state: &McpStoryState,
    surfaces: &[LoreSurface],
) -> Result<(), EngineError> {
    if surfaces.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            "lore_surface.surfaces",
            format!("surface count must be <= {MAX_LIST_LEN}"),
        ));
    }
    let lore_ids = state
        .lore_entries
        .iter()
        .map(|entry| entry.entry_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut surface_ids = BTreeSet::new();
    for surface in surfaces {
        validate_lore_surface_shape(surface)?;
        if !surface_ids.insert(surface.surface_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_surface.surface_id",
                format!("duplicate lore surface {}", surface.surface_id),
            ));
        }
        for lore_entry_id in &surface.lore_entry_ids {
            if !lore_ids.contains(lore_entry_id.as_str()) {
                return Err(invalid_mcp_story(
                    "lore_surface.lore_entry_ids",
                    format!(
                        "{} references missing lore entry {}",
                        surface.surface_id, lore_entry_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_lore_surface_shape(surface: &LoreSurface) -> Result<(), EngineError> {
    validate_symbol("lore_surface.surface_id", &surface.surface_id)?;
    validate_single_line("lore_surface.title", &surface.title)?;
    validate_rich_text("lore_surface.body_rich_text", &surface.body_rich_text)?;
    validate_single_line("lore_surface.excerpt", &surface.excerpt)?;
    validate_single_line("lore_surface.world_ref", &surface.world_ref)?;
    validate_single_line("lore_surface.discovery_trigger", &surface.discovery_trigger)?;
    if surface.lore_entry_ids.is_empty() || surface.lore_entry_ids.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            "lore_surface.lore_entry_ids",
            format!("lore entry ref count must be in 1..={MAX_LIST_LEN}"),
        ));
    }
    validate_symbol_list("lore_surface.lore_entry_ids", &surface.lore_entry_ids)?;
    validate_symbol_list("lore_surface.source_refs", &surface.source_refs)?;
    validate_symbol_list("lore_surface.tags", &surface.tags)?;
    validate_codex(surface)
}

fn validate_codex(surface: &LoreSurface) -> Result<(), EngineError> {
    if matches!(surface.kind, LoreSurfaceKind::CodexEntry) && surface.codex_unlock_id.is_none() {
        return Err(invalid_mcp_story(
            "lore_surface.codex_unlock_id",
            "codex_entry surfaces require a codex_unlock_id",
        ));
    }
    if let Some(unlock_id) = &surface.codex_unlock_id {
        validate_symbol("lore_surface.codex_unlock_id", unlock_id)?;
    }
    Ok(())
}

fn validate_rich_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_RICH_TEXT_LEN || value.contains('\0') {
        return Err(invalid_mcp_story(
            field,
            "rich text must be bounded non-empty text",
        ));
    }
    let lower = value.to_ascii_lowercase();
    for denied in [
        "<script",
        "<iframe",
        "<object",
        "<embed",
        "javascript:",
        "data:text/html",
        "onerror=",
        "onload=",
    ] {
        if lower.contains(denied) {
            return Err(invalid_mcp_story(
                field,
                format!("active rich-text content is not allowed: {denied}"),
            ));
        }
    }
    Ok(())
}

fn validate_symbol_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.is_empty() || values.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            field,
            format!("list length must be in 1..={MAX_LIST_LEN}"),
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_symbol(field, value)?;
        if !seen.insert(value.as_str()) {
            return Err(invalid_mcp_story(field, format!("duplicate value {value}")));
        }
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_story(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_mcp_story(field, "symbol cannot be empty"));
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(invalid_mcp_story(
            field,
            "symbol must be an ASCII identifier",
        ));
    }
    Ok(())
}
