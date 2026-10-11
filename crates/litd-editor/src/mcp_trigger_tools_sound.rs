use std::path::Path;

use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::{
    mcp_project_tools::mcp_project_tools_validation::confine_tool_paths,
    mcp_tools::McpDomainTool,
    sound_editor::{
        SoundEditorRecord, TriggerSoundBinding, build_sound_editor_runtime, import_sound_file,
    },
};

use super::{
    IMPORT_SOUND_TOOL, ImportSoundArgs, McpTriggerToolCallOutput, PreviewSoundArgs,
    READ_SOUND_EDITOR_TOOL, ReadSoundEditorArgs, TriggerRecordArgs, UPSERT_SOUND_VARIABLE_TOOL,
    UpsertSoundVariableArgs, empty_trigger_counts, invalid_mcp_trigger, output, parse_args,
    read_mcp_sound_record, read_mcp_sound_runtime, trigger_record, validate_mutating_route,
};
use super::{
    mcp_trigger_tools_store::{
        McpSoundRecordReadback, McpSoundRuntimeReadback, load_mcp_sound_record_for_mutation,
        write_mcp_sound_record, write_mcp_sound_runtime, write_mcp_trigger_tool_ledger,
    },
    mcp_trigger_tools_validation::{validate_request_identity, validate_symbol},
};

pub(super) fn import_sound(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ImportSoundArgs = parse_args(IMPORT_SOUND_TOOL, arguments)?;
    // #1518: confine the record/ledger write sinks AND the source sound file read path to
    // the project root before any fs effect (import_sound_file reads source_path).
    confine_tool_paths(&[
        ("record_path", args.record_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
        ("source_path", args.source_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_symbol("sound.asset_id", &args.asset_id)?;
    let mut record = load_mcp_sound_record_for_mutation(&args.record_path, args.initial_record)?;
    if record
        .library
        .iter()
        .any(|entry| entry.asset_id == args.asset_id)
    {
        return Err(invalid_mcp_trigger(
            "sound.asset_id",
            format!("{} already exists", args.asset_id),
        ));
    }
    let entry = import_sound_file(&args.source_path, args.category, args.asset_id.clone())?;
    if record
        .library
        .iter()
        .any(|existing| existing.source_path == entry.source_path)
    {
        return Err(invalid_mcp_trigger(
            "sound.source_path",
            format!("{} already imported", entry.source_path),
        ));
    }
    record.library.push(entry);
    let sound_readback = write_mcp_sound_record(&args.record_path, &record)?;
    finish_sound_mutation(FinishSoundMutationArgs {
        route,
        primary_path: &args.record_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "imported decoded sound asset",
        sound_readback,
        runtime_readback: None,
        affected_ids: vec![args.asset_id],
    })
}

pub(super) fn upsert_sound_variable(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertSoundVariableArgs = parse_args(UPSERT_SOUND_VARIABLE_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("record_path", args.record_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    args.sound.validate()?;
    if let Some(binding) = &args.trigger_binding {
        binding.validate()?;
        if binding.variable_name != args.sound.variable_name {
            return Err(invalid_mcp_trigger(
                "sound.trigger_binding",
                "binding variable must match the upserted sound variable",
            ));
        }
    }
    let mut record = load_mcp_sound_record_for_mutation(&args.record_path, args.initial_record)?;
    upsert_by(
        &mut record.named_sounds,
        |sound| sound.variable_name == args.sound.variable_name,
        args.sound.clone(),
    );
    if let Some(binding) = args.trigger_binding {
        upsert_sound_binding(&mut record, binding);
    }
    record.validate()?;
    let sound_readback = write_mcp_sound_record(&args.record_path, &record)?;
    finish_sound_mutation(FinishSoundMutationArgs {
        route,
        primary_path: &args.record_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted named sound variable and optional trigger binding",
        sound_readback,
        runtime_readback: None,
        affected_ids: vec![args.sound.variable_name],
    })
}

pub(super) fn preview_sound(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: PreviewSoundArgs = parse_args(super::PREVIEW_SOUND_TOOL, arguments)?;
    // #1518: confine the record read + runtime/ledger write sinks before any fs effect.
    confine_tool_paths(&[
        ("record_path", args.record_path.as_path()),
        ("runtime_path", args.runtime_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_symbol("preview.variable_name", &args.variable_name)?;
    let sound_readback = read_mcp_sound_record(&args.record_path)?;
    let runtime = build_sound_editor_runtime(
        &sound_readback.record,
        Some(&args.variable_name),
        args.cursor_ms,
    )?;
    let runtime_readback = write_mcp_sound_runtime(&args.runtime_path, &runtime)?;
    finish_sound_mutation(FinishSoundMutationArgs {
        route,
        primary_path: &args.runtime_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "wrote sound preview runtime readback",
        sound_readback,
        runtime_readback: Some(runtime_readback),
        affected_ids: vec![args.variable_name],
    })
}

pub(super) fn read_sound_editor(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    let args: ReadSoundEditorArgs = parse_args(READ_SOUND_EDITOR_TOOL, arguments)?;
    // #1518: confine every read path to the project root before reading from disk.
    let mut confined = vec![("record_path", args.record_path.as_path())];
    if let Some(runtime_path) = args.runtime_path.as_ref() {
        confined.push(("runtime_path", runtime_path.as_path()));
    }
    confine_tool_paths(&confined)?;
    let sound_readback = read_mcp_sound_record(&args.record_path)?;
    let runtime_readback = if let Some(path) = args.runtime_path {
        Some(read_mcp_sound_runtime(path)?)
    } else {
        None
    };
    Ok(output(
        route,
        "SoundEditorRecord RON + optional sound runtime JSON",
        json!({
            "sound_readback": sound_readback,
            "runtime_readback": runtime_readback
        }),
        None,
        Some(sound_readback),
        runtime_readback,
        None,
    ))
}

struct FinishSoundMutationArgs<'a> {
    route: McpDomainTool,
    primary_path: &'a Path,
    ledger_path: &'a Path,
    ledger_id: String,
    request_id: &'a str,
    requested_by: &'a str,
    action_summary: &'a str,
    sound_readback: McpSoundRecordReadback,
    runtime_readback: Option<McpSoundRuntimeReadback>,
    affected_ids: Vec<String>,
}

fn finish_sound_mutation(
    args: FinishSoundMutationArgs<'_>,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    let FinishSoundMutationArgs {
        route,
        primary_path,
        ledger_path,
        ledger_id,
        request_id,
        requested_by,
        action_summary,
        sound_readback,
        runtime_readback,
        affected_ids,
    } = args;
    let (primary_sha256, primary_bytes) = runtime_readback
        .as_ref()
        .map(|readback| (readback.sha256.clone(), readback.bytes))
        .unwrap_or_else(|| (sound_readback.sha256.clone(), sound_readback.bytes));
    let record = trigger_record(TriggerRecordArgs {
        request_id,
        requested_by,
        route: &route,
        primary_path,
        primary_sha256: &primary_sha256,
        primary_bytes,
        action_summary,
        affected_ids,
        trigger_counts: &empty_trigger_counts(),
        sound_counts: Some(&sound_readback),
    });
    let ledger_readback = write_mcp_trigger_tool_ledger(ledger_path, ledger_id, record)?;
    Ok(output(
        route,
        "SoundEditorRecord RON/runtime JSON + trigger MCP ledger JSON",
        json!({
            "sound_readback": sound_readback,
            "runtime_readback": runtime_readback
        }),
        None,
        Some(sound_readback),
        runtime_readback,
        Some(ledger_readback),
    ))
}

fn upsert_sound_binding(record: &mut SoundEditorRecord, binding: TriggerSoundBinding) {
    let trigger_id = binding.trigger_id.clone();
    let action_id = binding.action_id.clone();
    upsert_by(
        &mut record.trigger_bindings,
        |existing| existing.trigger_id == trigger_id && existing.action_id == action_id,
        binding,
    );
}

fn upsert_by<T, F>(values: &mut Vec<T>, mut predicate: F, value: T)
where
    F: FnMut(&T) -> bool,
{
    if let Some(existing) = values.iter_mut().find(|existing| predicate(existing)) {
        *existing = value;
    } else {
        values.push(value);
    }
}
