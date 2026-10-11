//! GUI/text authoring and guarded lowering for deterministic trigger WASM.

use litd_engine::{
    EngineError,
    wasm_script::{
        WASM_FIXED_SLOT_COUNT, WASM_MAX_FUEL, WASM_MIN_FUEL, WasmFixedState, WasmScriptArtifact,
        WasmScriptBinding, WasmScriptRuntime,
    },
};
use litd_hash::{is_sha256_hex, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::mcp_trigger_tools::{EcaGraph, WasmEscape};

pub const WASM_GUI_SCHEMA: &str = "litd.trigger.wasm-gui.v1";
pub const WASM_GUI_CUSTOM_SECTION: &str = "litd.trigger.wasm-gui.v1";
const MAX_GUI_OPERATIONS: usize = 128;
const MAX_WAT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmGuiProgram {
    pub schema: String,
    pub script_id: String,
    pub entry_point: String,
    pub fuel_budget: u64,
    pub approved_reason: String,
    pub operations: Vec<WasmGuiOperation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WasmGuiOperation {
    SetFixed { slot: u8, raw: i64 },
    AddFixed { slot: u8, raw: i64 },
}

pub fn lower_gui_program(program: &WasmGuiProgram) -> Result<WasmEscape, EngineError> {
    validate_gui_program(program)?;
    let wat_source = gui_program_wat(program);
    let gui_metadata_json = serde_json::to_string(program).map_err(|error| {
        authoring_error("gui.metadata", format!("serialize GUI metadata: {error}"))
    })?;
    let mut escape = WasmEscape {
        script_id: program.script_id.clone(),
        wat_source,
        wat_source_sha256: None,
        entry_point: program.entry_point.clone(),
        fuel_budget: program.fuel_budget,
        approved_reason: program.approved_reason.clone(),
        gui_metadata_json: Some(gui_metadata_json),
        artifact: None,
    };
    materialize_wasm_escape(&mut escape)?;
    Ok(escape)
}

pub fn raise_gui_program(escape: &WasmEscape) -> Result<WasmGuiProgram, EngineError> {
    validate_materialized_wasm_escape(escape)?;
    let metadata = escape.gui_metadata_json.as_deref().ok_or_else(|| {
        authoring_error(
            "gui.metadata",
            "arbitrary WAT has no reversible GUI metadata section",
        )
    })?;
    let artifact = escape
        .artifact
        .as_ref()
        .ok_or_else(|| authoring_error("artifact", "materialized artifact is required"))?;
    let physical = artifact
        .custom_section(WASM_GUI_CUSTOM_SECTION)
        .map_err(|error| authoring_error("artifact.custom_section", error))?
        .ok_or_else(|| {
            authoring_error(
                "artifact.custom_section",
                format!("missing {WASM_GUI_CUSTOM_SECTION}"),
            )
        })?;
    if physical != metadata.as_bytes() {
        return Err(authoring_error(
            "artifact.custom_section",
            "physical GUI metadata does not match authored metadata",
        ));
    }
    let program: WasmGuiProgram = serde_json::from_str(metadata)
        .map_err(|error| authoring_error("gui.metadata", format!("parse GUI metadata: {error}")))?;
    let regenerated = lower_gui_program(&program)?;
    if &regenerated != escape {
        return Err(authoring_error(
            "gui.roundtrip",
            "GUI metadata does not regenerate identical WAT and wasm content",
        ));
    }
    Ok(program)
}

pub fn prepare_wasm_escape(
    escape: &WasmEscape,
    initial_state: WasmFixedState,
) -> Result<WasmScriptRuntime, EngineError> {
    validate_materialized_wasm_escape(escape)?;
    let artifact = escape
        .artifact
        .clone()
        .ok_or_else(|| authoring_error("artifact", "materialized artifact is required"))?;
    WasmScriptRuntime::prepare(
        WasmScriptBinding {
            artifact,
            entry_point: escape.entry_point.clone(),
            fuel_budget: escape.fuel_budget,
        },
        initial_state,
    )
    .map_err(|error| authoring_error("runtime.prepare", error))
}

pub(crate) fn materialize_wasm_escape(escape: &mut WasmEscape) -> Result<(), EngineError> {
    validate_escape_shape(escape)?;
    let source_sha256 = sha256_hex(escape.wat_source.as_bytes());
    if let Some(supplied) = &escape.wat_source_sha256
        && supplied != &source_sha256
    {
        return Err(authoring_error(
            "wat_source_sha256",
            "source SHA-256 does not match authored source bytes",
        ));
    }
    escape.wat_source_sha256 = Some(source_sha256);
    validate_gui_metadata(escape)?;
    let generated = match escape.gui_metadata_json.as_deref() {
        Some(metadata) => WasmScriptArtifact::from_wat_with_custom_section(
            &escape.script_id,
            &escape.wat_source,
            WASM_GUI_CUSTOM_SECTION,
            metadata.as_bytes(),
        ),
        None => WasmScriptArtifact::from_wat(&escape.script_id, &escape.wat_source),
    }
    .map_err(|error| authoring_error("artifact.lower", error))?;
    if let Some(supplied) = &escape.artifact
        && supplied != &generated
    {
        return Err(authoring_error(
            "artifact.integrity",
            "supplied wasm artifact does not match deterministic lowering output",
        ));
    }
    escape.artifact = Some(generated);
    let artifact = escape
        .artifact
        .clone()
        .ok_or_else(|| authoring_error("artifact", "lowering did not produce an artifact"))?;
    WasmScriptRuntime::prepare(
        WasmScriptBinding {
            artifact,
            entry_point: escape.entry_point.clone(),
            fuel_budget: escape.fuel_budget,
        },
        WasmFixedState::default(),
    )
    .map(|_| ())
    .map_err(|error| authoring_error("runtime.prepare", error))
}

pub(crate) fn validate_materialized_wasm_escape(escape: &WasmEscape) -> Result<(), EngineError> {
    validate_escape_shape(escape)?;
    let Some(artifact) = &escape.artifact else {
        return Err(authoring_error(
            "artifact",
            "WASM source must be lowered to a physical artifact before persistence",
        ));
    };
    if artifact.module_id != escape.script_id {
        return Err(authoring_error(
            "artifact.module_id",
            "artifact module id does not match script id",
        ));
    }
    let mut regenerated = escape.clone();
    regenerated.artifact = None;
    materialize_wasm_escape(&mut regenerated)?;
    if regenerated.artifact.as_ref() != Some(artifact) {
        return Err(authoring_error(
            "artifact.integrity",
            "persisted artifact does not match authored source and metadata",
        ));
    }
    Ok(())
}

pub(crate) fn validate_graph_wasm_escape(graph: &EcaGraph) -> Result<(), EngineError> {
    graph
        .wasm_escape
        .as_ref()
        .map_or(Ok(()), validate_materialized_wasm_escape)
}

pub(crate) fn validate_state_wasm_escapes(graphs: &[EcaGraph]) -> Result<(), EngineError> {
    for graph in graphs {
        validate_graph_wasm_escape(graph)?;
    }
    Ok(())
}

fn validate_gui_program(program: &WasmGuiProgram) -> Result<(), EngineError> {
    if program.schema != WASM_GUI_SCHEMA {
        return Err(authoring_error(
            "gui.schema",
            "unexpected GUI program schema",
        ));
    }
    if program.operations.is_empty() || program.operations.len() > MAX_GUI_OPERATIONS {
        return Err(authoring_error(
            "gui.operations",
            format!("must contain 1..={MAX_GUI_OPERATIONS} operations"),
        ));
    }
    validate_common(
        &program.script_id,
        &program.entry_point,
        program.fuel_budget,
        &program.approved_reason,
    )?;
    for operation in &program.operations {
        let slot = match operation {
            WasmGuiOperation::SetFixed { slot, .. } | WasmGuiOperation::AddFixed { slot, .. } => {
                *slot
            }
        };
        if usize::from(slot) >= WASM_FIXED_SLOT_COUNT {
            return Err(authoring_error(
                "gui.operation.slot",
                format!("slot {slot} exceeds {} fixed slots", WASM_FIXED_SLOT_COUNT),
            ));
        }
    }
    Ok(())
}

fn validate_escape_shape(escape: &WasmEscape) -> Result<(), EngineError> {
    validate_common(
        &escape.script_id,
        &escape.entry_point,
        escape.fuel_budget,
        &escape.approved_reason,
    )?;
    if escape.wat_source.trim().is_empty() || escape.wat_source.len() > MAX_WAT_BYTES {
        return Err(authoring_error(
            "wat_source",
            format!("must contain 1..={MAX_WAT_BYTES} bytes"),
        ));
    }
    if escape.wat_source.contains('\0') {
        return Err(authoring_error("wat_source", "NUL bytes are not allowed"));
    }
    if let Some(metadata) = &escape.gui_metadata_json
        && (metadata.is_empty() || metadata.len() > 64 * 1024)
    {
        return Err(authoring_error(
            "gui_metadata_json",
            "must contain 1..=65536 bytes when present",
        ));
    }
    if let Some(sha256) = &escape.wat_source_sha256
        && !is_sha256_hex(sha256)
    {
        return Err(authoring_error(
            "wat_source_sha256",
            "must be a canonical lowercase SHA-256 when present",
        ));
    }
    Ok(())
}

fn validate_gui_metadata(escape: &WasmEscape) -> Result<(), EngineError> {
    let Some(metadata) = escape.gui_metadata_json.as_deref() else {
        return Ok(());
    };
    let program: WasmGuiProgram = serde_json::from_str(metadata)
        .map_err(|error| authoring_error("gui_metadata_json", format!("parse: {error}")))?;
    validate_gui_program(&program)?;
    let canonical = serde_json::to_string(&program)
        .map_err(|error| authoring_error("gui_metadata_json", format!("canonicalize: {error}")))?;
    if canonical != metadata {
        return Err(authoring_error(
            "gui_metadata_json",
            "GUI metadata must use the canonical serialized representation",
        ));
    }
    if program.script_id != escape.script_id
        || program.entry_point != escape.entry_point
        || program.fuel_budget != escape.fuel_budget
        || program.approved_reason != escape.approved_reason
    {
        return Err(authoring_error(
            "gui_metadata_json",
            "GUI metadata identity, entry point, fuel, or approval does not match the escape",
        ));
    }
    if gui_program_wat(&program) != escape.wat_source {
        return Err(authoring_error(
            "gui_metadata_json",
            "GUI metadata does not regenerate the authored WAT",
        ));
    }
    Ok(())
}

fn validate_common(
    script_id: &str,
    entry_point: &str,
    fuel_budget: u64,
    approved_reason: &str,
) -> Result<(), EngineError> {
    for (field, value) in [("script_id", script_id), ("entry_point", entry_point)] {
        let mut chars = value.chars();
        if value.len() > 96
            || !chars
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
            || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Err(authoring_error(field, "must be a bounded ASCII identifier"));
        }
    }
    if !(WASM_MIN_FUEL..=WASM_MAX_FUEL).contains(&fuel_budget) {
        return Err(authoring_error(
            "fuel_budget",
            format!("must be in {WASM_MIN_FUEL}..={WASM_MAX_FUEL}"),
        ));
    }
    if approved_reason.trim().is_empty()
        || approved_reason.len() > 2048
        || approved_reason.contains(['\r', '\n', '\t'])
    {
        return Err(authoring_error(
            "approved_reason",
            "must be bounded non-empty single-line text",
        ));
    }
    Ok(())
}

fn gui_program_wat(program: &WasmGuiProgram) -> String {
    let mut source = String::from(
        "(module\n  (import \"litd\" \"set_fixed\" (func $set_fixed (param i32 i64)))\n  (import \"litd\" \"add_fixed\" (func $add_fixed (param i32 i64) (result i64)))\n  (func (export \"",
    );
    source.push_str(&program.entry_point);
    source.push_str("\")\n");
    for operation in &program.operations {
        match operation {
            WasmGuiOperation::SetFixed { slot, raw } => {
                source.push_str(&format!(
                    "    i32.const {slot}\n    i64.const {raw}\n    call $set_fixed\n"
                ));
            }
            WasmGuiOperation::AddFixed { slot, raw } => {
                source.push_str(&format!(
                    "    i32.const {slot}\n    i64.const {raw}\n    call $add_fixed\n    drop\n"
                ));
            }
        }
    }
    source.push_str("  ))\n");
    source
}

fn authoring_error(field: &str, reason: impl std::fmt::Display) -> EngineError {
    EngineError::SerializationFailed {
        format: "trigger-wasm-authoring",
        reason: format!("{field}: {reason}"),
    }
}
