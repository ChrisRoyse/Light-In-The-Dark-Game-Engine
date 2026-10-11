//! Real terrain/world MCP tool dispatch over the shared editor document layer.

use std::collections::BTreeSet;
use std::path::Path;

use litd_engine::{
    EngineError,
    math::{Quat, Transform, Vec3},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::{
    history::{EditorDocument, EditorObject},
    mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest},
    terrain_advanced_tools::{apply_terrain_brush, flood_fill_tile},
    terrain_bulk_ops::{
        adjust_cliff_levels, replace_cliff_type, replace_tiles, reset_height_field,
    },
    terrain_pathing::{paint_pathing_cells, set_global_tile_pathing},
};

use self::mcp_world_tools_store::load_mcp_world_document_for_mutation;

#[path = "mcp_world_tools_store.rs"]
mod mcp_world_tools_store;
#[path = "mcp_world_tools_types.rs"]
mod mcp_world_tools_types;
#[path = "mcp_world_tools_validation.rs"]
mod mcp_world_tools_validation;

pub use mcp_world_tools_store::{
    MCP_WORLD_LEDGER_SCHEMA, McpWorldDocumentReadback, McpWorldToolLedger,
    McpWorldToolLedgerReadback, McpWorldToolRecord, read_mcp_world_document,
    read_mcp_world_tool_ledger, validate_mcp_world_tool_ledger, write_mcp_world_document,
    write_mcp_world_tool_ledger,
};
pub use mcp_world_tools_types::{
    BulkEditTerrainArgs, EvaluateRegionTransitionsArgs, PaintTilesArgs, PlaceActorRegionCameraArgs,
    ReadWorldArgs, SetStartLocationsArgs, WorldBulkEdit, WorldPathingPaint, WorldStartLocation,
};
pub use mcp_world_tools_validation::sample_issue918_document;
use mcp_world_tools_validation::{
    validate_initial_document, validate_single_line, validate_start_locations,
    validate_start_locations_against_world,
};

pub const PAINT_TILES_TOOL: &str = "litd.editor.world.paint_tiles";
pub const BULK_EDIT_TERRAIN_TOOL: &str = "litd.editor.world.bulk_edit_terrain";
pub const PLACE_ACTOR_REGION_CAMERA_TOOL: &str = "litd.editor.world.place_actor_region_camera";
pub const SET_START_LOCATIONS_TOOL: &str = "litd.editor.world.set_start_locations";
pub const READ_WORLD_TOOL: &str = "litd.editor.world.read_world";
pub const EVALUATE_REGION_TRANSITIONS_TOOL: &str = "litd.editor.world.evaluate_region_transitions";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpWorldToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub document_readback: Option<McpWorldDocumentReadback>,
    pub ledger_readback: Option<McpWorldToolLedgerReadback>,
}

pub fn call_mcp_world_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpWorldToolCallOutput, EngineError> {
    let route = world_route(tool_name)?;
    match tool_name {
        PAINT_TILES_TOOL => paint_tiles(route, arguments),
        BULK_EDIT_TERRAIN_TOOL => bulk_edit_terrain(route, arguments),
        PLACE_ACTOR_REGION_CAMERA_TOOL => place_actor_region_camera(route, arguments),
        SET_START_LOCATIONS_TOOL => set_start_locations(route, arguments),
        READ_WORLD_TOOL => read_world(route, arguments),
        EVALUATE_REGION_TRANSITIONS_TOOL => evaluate_region_transitions_tool(route, arguments),
        other => Err(invalid_mcp_world(
            "tool_name",
            format!("{other} is not a registered terrain/world MCP tool"),
        )),
    }
}

fn paint_tiles(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpWorldToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: PaintTilesArgs = parse_args(PAINT_TILES_TOOL, arguments)?;
    validate_mutation_paths(&args.document_path, &args.ledger_path)?;
    validate_initial_document(args.initial_document.as_ref())?;
    let mut document =
        load_mcp_world_document_for_mutation(&args.document_path, args.initial_document)?;
    let terrain_report = apply_terrain_brush(&mut document, &args.brush)?;
    let pathing_report = if let Some(pathing) = args.pathing {
        Some(paint_pathing_cells(
            &mut document,
            pathing.cell_ids,
            pathing.mode,
        )?)
    } else {
        None
    };
    finish_world_mutation(FinishWorldMutationArgs {
        route,
        document_path: &args.document_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "painted terrain tiles/heights and optional pathing",
        document,
        operation_report: json!({ "terrain": terrain_report, "pathing": pathing_report }),
    })
}

fn bulk_edit_terrain(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpWorldToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: BulkEditTerrainArgs = parse_args(BULK_EDIT_TERRAIN_TOOL, arguments)?;
    validate_mutation_paths(&args.document_path, &args.ledger_path)?;
    validate_initial_document(args.initial_document.as_ref())?;
    let mut document =
        load_mcp_world_document_for_mutation(&args.document_path, args.initial_document)?;
    let report = match args.edit {
        WorldBulkEdit::ReplaceTiles {
            catalog,
            old_tile_id,
            new_tile_id,
            scope,
        } => json!(replace_tiles(
            &mut document,
            &catalog,
            &old_tile_id,
            &new_tile_id,
            &scope,
        )?),
        WorldBulkEdit::ReplaceCliffType {
            catalog,
            old_cliff_type,
            new_cliff_type,
            scope,
        } => json!(replace_cliff_type(
            &mut document,
            &catalog,
            &old_cliff_type,
            &new_cliff_type,
            &scope,
        )?),
        WorldBulkEdit::AdjustCliffLevels { delta, scope } => {
            json!(adjust_cliff_levels(&mut document, delta, &scope)?)
        }
        WorldBulkEdit::ResetHeightField { base_height, scope } => {
            json!(reset_height_field(&mut document, base_height, &scope)?)
        }
        WorldBulkEdit::FloodFillTile {
            seed_cell_id,
            new_tile_id,
        } => json!(flood_fill_tile(&mut document, &seed_cell_id, &new_tile_id)?),
        WorldBulkEdit::SetGlobalTilePathing { tile_id, flags } => {
            json!(set_global_tile_pathing(&mut document, &tile_id, flags)?)
        }
    };
    finish_world_mutation(FinishWorldMutationArgs {
        route,
        document_path: &args.document_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "applied validated terrain/world bulk edit",
        document,
        operation_report: json!({ "bulk_edit": report }),
    })
}

fn place_actor_region_camera(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpWorldToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: PlaceActorRegionCameraArgs = parse_args(PLACE_ACTOR_REGION_CAMERA_TOOL, arguments)?;
    validate_mutation_paths(&args.document_path, &args.ledger_path)?;
    validate_initial_document(args.initial_document.as_ref())?;
    if args.objects.is_empty()
        && args.regions.is_empty()
        && args.camera.is_none()
        && args.named_cameras.is_empty()
    {
        return Err(invalid_mcp_world(
            "place.inputs",
            "at least one object, region, camera config, or named camera is required",
        ));
    }
    let mut document =
        load_mcp_world_document_for_mutation(&args.document_path, args.initial_document)?;
    let object_count = args.objects.len();
    let region_count = args.regions.len();
    let named_camera_count = args.named_cameras.len();
    let camera_updated = args.camera.is_some();
    for object in args.objects {
        document.upsert_object(object)?;
    }
    for region in args.regions {
        document.upsert_region(region)?;
    }
    for named_camera in args.named_cameras {
        document.upsert_named_camera(named_camera)?;
    }
    if let Some(camera) = args.camera {
        document.set_play_camera_config(camera)?;
    }
    finish_world_mutation(FinishWorldMutationArgs {
        route,
        document_path: &args.document_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "placed actors/regions/named cameras and optional camera config",
        document,
        operation_report: json!({
            "object_count": object_count,
            "region_count": region_count,
            "named_camera_count": named_camera_count,
            "camera_updated": camera_updated
        }),
    })
}

fn set_start_locations(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpWorldToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetStartLocationsArgs = parse_args(SET_START_LOCATIONS_TOOL, arguments)?;
    validate_mutation_paths(&args.document_path, &args.ledger_path)?;
    validate_initial_document(args.initial_document.as_ref())?;
    validate_start_locations(&args.starts)?;
    let mut document =
        load_mcp_world_document_for_mutation(&args.document_path, args.initial_document)?;
    // #817: bounds + pathing validation needs the loaded terrain, so it runs after load.
    validate_start_locations_against_world(&document, &args.starts)?;
    let mut object_ids = Vec::new();
    for start in args.starts {
        let object = start_location_object(&start);
        object_ids.push(object.id.clone());
        document.upsert_object(object)?;
    }
    finish_world_mutation(FinishWorldMutationArgs {
        route,
        document_path: &args.document_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "set explicit player start-location objects",
        document,
        operation_report: json!({ "start_location_object_ids": object_ids }),
    })
}

fn read_world(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpWorldToolCallOutput, EngineError> {
    let args: ReadWorldArgs = parse_args(READ_WORLD_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    let document_readback = read_mcp_world_document(&args.document_path)?;
    Ok(output(
        route,
        "MCP world EditorDocument JSON",
        json!({ "document_readback": document_readback }),
        Some(document_readback),
        None,
    ))
}

fn evaluate_region_transitions_tool(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpWorldToolCallOutput, EngineError> {
    let args: EvaluateRegionTransitionsArgs =
        parse_args(EVALUATE_REGION_TRANSITIONS_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    let document_readback = read_mcp_world_document(&args.document_path)?;
    let report = crate::region_transitions::evaluate_region_transitions(
        &document_readback.document,
        args.previous,
        args.next,
    )?;
    Ok(output(
        route,
        "MCP world EditorDocument regions + deterministic region-transition events",
        json!({ "region_transitions": report }),
        Some(document_readback),
        None,
    ))
}

struct FinishWorldMutationArgs<'a> {
    route: McpDomainTool,
    document_path: &'a Path,
    ledger_path: &'a Path,
    ledger_id: String,
    request_id: &'a str,
    requested_by: &'a str,
    action_summary: &'a str,
    document: EditorDocument,
    operation_report: Value,
}

fn finish_world_mutation(
    args: FinishWorldMutationArgs<'_>,
) -> Result<McpWorldToolCallOutput, EngineError> {
    let FinishWorldMutationArgs {
        route,
        document_path,
        ledger_path,
        ledger_id,
        request_id,
        requested_by,
        action_summary,
        document,
        operation_report,
    } = args;
    let document_readback = write_mcp_world_document(document_path, &document)?;
    let record = world_record(
        request_id,
        requested_by,
        &route,
        document_path,
        &document_readback,
        action_summary,
    )?;
    let ledger_readback = write_mcp_world_tool_ledger(ledger_path, ledger_id, record)?;
    Ok(output(
        route,
        "MCP world EditorDocument JSON + world MCP ledger JSON",
        json!({
            "operation_report": operation_report,
            "document_readback": document_readback
        }),
        Some(document_readback),
        Some(ledger_readback),
    ))
}

fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    document_readback: Option<McpWorldDocumentReadback>,
    ledger_readback: Option<McpWorldToolLedgerReadback>,
) -> McpWorldToolCallOutput {
    McpWorldToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        document_readback,
        ledger_readback,
    }
}

fn world_record(
    request_id: &str,
    requested_by: &str,
    route: &McpDomainTool,
    document_path: &Path,
    readback: &McpWorldDocumentReadback,
    action_summary: &str,
) -> Result<McpWorldToolRecord, EngineError> {
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)?;
    Ok(McpWorldToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        document_path: document_path.display().to_string(),
        document_sha256: readback.sha256.clone(),
        document_bytes: readback.bytes,
        document_fingerprint: readback.document_fingerprint.clone(),
        action_summary: action_summary.to_string(),
        terrain_cell_count: readback.terrain_cell_count,
        terrain_stamp_count: readback.terrain_stamp_count,
        object_count: readback.object_count,
        region_count: readback.region_count,
        pathing_override_count: readback.pathing_override_count,
        start_location_count: readback.start_location_count,
    })
}

fn world_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "terrain_world")
        .ok_or_else(|| invalid_mcp_world("domain", "missing terrain_world domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_world(
                "tool_name",
                format!("{tool_name} is not registered in terrain_world"),
            )
        })
}

fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != READ_WORLD_TOOL
    {
        return Err(invalid_mcp_world(
            "route",
            format!("{} is not a guarded terrain/world route", route.tool_name),
        ));
    }
    Ok(())
}

fn validate_mutation_paths(document_path: &Path, ledger_path: &Path) -> Result<(), EngineError> {
    validate_path("document_path", document_path)?;
    validate_path("ledger_path", ledger_path)?;
    // #1518: confine both write sinks to the configured project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("document_path", document_path),
        ("ledger_path", ledger_path),
    ])
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_world(field, "path must be non-empty"));
    }
    Ok(())
}

fn start_location_object(start: &WorldStartLocation) -> EditorObject {
    EditorObject::new(
        start.object_id.clone(),
        "StartLocation",
        None,
        Transform::from_trs(
            start.position,
            Quat::from_rotation_y(start.yaw_radians),
            Vec3::ONE,
        ),
    )
    .with_property("player_id", start.player_id.clone())
    .with_property("slot", start.slot.to_string())
    .with_property("kind", "start_location")
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_world(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

pub(super) fn invalid_mcp_world(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_world_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_world_tools",
        field = %field,
        reason = %reason,
        "invalid terrain/world MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

pub fn selected_cell_ids(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

#[cfg(test)]
#[path = "mcp_world_tools_tests.rs"]
mod tests;
