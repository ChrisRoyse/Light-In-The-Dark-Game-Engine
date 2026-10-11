use std::collections::BTreeMap;

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    EngineError,
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_semantic::{EngineCalyxSemanticIndex, build_engine_calyx_semantic_index},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use super::{QueryLoreCanonArgs, invalid_mcp_story, refs};
use crate::mcp_story_tools::{LoreEntry, LoreSurface, mcp_story_tools_store::McpStoryState};

#[derive(Debug, Clone)]
pub(super) struct CanonRecord {
    pub record_type: String,
    pub content_id: String,
    pub source_id: String,
    pub title: String,
    pub excerpt: String,
    pub source_refs: Vec<String>,
    pub linked_lore_entry_ids: Vec<String>,
}

pub(super) fn build_semantic_index(
    args: &QueryLoreCanonArgs,
    state: &McpStoryState,
) -> Result<(EngineCalyxSemanticIndex, BTreeMap<String, CanonRecord>), EngineError> {
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(
        &args.semantic_vault_path,
    ))
    .map_err(|error| invalid_mcp_story("lore_canon.semantic.open", error.to_string()))?;
    let store = EngineCalyxContentStore::new(database.clone());
    let mut catalog = BTreeMap::new();
    let mut readbacks = Vec::new();
    for entry in &state.lore_entries {
        let (record, canon) = entry_record(entry)?;
        readbacks.push(write_content(&store, &record)?);
        catalog.insert(canon.content_id.clone(), canon);
    }
    for surface in &state.lore_surfaces {
        let (record, canon) = surface_record(surface)?;
        readbacks.push(write_content(&store, &record)?);
        catalog.insert(canon.content_id.clone(), canon);
    }
    let index = build_engine_calyx_semantic_index(&args.semantic_index_id, &readbacks)
        .map_err(|error| invalid_mcp_story("lore_canon.semantic.index", error.to_string()))?;
    let persisted =
        litd_engine::calyx_semantic::store_engine_calyx_semantic_index(&database, &index)
            .map_err(|error| invalid_mcp_story("lore_canon.semantic.store", error.to_string()))?;
    Ok((persisted.index, catalog))
}

fn write_content(
    store: &EngineCalyxContentStore,
    record: &EngineContentRecord,
) -> Result<litd_engine::calyx_content::EngineContentConstellationReadback, EngineError> {
    store
        .put_content(record)
        .map_err(|error| invalid_mcp_story("lore_canon.content.write", error.to_string()))
}

fn entry_record(entry: &LoreEntry) -> Result<(EngineContentRecord, CanonRecord), EngineError> {
    let content_id = format!("lore_entry_{}", entry.entry_id);
    let payload = json!({
        "canon_record_type": "entry",
        "entry_id": entry.entry_id,
        "kind": entry.kind.as_str(),
        "title": entry.title,
        "summary": entry.summary,
        "body_rich_text": entry.body_rich_text,
        "metadata": entry.metadata,
        "source_refs": entry.source_refs,
        "tags": entry.tags,
        "links": entry.links,
        "timeline_markers": entry.timeline_markers
    });
    let canon = CanonRecord {
        record_type: "entry".to_string(),
        content_id: content_id.clone(),
        source_id: entry.entry_id.clone(),
        title: entry.title.clone(),
        excerpt: entry.summary.clone(),
        source_refs: refs("lore://entry", &entry.entry_id, &entry.source_refs),
        linked_lore_entry_ids: entry
            .links
            .iter()
            .map(|link| link.target_entry_id.clone())
            .collect(),
    };
    Ok((content_record(content_id, &entry.title, payload)?, canon))
}

fn surface_record(
    surface: &LoreSurface,
) -> Result<(EngineContentRecord, CanonRecord), EngineError> {
    let content_id = format!("lore_surface_{}", surface.surface_id);
    let payload = json!({
        "canon_record_type": "surface",
        "surface_id": surface.surface_id,
        "kind": surface.kind.as_str(),
        "title": surface.title,
        "body_rich_text": surface.body_rich_text,
        "excerpt": surface.excerpt,
        "lore_entry_ids": surface.lore_entry_ids,
        "world_ref": surface.world_ref,
        "codex_unlock_id": surface.codex_unlock_id,
        "discovery_trigger": surface.discovery_trigger,
        "source_refs": surface.source_refs,
        "tags": surface.tags
    });
    let canon = CanonRecord {
        record_type: "surface".to_string(),
        content_id: content_id.clone(),
        source_id: surface.surface_id.clone(),
        title: surface.title.clone(),
        excerpt: surface.excerpt.clone(),
        source_refs: refs("lore://surface", &surface.surface_id, &surface.source_refs),
        linked_lore_entry_ids: surface.lore_entry_ids.clone(),
    };
    Ok((content_record(content_id, &surface.title, payload)?, canon))
}

fn content_record(
    id: String,
    display_name: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let bytes = serde_json::to_vec(&payload).map_err(|error| EngineError::SerializationFailed {
        format: "lore-canon-content-json",
        reason: error.to_string(),
    })?;
    let source_path = format!("calyx://litd/lore-canon/{id}");
    Ok(EngineContentRecord {
        kind: EngineContentKind::Lore,
        id,
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: source_path,
            author: "litd-editor-lore-canon".to_string(),
            source_hash: sha256_hex(&bytes),
        },
        payload,
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1052_lore_canon_query".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    })
}
