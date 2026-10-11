//! Real VFX MCP tool dispatch over the shared VFX capability layer.

use std::path::{Path, PathBuf};

use litd_engine::{
    EngineError,
    render::vfx::{
        VfxEffectAsset, VfxEffectDescriptor, VfxEffectStoreReadback, VfxRenderCue,
        VfxRenderCueSourceKind, effect_store_relative_path, read_effect_from_content_store,
        resolve_render_cue_effect, validate_effect, write_effect_to_content_store,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::{
    mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest},
    vfx_text_generation::{VfxTextGenerationRequest, generate_vfx_text_proposal},
};

use self::mcp_vfx_tools_store::{
    McpVfxToolLedgerReadback, McpVfxToolRecord, append_mcp_vfx_tool_ledger,
};

#[path = "mcp_vfx_tools_store.rs"]
mod mcp_vfx_tools_store;

pub use mcp_vfx_tools_store::{
    McpVfxToolLedger, VFX_MCP_LEDGER_SCHEMA, read_mcp_vfx_tool_ledger, validate_mcp_vfx_tool_ledger,
};

pub const DESCRIBE_EFFECT_TOOL: &str = "litd.engine.vfx.describe_effect_asset";
pub const SET_PARTICLE_STACK_TOOL: &str = "litd.engine.vfx.set_particle_stack";
pub const ATTACH_EFFECT_TOOL: &str = "litd.engine.vfx.attach_effect";
pub const READ_VFX_TOOL: &str = "litd.engine.vfx.read_vfx";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpVfxToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub ledger_readback: Option<McpVfxToolLedgerReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DescribeEffectAssetArgs {
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub store_root: PathBuf,
    pub request_id: String,
    pub prompt: String,
    pub seed: u64,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetParticleStackArgs {
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub store_root: PathBuf,
    pub request_id: String,
    pub requested_by: String,
    pub effect: VfxEffectAsset,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachEffectArgs {
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub store_root: PathBuf,
    pub request_id: String,
    pub requested_by: String,
    pub cue_id: String,
    pub source_kind: VfxRenderCueSourceKind,
    pub source_id: String,
    pub descriptor: VfxEffectDescriptor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadVfxArgs {
    pub store_root: PathBuf,
    pub descriptor: VfxEffectDescriptor,
}

pub fn call_mcp_vfx_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpVfxToolCallOutput, EngineError> {
    let route = vfx_route(tool_name)?;
    match tool_name {
        DESCRIBE_EFFECT_TOOL => describe_effect_asset(route, arguments),
        SET_PARTICLE_STACK_TOOL => set_particle_stack(route, arguments),
        ATTACH_EFFECT_TOOL => attach_effect(route, arguments),
        READ_VFX_TOOL => read_vfx(route, arguments),
        other => Err(invalid_mcp_vfx(
            "tool_name",
            format!("{other} is not a registered VFX MCP tool"),
        )),
    }
}

fn describe_effect_asset(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpVfxToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: DescribeEffectAssetArgs = parse_args(DESCRIBE_EFFECT_TOOL, arguments)?;
    validate_mutation_paths(&args.ledger_path, &args.store_root)?;
    let proposal = generate_vfx_text_proposal(VfxTextGenerationRequest {
        request_id: args.request_id.clone(),
        prompt: args.prompt.clone(),
        seed: args.seed,
        requested_by: args.requested_by.clone(),
    })?;
    let writeback = write_effect_to_content_store(&args.store_root, &proposal.effect)?;
    let record = record_from_readback(
        &args.request_id,
        &args.requested_by,
        &route,
        Some(args.prompt),
        &writeback.readback,
        None,
    )?;
    let ledger_readback = write_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(output(
        route,
        "generated effect asset from grounded text prompt",
        json!({
            "taxonomy": proposal.taxonomy,
            "budget": proposal.budget,
            "descriptor": writeback.descriptor,
            "content_object_path": writeback.object_path,
            "content_object_sha256": writeback.readback.sha256,
            "deduplicated": writeback.deduplicated
        }),
        Some(ledger_readback),
    ))
}

fn set_particle_stack(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpVfxToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetParticleStackArgs = parse_args(SET_PARTICLE_STACK_TOOL, arguments)?;
    validate_mutation_paths(&args.ledger_path, &args.store_root)?;
    let validation = validate_effect(&args.effect)?;
    let writeback = write_effect_to_content_store(&args.store_root, &args.effect)?;
    let record = record_from_readback(
        &args.request_id,
        &args.requested_by,
        &route,
        None,
        &writeback.readback,
        None,
    )?;
    let ledger_readback = write_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(output(
        route,
        "stored explicit particle stack as a content-addressed effect asset",
        json!({
            "descriptor": writeback.descriptor,
            "validation": validation,
            "content_object_path": writeback.object_path,
            "content_object_sha256": writeback.readback.sha256,
            "deduplicated": writeback.deduplicated
        }),
        Some(ledger_readback),
    ))
}

fn attach_effect(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpVfxToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: AttachEffectArgs = parse_args(ATTACH_EFFECT_TOOL, arguments)?;
    validate_mutation_paths(&args.ledger_path, &args.store_root)?;
    let cue = match args.source_kind {
        VfxRenderCueSourceKind::Ability => {
            VfxRenderCue::for_ability(args.cue_id, args.source_id, args.descriptor)?
        }
        VfxRenderCueSourceKind::Trigger => {
            VfxRenderCue::for_trigger(args.cue_id, args.source_id, args.descriptor)?
        }
    };
    let resolved = resolve_render_cue_effect(&args.store_root, &cue)?;
    let record = record_from_readback(
        &args.request_id,
        &args.requested_by,
        &route,
        None,
        &resolved.effect_readback,
        Some(cue.clone()),
    )?;
    let ledger_readback = write_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(output(
        route,
        "attached VFX descriptor to a render cue after content-store readback",
        json!({
            "cue": cue,
            "effect_readback": resolved.effect_readback,
            "resolved_source_of_truth": resolved.source_of_truth
        }),
        Some(ledger_readback),
    ))
}

fn read_vfx(route: McpDomainTool, arguments: Value) -> Result<McpVfxToolCallOutput, EngineError> {
    let args: ReadVfxArgs = parse_args(READ_VFX_TOOL, arguments)?;
    validate_path("store_root", &args.store_root)?;
    let readback = read_effect_from_content_store(&args.store_root, &args.descriptor)?;
    Ok(output(
        route,
        "content-addressed VFX object bytes",
        json!({
            "descriptor": readback.descriptor,
            "object_path": readback.object_path,
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "effect": readback.effect,
            "validation": readback.validation
        }),
        None,
    ))
}

fn write_ledger(
    ledger_path: &Path,
    ledger_id: String,
    record: McpVfxToolRecord,
) -> Result<McpVfxToolLedgerReadback, EngineError> {
    // Append to the ledger row rather than overwriting it with a single-record ledger
    // (#1521): every attach/generate mutation now accumulates in the audit trail.
    append_mcp_vfx_tool_ledger(ledger_path, ledger_id, record)
}

fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    ledger_readback: Option<McpVfxToolLedgerReadback>,
) -> McpVfxToolCallOutput {
    McpVfxToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        ledger_readback,
    }
}

fn record_from_readback(
    request_id: &str,
    requested_by: &str,
    route: &McpDomainTool,
    prompt: Option<String>,
    readback: &VfxEffectStoreReadback,
    cue: Option<VfxRenderCue>,
) -> Result<McpVfxToolRecord, EngineError> {
    Ok(McpVfxToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        effect_id: readback.effect.effect_id.clone(),
        prompt,
        descriptor: readback.descriptor.clone(),
        content_object_path: readback.object_path.display().to_string(),
        content_object_sha256: readback.sha256.clone(),
        content_object_bytes: readback.bytes,
        cue,
    })
}

fn vfx_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "vfx_particles")
        .ok_or_else(|| invalid_mcp_vfx("domain", "missing vfx_particles domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_vfx(
                "tool_name",
                format!("{tool_name} is not registered in vfx_particles"),
            )
        })
}

fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != READ_VFX_TOOL
    {
        return Err(invalid_mcp_vfx(
            "route",
            format!("{} is not a guarded VFX mutation route", route.tool_name),
        ));
    }
    Ok(())
}

fn validate_mutation_paths(ledger_path: &Path, store_root: &Path) -> Result<(), EngineError> {
    validate_path("ledger_path", ledger_path)?;
    validate_path("store_root", store_root)?;
    // #1518: confine both write sinks to the configured project root before any fs effect.
    // `store_root` does not follow the `_path` naming convention, so an explicit list (not a
    // field-name heuristic) is required to confine it.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("ledger_path", ledger_path),
        ("store_root", store_root),
    ])
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_vfx(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_vfx(field, "path must be non-empty"));
    }
    Ok(())
}

fn invalid_mcp_vfx(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.mcp_vfx_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_vfx_tools",
        field = %field,
        reason = %reason,
        "invalid VFX MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

pub fn descriptor_from_sha(
    effect_id: impl Into<String>,
    sha256: &str,
    size: u64,
) -> Result<VfxEffectDescriptor, EngineError> {
    let store_relative_path = effect_store_relative_path(sha256)?;
    VfxEffectDescriptor::new(effect_id, 1, format!("sha256:{sha256}"), size).and_then(|desc| {
        if desc.store_relative_path != store_relative_path {
            return Err(invalid_mcp_vfx(
                "descriptor",
                "computed descriptor path did not match sha path",
            ));
        }
        Ok(desc)
    })
}

#[cfg(test)]
#[path = "mcp_vfx_tools_tests.rs"]
mod tests;
