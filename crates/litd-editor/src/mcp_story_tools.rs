//! Real visual-story and lore MCP tool dispatch over persisted story state.

use std::path::Path;

use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths;
use crate::mcp_tools::McpDomainTool;

use self::mcp_story_tools_store::{
    McpStoryState, McpStoryToolRecord, load_or_create_mcp_story_state, write_mcp_story_state,
    write_mcp_story_tool_ledger,
};

#[path = "mcp_story_tools_dispatch.rs"]
mod mcp_story_tools_dispatch;
#[path = "mcp_story_tools_lore_canon.rs"]
mod mcp_story_tools_lore_canon;
#[path = "mcp_story_tools_lore_cowriter.rs"]
mod mcp_story_tools_lore_cowriter;
#[path = "mcp_story_tools_lore_graph.rs"]
mod mcp_story_tools_lore_graph;
#[path = "mcp_story_tools_lore_surface.rs"]
mod mcp_story_tools_lore_surface;
#[path = "mcp_story_tools_lore_templates.rs"]
mod mcp_story_tools_lore_templates;
#[path = "mcp_story_tools_store.rs"]
pub(crate) mod mcp_story_tools_store;
#[path = "mcp_story_tools_types.rs"]
mod mcp_story_tools_types;
#[path = "mcp_story_tools_validation.rs"]
mod mcp_story_tools_validation;

pub use mcp_story_tools_dispatch::McpStoryToolCallOutput;
use mcp_story_tools_dispatch::{
    invalid_mcp_story, output, parse_args, story_route, upsert_by, validate_mutating_route,
};
pub use mcp_story_tools_lore_canon::{
    LORE_CANON_QUERY_LEDGER_SCHEMA, LoreCanonQueryLedger, LoreCanonQueryLedgerReadback,
    read_lore_canon_query_ledger, validate_lore_canon_query_ledger,
};
pub use mcp_story_tools_lore_cowriter::{
    LORE_COWRITER_LEDGER_SCHEMA, LoreCowriterLedger, LoreCowriterLedgerReadback,
    read_lore_cowriter_ledger, validate_lore_cowriter_ledger,
};
use mcp_story_tools_lore_graph::{
    build_lore_relationship_graph, validate_lore_relationship_graph_queries,
};
use mcp_story_tools_lore_surface::validate_lore_surface_batch;
pub use mcp_story_tools_lore_templates::{
    LORE_TEMPLATE_LEDGER_SCHEMA, LoreTemplateLedger, LoreTemplateLedgerReadback,
    read_lore_template_ledger, validate_lore_template_ledger,
};
pub use mcp_story_tools_store::{
    MCP_STORY_LEDGER_SCHEMA, MCP_STORY_STATE_SCHEMA, McpStoryToolLedger, read_mcp_story_state,
    read_mcp_story_tool_ledger, validate_mcp_story_state, validate_mcp_story_tool_ledger,
};
pub use mcp_story_tools_types::{
    BuildLoreRelationshipGraphArgs, CinematicBeat, CinematicStage, CoWriteLoreArgs, DialogueChoice,
    DialogueNode, DialogueTree, GenerateLoreTemplatesArgs, LoreCanonClaim, LoreCanonQuery,
    LoreCanonQueryKind, LoreContext, LoreCowriterDraft, LoreCowriterDraftKind,
    LoreCowriterWardPolicy, LoreEntry, LoreEntryKind, LoreEntryLink, LoreSurface, LoreSurfaceKind,
    LoreTemplateKind, LoreTemplateRequest, LoreTemplateWardPolicy, LoreTimelineMarker,
    QueryLoreCanonArgs, QuestArc, QuestObjective, ReadStoryArgs, StageCinematicArgs,
    StoryCharacter, UpsertDialogueQuestArgs, UpsertLoreContextArgs, UpsertLoreEntryArgs,
    UpsertLoreSurfaceArgs,
};
use mcp_story_tools_validation::{
    validate_character, validate_cinematic, validate_dialogue, validate_lore,
    validate_lore_entry_batch, validate_quest, validate_request_identity,
};

use crate::world_graph_navigation::{
    build_world_graph_navigation_ledger, traverse_world_graph, write_world_graph_navigation_ledger,
};

pub const UPSERT_DIALOGUE_QUEST_TOOL: &str = "litd.editor.story.upsert_dialogue_quest";
pub const STAGE_CINEMATIC_TOOL: &str = "litd.editor.story.stage_cinematic";
pub const UPSERT_LORE_CONTEXT_TOOL: &str = "litd.editor.lore.upsert_lore_context";
pub const UPSERT_LORE_ENTRY_TOOL: &str = "litd.editor.lore.upsert_lore_entry";
pub const UPSERT_LORE_SURFACE_TOOL: &str = "litd.editor.lore.upsert_lore_surface";
pub const BUILD_LORE_RELATIONSHIP_GRAPH_TOOL: &str = "litd.editor.lore.build_relationship_graph";
pub const QUERY_LORE_CANON_TOOL: &str = "litd.editor.lore.query_canon";
pub const CO_WRITE_LORE_TOOL: &str = "litd.editor.lore.co_write";
pub const GENERATE_LORE_TEMPLATES_TOOL: &str = "litd.editor.lore.generate_template";
pub const READ_STORY_TOOL: &str = "litd.editor.story.read_story";

pub fn call_mcp_story_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    let route = story_route(tool_name)?;
    match tool_name {
        UPSERT_DIALOGUE_QUEST_TOOL => upsert_dialogue_quest(route, arguments),
        STAGE_CINEMATIC_TOOL => stage_cinematic(route, arguments),
        UPSERT_LORE_CONTEXT_TOOL => upsert_lore_context(route, arguments),
        UPSERT_LORE_ENTRY_TOOL => upsert_lore_entry(route, arguments),
        UPSERT_LORE_SURFACE_TOOL => upsert_lore_surface(route, arguments),
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL => build_lore_relationship_graph_tool(route, arguments),
        QUERY_LORE_CANON_TOOL => {
            mcp_story_tools_lore_canon::query_lore_canon_tool(route, arguments)
        }
        CO_WRITE_LORE_TOOL => mcp_story_tools_lore_cowriter::co_write_lore_tool(route, arguments),
        GENERATE_LORE_TEMPLATES_TOOL => {
            mcp_story_tools_lore_templates::generate_lore_templates_tool(route, arguments)
        }
        READ_STORY_TOOL => read_story(route, arguments),
        other => Err(invalid_mcp_story(
            "tool_name",
            format!("{other} is not a registered story/lore MCP tool"),
        )),
    }
}

fn upsert_dialogue_quest(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertDialogueQuestArgs = parse_args(UPSERT_DIALOGUE_QUEST_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let has_dialogue = args.dialogue.is_some();
    if args.characters.is_empty() && !has_dialogue && args.quests.is_empty() {
        return Err(invalid_mcp_story(
            "dialogue_quest.inputs",
            "at least one character, dialogue, or quest is required",
        ));
    }
    let mut state = load_or_create_mcp_story_state(&args.state_path, &args.state_id)?;
    for character in &args.characters {
        validate_character(character)?;
        upsert_by(
            &mut state.characters,
            |item| item.character_id == character.character_id,
            character.clone(),
        );
    }
    if let Some(dialogue) = args.dialogue {
        validate_dialogue(&state, &dialogue)?;
        upsert_by(
            &mut state.dialogues,
            |item| item.dialogue_id == dialogue.dialogue_id,
            dialogue.clone(),
        );
    }
    for quest in &args.quests {
        validate_quest(&state, quest)?;
        upsert_by(
            &mut state.quests,
            |item| item.quest_id == quest.quest_id,
            quest.clone(),
        );
    }
    state.update_count += 1;
    finish_story_mutation(FinishStoryMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted dialogue, characters, and quests",
        state,
        affected_ids: vec!["dialogue_quest".to_string()],
    })
}

fn stage_cinematic(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: StageCinematicArgs = parse_args(STAGE_CINEMATIC_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_story_state(&args.state_path, &args.state_id)?;
    validate_cinematic(&state, &args.cinematic)?;
    upsert_by(
        &mut state.cinematics,
        |item| item.cinematic_id == args.cinematic.cinematic_id,
        args.cinematic.clone(),
    );
    state.update_count += 1;
    finish_story_mutation(FinishStoryMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "staged validated cinematic timeline",
        state,
        affected_ids: vec![args.cinematic.cinematic_id],
    })
}

fn upsert_lore_context(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertLoreContextArgs = parse_args(UPSERT_LORE_CONTEXT_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_lore(&args.lore)?;
    let mut state = load_or_create_mcp_story_state(&args.state_path, &args.state_id)?;
    upsert_by(
        &mut state.lore_contexts,
        |item| item.lore_id == args.lore.lore_id,
        args.lore.clone(),
    );
    state.update_count += 1;
    finish_story_mutation(FinishStoryMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted grounded lore context",
        state,
        affected_ids: vec![args.lore.lore_id],
    })
}

fn upsert_lore_entry(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertLoreEntryArgs = parse_args(UPSERT_LORE_ENTRY_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_story_state(&args.state_path, &args.state_id)?;
    validate_lore_entry_batch(&state, &args.entries)?;
    let affected_ids = args
        .entries
        .iter()
        .map(|entry| entry.entry_id.clone())
        .collect::<Vec<_>>();
    for entry in &args.entries {
        upsert_by(
            &mut state.lore_entries,
            |item| item.entry_id == entry.entry_id,
            entry.clone(),
        );
    }
    state.update_count += 1;
    finish_story_mutation(FinishStoryMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted structured world-bible lore entries",
        state,
        affected_ids,
    })
}

fn upsert_lore_surface(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertLoreSurfaceArgs = parse_args(UPSERT_LORE_SURFACE_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_story_state(&args.state_path, &args.state_id)?;
    validate_lore_surface_batch(&state, &args.surfaces)?;
    let affected_ids = args
        .surfaces
        .iter()
        .map(|surface| surface.surface_id.clone())
        .collect::<Vec<_>>();
    for surface in &args.surfaces {
        upsert_by(
            &mut state.lore_surfaces,
            |item| item.surface_id == surface.surface_id,
            surface.clone(),
        );
    }
    state.update_count += 1;
    finish_story_mutation(FinishStoryMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted in-world lore surfaces",
        state,
        affected_ids,
    })
}

fn build_lore_relationship_graph_tool(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: BuildLoreRelationshipGraphArgs =
        parse_args(BUILD_LORE_RELATIONSHIP_GRAPH_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("graph_ledger_path", args.graph_ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_lore_relationship_graph_queries(&args.queries, &args.requested_by)?;
    let state_readback = read_mcp_story_state(&args.state_path)?;
    if state_readback.state.state_id != args.state_id {
        return Err(invalid_mcp_story(
            "lore_relationship_graph.state_id",
            "state_id mismatch",
        ));
    }
    let source_of_truth = format!(
        "Calyx Aster MCP story state {} sha256 {}",
        state_readback.path, state_readback.sha256
    );
    let build =
        build_lore_relationship_graph(&args.graph_id, source_of_truth, &state_readback.state)?;
    let traversals = args
        .queries
        .iter()
        .map(|query| traverse_world_graph(&build.graph, query))
        .collect::<Result<Vec<_>, _>>()?;
    let graph_ledger =
        build_world_graph_navigation_ledger(args.graph_ledger_id.clone(), build.graph, traversals)?;
    let graph_ledger_readback = write_world_graph_navigation_ledger(
        &args.state_path,
        &args.graph_ledger_path,
        &graph_ledger,
    )?;
    let result = json!({
        "request_id": args.request_id,
        "relationship_graph": build.summary,
        "graph_ledger_readback": {
            "path": args.graph_ledger_path,
            "ledger_id": args.graph_ledger_id,
            "sha256": graph_ledger_readback.sha256,
            "bytes": graph_ledger_readback.bytes.len(),
            "seq": graph_ledger_readback.seq,
            "graph_rows": graph_ledger_readback.graph_rows,
            "cell_key": graph_ledger_readback.cell_key,
            "index_key": graph_ledger_readback.index_key,
            "node_count": graph_ledger_readback.node_count,
            "edge_count": graph_ledger_readback.edge_count,
            "traversal_count": graph_ledger_readback.traversal_count,
        },
        "traversals": graph_ledger.traversals,
    });
    Ok(output(
        route,
        "MCP story/lore state JSON + Calyx Aster lore relationship graph ledger",
        result,
        Some(state_readback),
        None,
    ))
}

fn read_story(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    let args: ReadStoryArgs = parse_args(READ_STORY_TOOL, arguments)?;
    // #1518: confine the read path to the project root before reading from disk.
    confine_tool_paths(&[("state_path", args.state_path.as_path())])?;
    let state_readback = read_mcp_story_state(&args.state_path)?;
    Ok(output(
        route,
        "MCP story/lore state JSON",
        json!({ "state_readback": state_readback }),
        Some(state_readback),
        None,
    ))
}

struct FinishStoryMutationArgs<'a> {
    route: McpDomainTool,
    state_path: &'a Path,
    ledger_path: &'a Path,
    ledger_id: String,
    request_id: &'a str,
    requested_by: &'a str,
    action_summary: &'a str,
    state: McpStoryState,
    affected_ids: Vec<String>,
}

fn finish_story_mutation(
    args: FinishStoryMutationArgs<'_>,
) -> Result<McpStoryToolCallOutput, EngineError> {
    let FinishStoryMutationArgs {
        route,
        state_path,
        ledger_path,
        ledger_id,
        request_id,
        requested_by,
        action_summary,
        state,
        affected_ids,
    } = args;
    let state_readback = write_mcp_story_state(state_path, &state)?;
    let record = McpStoryToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: state_path.display().to_string(),
        state_sha256: state_readback.sha256.clone(),
        state_bytes: state_readback.bytes,
        action_summary: action_summary.to_string(),
        affected_ids,
        character_count: state_readback.character_count,
        dialogue_count: state_readback.dialogue_count,
        quest_count: state_readback.quest_count,
        cinematic_count: state_readback.cinematic_count,
        lore_context_count: state_readback.lore_context_count,
        lore_entry_count: state_readback.lore_entry_count,
        lore_entry_link_count: state_readback.lore_entry_link_count,
        lore_timeline_marker_count: state_readback.lore_timeline_marker_count,
        lore_surface_count: state_readback.lore_surface_count,
        lore_surface_lore_ref_count: state_readback.lore_surface_lore_ref_count,
        lore_surface_codex_unlock_count: state_readback.lore_surface_codex_unlock_count,
    };
    let ledger_readback = write_mcp_story_tool_ledger(ledger_path, ledger_id, record)?;
    Ok(output(
        route,
        "MCP story/lore state JSON + story MCP ledger JSON",
        json!({ "state_readback": state_readback }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}

#[cfg(test)]
#[path = "mcp_story_tools_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mcp_story_tools_lore_entry_tests.rs"]
mod lore_entry_tests;

#[cfg(test)]
#[path = "mcp_story_tools_lore_graph_tests.rs"]
mod lore_graph_tests;

#[cfg(test)]
#[path = "mcp_story_tools_lore_surface_tests.rs"]
mod lore_surface_tests;
