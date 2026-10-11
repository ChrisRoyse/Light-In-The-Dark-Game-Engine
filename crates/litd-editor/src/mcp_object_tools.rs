//! Real object/game-data MCP tool dispatch over the shared editor document layer.

use std::{collections::BTreeMap, path::Path};

use litd_engine::{EngineError, math::Transform};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::{
    history::{EditorDocument, EditorObject},
    mcp_tools::McpDomainTool,
};

use self::mcp_object_tools_store::{
    McpObjectDocumentReadback, McpObjectToolLedgerReadback, McpObjectToolRecord,
    load_mcp_object_document_for_mutation, write_mcp_object_document, write_mcp_object_tool_ledger,
};

#[path = "mcp_object_tools_dispatch.rs"]
mod mcp_object_tools_dispatch;
#[path = "mcp_object_tools_ids.rs"]
mod mcp_object_tools_ids;
#[path = "mcp_object_tools_lowering.rs"]
mod mcp_object_tools_lowering;
use mcp_object_tools_lowering as lowering;
#[path = "mcp_object_tools_store.rs"]
pub(crate) mod mcp_object_tools_store;
#[path = "mcp_object_tools_types.rs"]
mod mcp_object_tools_types;
#[path = "mcp_object_tools_validation.rs"]
mod mcp_object_tools_validation;

use mcp_object_tools_dispatch::{object_route, validate_affected_object_ids};
pub use mcp_object_tools_ids::{
    APPLY_OBJECT_DELTA_TOOL, COPY_BASE_OBJECT_TOOL, LOWER_ABILITY_TOOL, LOWER_BUFF_TOOL,
    LOWER_DESTRUCTIBLE_DOODAD_TOOL, LOWER_GAME_INTERFACE_TOOL, LOWER_GAMEPLAY_CONSTANTS_TOOL,
    LOWER_HERO_TOOL, LOWER_ITEM_TOOL, LOWER_SHOP_TOOL, LOWER_TECH_REQUIREMENTS_TOOL,
    LOWER_UNIT_TOOL, LOWER_UPGRADE_TOOL, READ_LOWERED_ABILITY_TOOL, READ_LOWERED_BUFF_TOOL,
    READ_LOWERED_DESTRUCTIBLE_DOODAD_TOOL, READ_LOWERED_GAME_INTERFACE_TOOL,
    READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL, READ_LOWERED_HERO_TOOL, READ_LOWERED_ITEM_TOOL,
    READ_LOWERED_SHOP_TOOL, READ_LOWERED_TECH_REQUIREMENTS_TOOL, READ_LOWERED_UNIT_TOOL,
    READ_LOWERED_UPGRADE_TOOL, READ_OBJECT_DATA_TOOL, UPSERT_GAMEPLAY_TABLES_TOOL,
};
pub use mcp_object_tools_store::{
    MCP_OBJECT_LEDGER_SCHEMA, McpObjectToolLedger, read_mcp_object_document,
    read_mcp_object_tool_ledger, validate_mcp_object_tool_ledger,
};
pub use mcp_object_tools_types::{
    ApplyObjectDeltaArgs, CopyBaseObjectArgs, GameplayTable, GameplayTableRow, LowerAbilityArgs,
    LowerGameplayConstantsArgs, LowerHeroArgs, LowerItemArgs, LowerShopArgs,
    LowerTechRequirementsArgs, LowerUnitArgs, ObjectBaseTemplate, ObjectDataKind,
    ObjectPropertyDelta, ReadLoweredAbilityArgs, ReadLoweredGameplayConstantsArgs,
    ReadLoweredHeroArgs, ReadLoweredItemArgs, ReadLoweredShopArgs, ReadLoweredTechRequirementsArgs,
    ReadLoweredUnitArgs, ReadObjectDataArgs, UpsertGameplayTablesArgs,
};
pub use mcp_object_tools_validation::sample_issue919_document;
use mcp_object_tools_validation::{
    gameplay_table_object_id, validate_base_copy, validate_gameplay_tables,
    validate_initial_document, validate_object_delta, validate_single_line,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpObjectToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub document_readback: Option<McpObjectDocumentReadback>,
    pub ledger_readback: Option<McpObjectToolLedgerReadback>,
}

pub fn call_mcp_object_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let route = object_route(tool_name)?;
    match tool_name {
        COPY_BASE_OBJECT_TOOL => copy_base_object(route, arguments),
        APPLY_OBJECT_DELTA_TOOL => apply_object_delta(route, arguments),
        UPSERT_GAMEPLAY_TABLES_TOOL => upsert_gameplay_tables(route, arguments),
        READ_OBJECT_DATA_TOOL => read_object_data(route, arguments),
        LOWER_UNIT_TOOL => lowering::lower_unit(route, arguments),
        READ_LOWERED_UNIT_TOOL => lowering::read_lowered_unit(route, arguments),
        LOWER_ABILITY_TOOL => lowering::lower_ability(route, arguments),
        READ_LOWERED_ABILITY_TOOL => lowering::read_lowered_ability(route, arguments),
        LOWER_HERO_TOOL => lowering::lower_hero(route, arguments),
        READ_LOWERED_HERO_TOOL => lowering::read_lowered_hero(route, arguments),
        LOWER_ITEM_TOOL => lowering::lower_item(route, arguments),
        READ_LOWERED_ITEM_TOOL => lowering::read_lowered_item(route, arguments),
        LOWER_SHOP_TOOL => lowering::lower_shop(route, arguments),
        READ_LOWERED_SHOP_TOOL => lowering::read_lowered_shop(route, arguments),
        LOWER_TECH_REQUIREMENTS_TOOL => lowering::lower_tech_requirements(route, arguments),
        READ_LOWERED_TECH_REQUIREMENTS_TOOL => {
            lowering::read_lowered_tech_requirements(route, arguments)
        }
        LOWER_GAMEPLAY_CONSTANTS_TOOL => lowering::lower_gameplay_constants(route, arguments),
        READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL => {
            lowering::read_lowered_gameplay_constants(route, arguments)
        }
        LOWER_BUFF_TOOL => lowering::lower_buff(route, arguments),
        READ_LOWERED_BUFF_TOOL => lowering::read_lowered_buff(route, arguments),
        LOWER_UPGRADE_TOOL => lowering::lower_upgrade(route, arguments),
        READ_LOWERED_UPGRADE_TOOL => lowering::read_lowered_upgrade(route, arguments),
        LOWER_DESTRUCTIBLE_DOODAD_TOOL => lowering::lower_destructible_doodad(route, arguments),
        READ_LOWERED_DESTRUCTIBLE_DOODAD_TOOL => {
            lowering::read_lowered_destructible_doodad(route, arguments)
        }
        LOWER_GAME_INTERFACE_TOOL => lowering::lower_game_interface(route, arguments),
        READ_LOWERED_GAME_INTERFACE_TOOL => lowering::read_lowered_game_interface(route, arguments),
        other => Err(invalid_mcp_object(
            "tool_name",
            format!("{other} is not a registered object/game-data MCP tool"),
        )),
    }
}

fn copy_base_object(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: CopyBaseObjectArgs = parse_args(COPY_BASE_OBJECT_TOOL, arguments)?;
    validate_mutation_paths(&args.document_path, &args.ledger_path)?;
    validate_initial_document(args.initial_document.as_ref())?;
    validate_base_copy(
        &args.new_object_id,
        &args.rawcode,
        &args.base,
        &args.overrides,
    )?;
    let mut document =
        load_mcp_object_document_for_mutation(&args.document_path, args.initial_document)?;
    if document.objects.contains_key(&args.new_object_id) {
        return Err(invalid_mcp_object(
            "copy_base_object.new_object_id",
            format!("{} already exists", args.new_object_id),
        ));
    }
    let mut properties = args.base.properties.clone();
    for (key, value) in args.overrides {
        properties.insert(key, value);
    }
    properties.insert(
        "object.base_id".to_string(),
        args.base.base_object_id.clone(),
    );
    properties.insert("object.rawcode".to_string(), args.rawcode.clone());
    properties.insert(
        "object.display_name".to_string(),
        args.base.display_name.clone(),
    );
    properties.insert(
        "object.category".to_string(),
        args.base.kind.as_str().to_string(),
    );
    let object = EditorObject {
        id: args.new_object_id.clone(),
        kind: args.base.kind.as_str().to_string(),
        parent: None,
        transform: Transform::IDENTITY,
        properties,
    };
    document.upsert_object(object)?;
    finish_object_mutation(FinishObjectMutationArgs {
        route,
        document_path: &args.document_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "copied base object into validated object data",
        document,
        affected_object_ids: vec![args.new_object_id],
        operation_report: json!({
            "base_object_id": args.base.base_object_id,
            "rawcode": args.rawcode
        }),
    })
}

fn apply_object_delta(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ApplyObjectDeltaArgs = parse_args(APPLY_OBJECT_DELTA_TOOL, arguments)?;
    validate_mutation_paths(&args.document_path, &args.ledger_path)?;
    validate_initial_document(args.initial_document.as_ref())?;
    validate_object_delta(&args.object_id, &args.delta)?;
    let mut document =
        load_mcp_object_document_for_mutation(&args.document_path, args.initial_document)?;
    let mut object = document
        .objects
        .get(&args.object_id)
        .cloned()
        .ok_or_else(|| {
            invalid_mcp_object(
                "apply_object_delta.object_id",
                format!("{} does not exist", args.object_id),
            )
        })?;
    let mut changed = false;
    if let Some(kind) = args.delta.set_kind {
        let kind = kind.as_str();
        if object.kind != kind {
            object.kind = kind.to_string();
            object
                .properties
                .insert("object.category".to_string(), kind.to_string());
            changed = true;
        }
    }
    for (key, value) in args.delta.upsert_properties {
        if object.properties.get(&key) != Some(&value) {
            object.properties.insert(key, value);
            changed = true;
        }
    }
    for key in args.delta.remove_properties {
        if object.properties.remove(&key).is_some() {
            changed = true;
        }
    }
    if !changed {
        return Err(invalid_mcp_object(
            "apply_object_delta.delta",
            "delta produced no object data change",
        ));
    }
    document.upsert_object(object)?;
    finish_object_mutation(FinishObjectMutationArgs {
        route,
        document_path: &args.document_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "applied validated object property delta",
        document,
        affected_object_ids: vec![args.object_id],
        operation_report: json!({ "delta_applied": true }),
    })
}

fn upsert_gameplay_tables(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: UpsertGameplayTablesArgs = parse_args(UPSERT_GAMEPLAY_TABLES_TOOL, arguments)?;
    validate_mutation_paths(&args.document_path, &args.ledger_path)?;
    validate_initial_document(args.initial_document.as_ref())?;
    validate_gameplay_tables(&args.tables)?;
    let mut document =
        load_mcp_object_document_for_mutation(&args.document_path, args.initial_document)?;
    let mut affected_object_ids = Vec::new();
    for table in args.tables {
        let object_id = gameplay_table_object_id(&table.table_id);
        let mut properties = BTreeMap::new();
        properties.insert("gameplay.table_id".to_string(), table.table_id.clone());
        properties.insert("gameplay.category".to_string(), table.category.clone());
        properties.insert(
            "gameplay.row_count".to_string(),
            table.rows.len().to_string(),
        );
        for row in table.rows {
            for (field, value) in row.fields {
                properties.insert(format!("row.{}.{}", row.row_id, field), value);
            }
        }
        let object = EditorObject {
            id: object_id.clone(),
            kind: ObjectDataKind::GameplayTable.as_str().to_string(),
            parent: None,
            transform: Transform::IDENTITY,
            properties,
        };
        document.upsert_object(object)?;
        affected_object_ids.push(object_id);
    }
    finish_object_mutation(FinishObjectMutationArgs {
        route,
        document_path: &args.document_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "upserted validated gameplay tables",
        document,
        affected_object_ids,
        operation_report: json!({ "gameplay_tables_upserted": true }),
    })
}

fn read_object_data(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadObjectDataArgs = parse_args(READ_OBJECT_DATA_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    let document_readback = read_mcp_object_document(&args.document_path)?;
    Ok(output(
        route,
        "Calyx Aster Graph CF object/gameplay document rows + structured constellations",
        json!({ "document_readback": document_readback }),
        Some(document_readback),
        None,
    ))
}

struct FinishObjectMutationArgs<'a> {
    route: McpDomainTool,
    document_path: &'a Path,
    ledger_path: &'a Path,
    ledger_id: String,
    request_id: &'a str,
    requested_by: &'a str,
    action_summary: &'a str,
    document: EditorDocument,
    affected_object_ids: Vec<String>,
    operation_report: Value,
}

fn finish_object_mutation(
    args: FinishObjectMutationArgs<'_>,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let FinishObjectMutationArgs {
        route,
        document_path,
        ledger_path,
        ledger_id,
        request_id,
        requested_by,
        action_summary,
        document,
        affected_object_ids,
        operation_report,
    } = args;
    validate_affected_object_ids(&affected_object_ids)?;
    let document_readback = write_mcp_object_document(document_path, &document)?;
    let record = object_record(
        request_id,
        requested_by,
        &route,
        document_path,
        &document_readback,
        action_summary,
        affected_object_ids,
    )?;
    let ledger_readback = write_mcp_object_tool_ledger(ledger_path, ledger_id, record)?;
    Ok(output(
        route,
        "Calyx Aster Graph CF object/gameplay document rows + structured constellations + object MCP ledger rows",
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
    document_readback: Option<McpObjectDocumentReadback>,
    ledger_readback: Option<McpObjectToolLedgerReadback>,
) -> McpObjectToolCallOutput {
    McpObjectToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        document_readback,
        ledger_readback,
    }
}

fn object_record(
    request_id: &str,
    requested_by: &str,
    route: &McpDomainTool,
    document_path: &Path,
    readback: &McpObjectDocumentReadback,
    action_summary: &str,
    affected_object_ids: Vec<String>,
) -> Result<McpObjectToolRecord, EngineError> {
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)?;
    Ok(McpObjectToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        document_path: document_path.display().to_string(),
        document_sha256: readback.sha256.clone(),
        document_bytes: readback.bytes,
        document_fingerprint: readback.document_fingerprint.clone(),
        action_summary: action_summary.to_string(),
        affected_object_ids,
        object_count: readback.object_count,
        unit_count: readback.unit_count,
        hero_count: readback.hero_count,
        ability_count: readback.ability_count,
        item_count: readback.item_count,
        buff_count: readback.buff_count,
        upgrade_count: readback.upgrade_count,
        tech_count: readback.tech_count,
        gameplay_table_count: readback.gameplay_table_count,
    })
}

fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != READ_OBJECT_DATA_TOOL
    {
        return Err(invalid_mcp_object(
            "route",
            format!(
                "{} is not a guarded object/game-data route",
                route.tool_name
            ),
        ));
    }
    Ok(())
}

fn validate_mutation_paths(document_path: &Path, ledger_path: &Path) -> Result<(), EngineError> {
    validate_path("document_path", document_path)?;
    validate_path("ledger_path", ledger_path)?;
    // #1518: confine both write sinks to the configured project root before any fs effect,
    // so a poisoned document_path/ledger_path cannot become an arbitrary-write primitive.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("document_path", document_path),
        ("ledger_path", ledger_path),
    ])
}

fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_object(field, "path must be non-empty"));
    }
    Ok(())
}

fn parse_args<T: DeserializeOwned>(tool_name: &str, value: Value) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_object(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

pub(super) fn invalid_mcp_object(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_object_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_object_tools",
        field = %field,
        reason = %reason,
        "invalid object/game-data MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "mcp_object_tools_tests.rs"]
mod tests;
