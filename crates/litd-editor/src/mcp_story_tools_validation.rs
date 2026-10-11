use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    CinematicStage, DialogueTree, LoreContext, LoreEntry, LoreEntryKind, QuestArc, StoryCharacter,
    invalid_mcp_story, mcp_story_tools_store::McpStoryState,
};

const MAX_TEXT_LEN: usize = 4096;
const MAX_RICH_TEXT_LEN: usize = 16384;
const MAX_LIST_LEN: usize = 128;

pub(super) fn validate_request_identity(
    request_id: &str,
    requested_by: &str,
) -> Result<(), EngineError> {
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)
}

pub(super) fn validate_character(character: &StoryCharacter) -> Result<(), EngineError> {
    validate_symbol("character.character_id", &character.character_id)?;
    validate_single_line("character.display_name", &character.display_name)?;
    validate_single_line("character.role", &character.role)?;
    validate_symbol_list("character.voice_tags", &character.voice_tags)
}

pub(super) fn validate_dialogue(
    state: &McpStoryState,
    dialogue: &DialogueTree,
) -> Result<(), EngineError> {
    validate_symbol("dialogue.dialogue_id", &dialogue.dialogue_id)?;
    validate_single_line("dialogue.title", &dialogue.title)?;
    if dialogue.nodes.is_empty() {
        return Err(invalid_mcp_story(
            "dialogue.nodes",
            "at least one node is required",
        ));
    }
    let character_ids = state
        .characters
        .iter()
        .map(|character| character.character_id.as_str())
        .collect::<BTreeSet<_>>();
    let node_ids = dialogue
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<BTreeSet<_>>();
    if node_ids.len() != dialogue.nodes.len() {
        return Err(invalid_mcp_story(
            "dialogue.nodes",
            "duplicate dialogue node id",
        ));
    }
    for node in &dialogue.nodes {
        validate_symbol("dialogue.node_id", &node.node_id)?;
        validate_symbol("dialogue.speaker_id", &node.speaker_id)?;
        if !character_ids.contains(node.speaker_id.as_str()) {
            return Err(invalid_mcp_story(
                "dialogue.speaker_id",
                format!("missing character {}", node.speaker_id),
            ));
        }
        validate_single_line("dialogue.line", &node.line)?;
        validate_symbol_list("dialogue.quest_effects", &node.quest_effects)?;
        let mut choice_ids = BTreeSet::new();
        for choice in &node.choices {
            validate_symbol("dialogue.choice_id", &choice.choice_id)?;
            if !choice_ids.insert(choice.choice_id.as_str()) {
                return Err(invalid_mcp_story(
                    "dialogue.choice_id",
                    format!("duplicate choice {}", choice.choice_id),
                ));
            }
            validate_single_line("dialogue.choice_text", &choice.text)?;
            validate_symbol_list("dialogue.conditions", &choice.conditions)?;
            validate_symbol_list("dialogue.effects", &choice.effects)?;
            if let Some(target) = &choice.target_node_id {
                validate_symbol("dialogue.choice_target", target)?;
                if !node_ids.contains(target.as_str()) {
                    return Err(invalid_mcp_story(
                        "dialogue.choice_target",
                        format!("choice {} targets missing node {target}", choice.choice_id),
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn validate_quest(state: &McpStoryState, quest: &QuestArc) -> Result<(), EngineError> {
    validate_symbol("quest.quest_id", &quest.quest_id)?;
    validate_single_line("quest.title", &quest.title)?;
    let dialogue_ids = state
        .dialogues
        .iter()
        .map(|dialogue| dialogue.dialogue_id.as_str())
        .collect::<BTreeSet<_>>();
    for (field, value) in [
        ("quest.start_dialogue_id", quest.start_dialogue_id.as_ref()),
        (
            "quest.completion_dialogue_id",
            quest.completion_dialogue_id.as_ref(),
        ),
    ] {
        if let Some(dialogue_id) = value {
            validate_symbol(field, dialogue_id)?;
            if !dialogue_ids.contains(dialogue_id.as_str()) {
                return Err(invalid_mcp_story(
                    field,
                    format!("missing dialogue {dialogue_id}"),
                ));
            }
        }
    }
    if quest.objectives.is_empty() {
        return Err(invalid_mcp_story(
            "quest.objectives",
            "at least one objective is required",
        ));
    }
    let mut objective_ids = BTreeSet::new();
    for objective in &quest.objectives {
        validate_symbol("quest.objective_id", &objective.objective_id)?;
        if !objective_ids.insert(objective.objective_id.as_str()) {
            return Err(invalid_mcp_story(
                "quest.objective_id",
                format!("duplicate objective {}", objective.objective_id),
            ));
        }
        validate_single_line("quest.objective.description", &objective.description)?;
        validate_single_line("quest.objective.target_ref", &objective.target_ref)?;
        if objective.required_count == 0 {
            return Err(invalid_mcp_story(
                "quest.objective.required_count",
                "required count must be greater than zero",
            ));
        }
    }
    validate_symbol_list("quest.rewards", &quest.rewards)
}

pub(super) fn validate_cinematic(
    state: &McpStoryState,
    cinematic: &CinematicStage,
) -> Result<(), EngineError> {
    validate_symbol("cinematic.cinematic_id", &cinematic.cinematic_id)?;
    validate_single_line("cinematic.title", &cinematic.title)?;
    if cinematic.beats.is_empty() || cinematic.beats.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            "cinematic.beats",
            format!("beat count must be in 1..={MAX_LIST_LEN}"),
        ));
    }
    let dialogue_ids = state
        .dialogues
        .iter()
        .map(|dialogue| dialogue.dialogue_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut beat_ids = BTreeSet::new();
    let mut starts = BTreeSet::new();
    for beat in &cinematic.beats {
        validate_symbol("cinematic.beat_id", &beat.beat_id)?;
        if !beat_ids.insert(beat.beat_id.as_str()) || !starts.insert(beat.start_ms) {
            return Err(invalid_mcp_story(
                "cinematic.beats",
                "beat ids and start times must be unique",
            ));
        }
        if beat.duration_ms == 0 {
            return Err(invalid_mcp_story(
                "cinematic.duration_ms",
                "duration must be greater than zero",
            ));
        }
        validate_single_line("cinematic.camera", &beat.camera)?;
        validate_single_line("cinematic.mood", &beat.mood)?;
        validate_single_line("cinematic.staging", &beat.staging)?;
        if let Some(expression) = &beat.expression {
            validate_single_line("cinematic.expression", expression)?;
        }
        if let Some(dialogue_id) = &beat.dialogue_id {
            validate_symbol("cinematic.dialogue_id", dialogue_id)?;
            if !dialogue_ids.contains(dialogue_id.as_str()) {
                return Err(invalid_mcp_story(
                    "cinematic.dialogue_id",
                    format!("missing dialogue {dialogue_id}"),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_lore(lore: &LoreContext) -> Result<(), EngineError> {
    validate_symbol("lore.lore_id", &lore.lore_id)?;
    validate_single_line("lore.title", &lore.title)?;
    if lore.body.trim().is_empty() || lore.body.len() > MAX_TEXT_LEN {
        return Err(invalid_mcp_story(
            "lore.body",
            "body must be bounded non-empty text",
        ));
    }
    if lore.source_refs.is_empty() {
        return Err(invalid_mcp_story(
            "lore.source_refs",
            "at least one source ref is required",
        ));
    }
    validate_symbol_list("lore.source_refs", &lore.source_refs)?;
    validate_symbol_list("lore.tags", &lore.tags)
}

pub(super) fn validate_lore_entry_batch(
    state: &McpStoryState,
    entries: &[LoreEntry],
) -> Result<(), EngineError> {
    if entries.is_empty() {
        return Err(invalid_mcp_story(
            "lore_entry.entries",
            "at least one lore entry is required",
        ));
    }
    validate_lore_entry_collection(state, entries)
}

pub(super) fn validate_lore_entry_state(state: &McpStoryState) -> Result<(), EngineError> {
    validate_lore_entry_collection(state, &state.lore_entries)
}

fn validate_lore_entry_collection(
    state: &McpStoryState,
    entries: &[LoreEntry],
) -> Result<(), EngineError> {
    if entries.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            "lore_entry.entries",
            format!("entry count must be <= {MAX_LIST_LEN}"),
        ));
    }
    let mut batch_ids = BTreeSet::new();
    for entry in entries {
        validate_lore_entry_shape(entry)?;
        if !batch_ids.insert(entry.entry_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_entry.entry_id",
                format!("duplicate lore entry {}", entry.entry_id),
            ));
        }
    }
    let mut known_ids = state
        .lore_entries
        .iter()
        .map(|entry| entry.entry_id.clone())
        .collect::<BTreeSet<_>>();
    known_ids.extend(batch_ids.iter().map(|entry_id| (*entry_id).to_string()));
    for entry in entries {
        for link in &entry.links {
            if !known_ids.contains(&link.target_entry_id) {
                return Err(invalid_mcp_story(
                    "lore_entry.links",
                    format!(
                        "{} links to missing lore entry {}",
                        entry.entry_id, link.target_entry_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_lore_entry_shape(entry: &LoreEntry) -> Result<(), EngineError> {
    validate_symbol("lore_entry.entry_id", &entry.entry_id)?;
    validate_single_line("lore_entry.title", &entry.title)?;
    validate_single_line("lore_entry.summary", &entry.summary)?;
    validate_rich_text("lore_entry.body_rich_text", &entry.body_rich_text)?;
    validate_lore_metadata(&entry.metadata)?;
    if entry.source_refs.is_empty() {
        return Err(invalid_mcp_story(
            "lore_entry.source_refs",
            "at least one source ref is required",
        ));
    }
    validate_symbol_list("lore_entry.source_refs", &entry.source_refs)?;
    validate_symbol_list("lore_entry.tags", &entry.tags)?;
    validate_lore_links(entry)?;
    validate_lore_timeline_markers(entry)?;
    Ok(())
}

fn validate_lore_metadata(
    metadata: &std::collections::BTreeMap<String, String>,
) -> Result<(), EngineError> {
    if metadata.is_empty() || metadata.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            "lore_entry.metadata",
            format!("metadata count must be in 1..={MAX_LIST_LEN}"),
        ));
    }
    for (key, value) in metadata {
        validate_symbol("lore_entry.metadata.key", key)?;
        validate_single_line("lore_entry.metadata.value", value)?;
    }
    Ok(())
}

fn validate_lore_links(entry: &LoreEntry) -> Result<(), EngineError> {
    if entry.links.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            "lore_entry.links",
            format!("link count must be <= {MAX_LIST_LEN}"),
        ));
    }
    let mut links = BTreeSet::new();
    for link in &entry.links {
        validate_symbol("lore_entry.link.target_entry_id", &link.target_entry_id)?;
        validate_symbol("lore_entry.link.relation", &link.relation)?;
        validate_single_line("lore_entry.link.description", &link.description)?;
        if !links.insert((link.target_entry_id.as_str(), link.relation.as_str())) {
            return Err(invalid_mcp_story(
                "lore_entry.links",
                format!(
                    "duplicate {} link to {}",
                    link.relation, link.target_entry_id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_lore_timeline_markers(entry: &LoreEntry) -> Result<(), EngineError> {
    if entry.timeline_markers.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            "lore_entry.timeline_markers",
            format!("timeline marker count must be <= {MAX_LIST_LEN}"),
        ));
    }
    if matches!(entry.kind, LoreEntryKind::Timeline | LoreEntryKind::History)
        && entry.timeline_markers.is_empty()
    {
        return Err(invalid_mcp_story(
            "lore_entry.timeline_markers",
            "timeline and history entries require at least one marker",
        ));
    }
    let mut marker_ids = BTreeSet::new();
    let mut orders = BTreeSet::new();
    for marker in &entry.timeline_markers {
        validate_symbol("lore_entry.timeline.marker_id", &marker.marker_id)?;
        validate_single_line("lore_entry.timeline.label", &marker.label)?;
        validate_single_line("lore_entry.timeline.description", &marker.description)?;
        if !marker_ids.insert(marker.marker_id.as_str()) || !orders.insert(marker.order) {
            return Err(invalid_mcp_story(
                "lore_entry.timeline_markers",
                "marker ids and order values must be unique per entry",
            ));
        }
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
    if values.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_story(
            field,
            format!("list length must be <= {MAX_LIST_LEN}"),
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
