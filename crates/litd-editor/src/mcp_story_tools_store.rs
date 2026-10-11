use litd_hash::sha256_hex;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

use super::{
    CinematicStage, DialogueTree, LoreContext, LoreEntry, LoreSurface, QuestArc, StoryCharacter,
    invalid_mcp_story,
    mcp_story_tools_lore_surface::validate_lore_surface_state,
    mcp_story_tools_validation::{
        validate_character, validate_cinematic, validate_dialogue, validate_lore,
        validate_lore_entry_state, validate_quest,
    },
};

pub const MCP_STORY_STATE_SCHEMA: &str = "litd.mcp.story_tools.state.v1";
pub const MCP_STORY_LEDGER_SCHEMA: &str = "litd.mcp.story_tools.ledger.v1";
const MCP_STORY_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC0";
const MCP_STORY_VAULT_PURPOSE: &[u8] = b"litd-mcp-story-tools-issue1360";
const MCP_STORY_COLLECTION: &str = "litd_mcp_story_tools";
const MCP_STORY_STATE_ROW: &[u8] = b"story/state/latest";
const MCP_STORY_LEDGER_ROW: &[u8] = b"story/ledger/latest";
const MCP_STORY_STATE_COLUMN: &[u8] = b"state_json";
const MCP_STORY_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStoryState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub update_count: u64,
    pub characters: Vec<StoryCharacter>,
    pub dialogues: Vec<DialogueTree>,
    pub quests: Vec<QuestArc>,
    pub cinematics: Vec<CinematicStage>,
    pub lore_contexts: Vec<LoreContext>,
    #[serde(default)]
    pub lore_entries: Vec<LoreEntry>,
    #[serde(default)]
    pub lore_surfaces: Vec<LoreSurface>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStoryStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state_fingerprint: String,
    pub state: McpStoryState,
    pub character_count: usize,
    pub dialogue_count: usize,
    pub dialogue_node_count: usize,
    pub quest_count: usize,
    pub quest_objective_count: usize,
    pub cinematic_count: usize,
    pub cinematic_beat_count: usize,
    pub lore_context_count: usize,
    pub lore_entry_count: usize,
    pub lore_entry_link_count: usize,
    pub lore_timeline_marker_count: usize,
    pub lore_kind_counts: BTreeMap<String, usize>,
    pub lore_surface_count: usize,
    pub lore_surface_lore_ref_count: usize,
    pub lore_surface_codex_unlock_count: usize,
    pub lore_surface_kind_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStoryToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpStoryToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStoryToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub state_path: String,
    pub state_sha256: String,
    pub state_bytes: u64,
    pub action_summary: String,
    pub affected_ids: Vec<String>,
    pub character_count: usize,
    pub dialogue_count: usize,
    pub quest_count: usize,
    pub cinematic_count: usize,
    pub lore_context_count: usize,
    #[serde(default)]
    pub lore_entry_count: usize,
    #[serde(default)]
    pub lore_entry_link_count: usize,
    #[serde(default)]
    pub lore_timeline_marker_count: usize,
    #[serde(default)]
    pub lore_surface_count: usize,
    #[serde(default)]
    pub lore_surface_lore_ref_count: usize,
    #[serde(default)]
    pub lore_surface_codex_unlock_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStoryToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpStoryToolLedger,
}

impl McpStoryState {
    pub fn new(state_id: impl Into<String>) -> Result<Self, EngineError> {
        let state = Self {
            schema: MCP_STORY_STATE_SCHEMA.to_string(),
            issue: 921,
            state_id: state_id.into(),
            source_of_truth: "Calyx Aster MCP story/lore state row".to_string(),
            update_count: 0,
            characters: Vec::new(),
            dialogues: Vec::new(),
            quests: Vec::new(),
            cinematics: Vec::new(),
            lore_contexts: Vec::new(),
            lore_entries: Vec::new(),
            lore_surfaces: Vec::new(),
        };
        validate_mcp_story_state(&state)?;
        Ok(state)
    }
}

pub fn load_or_create_mcp_story_state(
    path: impl AsRef<Path>,
    state_id: impl Into<String>,
) -> Result<McpStoryState, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let state_id = state_id.into();
    if path.exists() {
        let state = read_mcp_story_state(path)?.state;
        if state.state_id != state_id {
            return Err(store_error("state_id", "state_id mismatch"));
        }
        Ok(state)
    } else {
        McpStoryState::new(state_id)
    }
}

pub fn write_mcp_story_state(
    path: impl AsRef<Path>,
    state: &McpStoryState,
) -> Result<McpStoryStateReadback, EngineError> {
    validate_mcp_story_state(state)?;
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-story-state-json",
            reason: format!("serialize story state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&story_state_spec(path), &bytes)
        .map_err(|error| store_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "state.write",
            "Aster story state bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_story_state(path)?;
    if readback.state != *state {
        return Err(store_error("state.readback", "state readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_story_state(path: impl AsRef<Path>) -> Result<McpStoryStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let readback = read_calyx_json_bytes(&story_state_spec(path))
        .map_err(|error| store_error("state.read", error))?
        .ok_or_else(|| {
            store_error(
                "state.read",
                format!("missing Calyx Aster story state row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let state: McpStoryState =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-story-state-json",
            reason: format!("parse story state {}: {error}", path.display()),
        })?;
    readback_from_state(path, bytes.len() as u64, sha256_hex(&bytes), state)
}

pub fn write_mcp_story_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpStoryToolRecord,
) -> Result<McpStoryToolLedgerReadback, EngineError> {
    let ledger = McpStoryToolLedger {
        schema: MCP_STORY_LEDGER_SCHEMA.to_string(),
        issue: 921,
        ledger_id: ledger_id.into(),
        source_of_truth: "Calyx Aster MCP story tool ledger row + story state row".to_string(),
        record_count: 1,
        records: vec![record],
    };
    validate_mcp_story_tool_ledger(&ledger)?;
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-story-ledger-json",
            reason: format!("serialize story ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&story_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster story ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_story_tool_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error("ledger.readback", "ledger readback mismatch"));
    }
    Ok(readback)
}

pub fn read_mcp_story_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpStoryToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&story_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!("missing Calyx Aster story ledger row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpStoryToolLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-story-ledger-json",
            reason: format!("parse story ledger {}: {error}", path.display()),
        })?;
    validate_mcp_story_tool_ledger(&ledger)?;
    Ok(McpStoryToolLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn validate_mcp_story_state(state: &McpStoryState) -> Result<(), EngineError> {
    if state.schema != MCP_STORY_STATE_SCHEMA
        || state.issue != 921
        || state.state_id.trim().is_empty()
        || state.source_of_truth.trim().is_empty()
    {
        return Err(store_error(
            "state",
            "schema, issue, and identity are invalid",
        ));
    }
    let mut character_ids = BTreeSet::new();
    for character in &state.characters {
        validate_character(character)?;
        if !character_ids.insert(character.character_id.as_str()) {
            return Err(store_error(
                "state.characters",
                format!("duplicate character {}", character.character_id),
            ));
        }
    }
    let mut dialogue_ids = BTreeSet::new();
    for dialogue in &state.dialogues {
        validate_dialogue(state, dialogue)?;
        if !dialogue_ids.insert(dialogue.dialogue_id.as_str()) {
            return Err(store_error(
                "state.dialogues",
                format!("duplicate dialogue {}", dialogue.dialogue_id),
            ));
        }
    }
    let mut quest_ids = BTreeSet::new();
    for quest in &state.quests {
        validate_quest(state, quest)?;
        if !quest_ids.insert(quest.quest_id.as_str()) {
            return Err(store_error(
                "state.quests",
                format!("duplicate quest {}", quest.quest_id),
            ));
        }
    }
    let mut cinematic_ids = BTreeSet::new();
    for cinematic in &state.cinematics {
        validate_cinematic(state, cinematic)?;
        if !cinematic_ids.insert(cinematic.cinematic_id.as_str()) {
            return Err(store_error(
                "state.cinematics",
                format!("duplicate cinematic {}", cinematic.cinematic_id),
            ));
        }
    }
    let mut lore_ids = BTreeSet::new();
    for lore in &state.lore_contexts {
        validate_lore(lore)?;
        if !lore_ids.insert(lore.lore_id.as_str()) {
            return Err(store_error(
                "state.lore_contexts",
                format!("duplicate lore {}", lore.lore_id),
            ));
        }
    }
    validate_lore_entry_state(state)?;
    validate_lore_surface_state(state)?;
    Ok(())
}

pub fn validate_mcp_story_tool_ledger(ledger: &McpStoryToolLedger) -> Result<(), EngineError> {
    if ledger.schema != MCP_STORY_LEDGER_SCHEMA
        || ledger.issue != 921
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.record_count != ledger.records.len()
        || ledger.records.is_empty()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and count are invalid",
        ));
    }
    for record in &ledger.records {
        if record.request_id.trim().is_empty()
            || record.requested_by.trim().is_empty()
            || record.state_sha256.len() != 64
            || record.state_bytes == 0
            || record.affected_ids.is_empty()
        {
            return Err(store_error("record", "required record fields are invalid"));
        }
    }
    Ok(())
}

fn readback_from_state(
    path: &Path,
    bytes: u64,
    sha256: String,
    state: McpStoryState,
) -> Result<McpStoryStateReadback, EngineError> {
    validate_mcp_story_state(&state)?;
    Ok(McpStoryStateReadback {
        path: path.display().to_string(),
        bytes,
        sha256,
        state_fingerprint: stable_fingerprint(&state)?,
        character_count: state.characters.len(),
        dialogue_count: state.dialogues.len(),
        dialogue_node_count: state
            .dialogues
            .iter()
            .map(|dialogue| dialogue.nodes.len())
            .sum(),
        quest_count: state.quests.len(),
        quest_objective_count: state
            .quests
            .iter()
            .map(|quest| quest.objectives.len())
            .sum(),
        cinematic_count: state.cinematics.len(),
        cinematic_beat_count: state.cinematics.iter().map(|stage| stage.beats.len()).sum(),
        lore_context_count: state.lore_contexts.len(),
        lore_entry_count: state.lore_entries.len(),
        lore_entry_link_count: state
            .lore_entries
            .iter()
            .map(|entry| entry.links.len())
            .sum(),
        lore_timeline_marker_count: state
            .lore_entries
            .iter()
            .map(|entry| entry.timeline_markers.len())
            .sum(),
        lore_kind_counts: lore_kind_counts(&state),
        lore_surface_count: state.lore_surfaces.len(),
        lore_surface_lore_ref_count: state
            .lore_surfaces
            .iter()
            .map(|surface| surface.lore_entry_ids.len())
            .sum(),
        lore_surface_codex_unlock_count: state
            .lore_surfaces
            .iter()
            .filter(|surface| surface.codex_unlock_id.is_some())
            .count(),
        lore_surface_kind_counts: lore_surface_kind_counts(&state),
        state,
    })
}

fn lore_kind_counts(state: &McpStoryState) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in &state.lore_entries {
        *counts.entry(entry.kind.as_str().to_string()).or_insert(0) += 1;
    }
    counts
}

fn lore_surface_kind_counts(state: &McpStoryState) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for surface in &state.lore_surfaces {
        *counts.entry(surface.kind.as_str().to_string()).or_insert(0) += 1;
    }
    counts
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(store_error(field, "path must be non-empty"));
    }
    if path.is_file() {
        return Err(store_error(
            field,
            format!(
                "{} is a file; expected a Calyx Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn story_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    story_spec(path, MCP_STORY_STATE_ROW, MCP_STORY_STATE_COLUMN)
}

fn story_ledger_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    story_spec(path, MCP_STORY_LEDGER_ROW, MCP_STORY_LEDGER_COLUMN)
}

fn story_spec<'a>(path: &'a Path, row_key: &'a [u8], column: &'a [u8]) -> CalyxJsonStoreSpec<'a> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: MCP_STORY_VAULT_ID,
        purpose: MCP_STORY_VAULT_PURPOSE,
        collection: MCP_STORY_COLLECTION,
        row_key,
        column,
    }
}

fn store_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    invalid_mcp_story(format!("store.{}", field.into()), reason)
}
