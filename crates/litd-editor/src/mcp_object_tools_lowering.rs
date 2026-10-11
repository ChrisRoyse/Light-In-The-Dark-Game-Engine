//! #829 lowering-valve MCP handlers: lower a persisted unit to a fingerprinted litd/data
//! TOML artifact and re-read that artifact from disk with fingerprint verification. Split
//! out of `mcp_object_tools` to keep each dispatch file under the 500-line ratchet.

use litd_engine::EngineError;
use serde_json::{Value, json};

use super::{
    LOWER_ABILITY_TOOL, LOWER_GAMEPLAY_CONSTANTS_TOOL, LOWER_HERO_TOOL, LOWER_ITEM_TOOL,
    LOWER_SHOP_TOOL, LOWER_TECH_REQUIREMENTS_TOOL, LOWER_UNIT_TOOL, LowerAbilityArgs,
    LowerGameplayConstantsArgs, LowerHeroArgs, LowerItemArgs, LowerShopArgs,
    LowerTechRequirementsArgs, LowerUnitArgs, McpDomainTool, McpObjectToolCallOutput,
    READ_LOWERED_ABILITY_TOOL, READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL, READ_LOWERED_HERO_TOOL,
    READ_LOWERED_ITEM_TOOL, READ_LOWERED_SHOP_TOOL, READ_LOWERED_TECH_REQUIREMENTS_TOOL,
    READ_LOWERED_UNIT_TOOL, ReadLoweredAbilityArgs, ReadLoweredGameplayConstantsArgs,
    ReadLoweredHeroArgs, ReadLoweredItemArgs, ReadLoweredShopArgs, ReadLoweredTechRequirementsArgs,
    ReadLoweredUnitArgs, output, parse_args, validate_path, validate_single_line,
};

#[path = "mcp_object_tools_lowering_p2.rs"]
mod p2;
pub(super) use p2::{
    lower_buff, lower_destructible_doodad, lower_game_interface, lower_upgrade, read_lowered_buff,
    read_lowered_destructible_doodad, read_lowered_game_interface, read_lowered_upgrade,
};

fn confine_artifact(path: &std::path::Path) -> Result<(), EngineError> {
    // Confine the lowered-artifact sink to the project root before any fs effect (#1518).
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[(
        "artifact_path",
        path,
    )])
}

pub(super) fn lower_unit(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: LowerUnitArgs = parse_args(LOWER_UNIT_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("object_id", &args.object_id)?;
    confine_artifact(&args.artifact_path)?;
    // Lower straight from the persisted (Aster) source of truth, never from caller memory.
    let lowered =
        crate::object_editor::lower_unit_from_document_path(&args.document_path, &args.object_id)?;
    let toml_readback =
        crate::object_editor::write_lowered_unit_toml(&args.artifact_path, &lowered)?;
    Ok(output(
        route,
        "Calyx Aster object document -> #829 lowering valve -> fingerprinted litd/data unit TOML re-read from disk",
        json!({
            "lowered_artifact": {
                "object_id": args.object_id,
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "unit": toml_readback.document.unit,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn read_lowered_unit(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadLoweredUnitArgs = parse_args(READ_LOWERED_UNIT_TOOL, arguments)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("expected_fingerprint", &args.expected_fingerprint)?;
    let toml_readback = crate::object_editor::read_lowered_unit_toml(
        &args.artifact_path,
        &args.expected_fingerprint,
    )?;
    Ok(output(
        route,
        "fingerprinted litd/data unit TOML artifact re-read from disk + fingerprint verified",
        json!({
            "lowered_artifact": {
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "unit": toml_readback.document.unit,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn lower_ability(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: LowerAbilityArgs = parse_args(LOWER_ABILITY_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("object_id", &args.object_id)?;
    confine_artifact(&args.artifact_path)?;
    let lowered = crate::object_editor::lower_ability_from_document_path(
        &args.document_path,
        &args.object_id,
    )?;
    let toml_readback =
        crate::object_editor::write_lowered_ability_toml(&args.artifact_path, &lowered)?;
    Ok(output(
        route,
        "Calyx Aster object document -> #829 lowering valve -> fingerprinted litd/data ability TOML re-read from disk",
        json!({
            "lowered_artifact": {
                "object_id": args.object_id,
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "ability": toml_readback.document.ability,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn read_lowered_ability(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadLoweredAbilityArgs = parse_args(READ_LOWERED_ABILITY_TOOL, arguments)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("expected_fingerprint", &args.expected_fingerprint)?;
    let toml_readback = crate::object_editor::read_lowered_ability_toml(
        &args.artifact_path,
        &args.expected_fingerprint,
    )?;
    Ok(output(
        route,
        "fingerprinted litd/data ability TOML artifact re-read from disk + fingerprint verified",
        json!({
            "lowered_artifact": {
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "ability": toml_readback.document.ability,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn lower_hero(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: LowerHeroArgs = parse_args(LOWER_HERO_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("object_id", &args.object_id)?;
    confine_artifact(&args.artifact_path)?;
    let lowered =
        crate::object_editor::lower_hero_from_document_path(&args.document_path, &args.object_id)?;
    let toml_readback =
        crate::object_editor::write_lowered_hero_toml(&args.artifact_path, &lowered)?;
    Ok(output(
        route,
        "Calyx Aster object document -> #829 lowering valve -> fingerprinted litd/data hero TOML re-read from disk",
        json!({
            "lowered_artifact": {
                "object_id": args.object_id,
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "hero": toml_readback.document.hero,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn read_lowered_hero(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadLoweredHeroArgs = parse_args(READ_LOWERED_HERO_TOOL, arguments)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("expected_fingerprint", &args.expected_fingerprint)?;
    let toml_readback = crate::object_editor::read_lowered_hero_toml(
        &args.artifact_path,
        &args.expected_fingerprint,
    )?;
    Ok(output(
        route,
        "fingerprinted litd/data hero TOML artifact re-read from disk + fingerprint verified",
        json!({
            "lowered_artifact": {
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "hero": toml_readback.document.hero,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn lower_item(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: LowerItemArgs = parse_args(LOWER_ITEM_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("object_id", &args.object_id)?;
    confine_artifact(&args.artifact_path)?;
    let lowering =
        crate::object_editor::lower_item_from_document_path(&args.document_path, &args.object_id)?;
    let toml_readback =
        crate::object_editor::write_lowered_item_toml(&args.artifact_path, &lowering.toml)?;
    Ok(output(
        route,
        "Calyx Aster object document -> #829 lowering valve -> fingerprinted litd/data item TOML re-read from disk",
        json!({
            "lowered_artifact": {
                "object_id": args.object_id,
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "item": toml_readback.document.item,
                "provenance": toml_readback.document.provenance,
                "ability_ref_resolution": lowering.ability_resolution,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn read_lowered_item(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadLoweredItemArgs = parse_args(READ_LOWERED_ITEM_TOOL, arguments)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("expected_fingerprint", &args.expected_fingerprint)?;
    let toml_readback = crate::object_editor::read_lowered_item_toml(
        &args.artifact_path,
        &args.expected_fingerprint,
    )?;
    Ok(output(
        route,
        "fingerprinted litd/data item TOML artifact re-read from disk + fingerprint verified",
        json!({
            "lowered_artifact": {
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "item": toml_readback.document.item,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn lower_shop(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: LowerShopArgs = parse_args(LOWER_SHOP_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("object_id", &args.object_id)?;
    confine_artifact(&args.artifact_path)?;
    // Cross-record validation (dangling item refs / recipe cycles) runs inside this lowering.
    let lowered =
        crate::object_editor::lower_shop_from_document_path(&args.document_path, &args.object_id)?;
    let toml_readback =
        crate::object_editor::write_lowered_shop_toml(&args.artifact_path, &lowered)?;
    Ok(output(
        route,
        "Calyx Aster object document -> #829 lowering valve (cross-record validated) -> fingerprinted litd/data commerce TOML re-read from disk",
        json!({
            "lowered_artifact": {
                "object_id": args.object_id,
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "commerce": toml_readback.document.commerce,
                "validation": toml_readback.document.validation,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn read_lowered_shop(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadLoweredShopArgs = parse_args(READ_LOWERED_SHOP_TOOL, arguments)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("expected_fingerprint", &args.expected_fingerprint)?;
    let toml_readback = crate::object_editor::read_lowered_shop_toml(
        &args.artifact_path,
        &args.expected_fingerprint,
    )?;
    Ok(output(
        route,
        "fingerprinted litd/data commerce TOML artifact re-read from disk + fingerprint verified",
        json!({
            "lowered_artifact": {
                "artifact_path": toml_readback.path,
                "fingerprint": toml_readback.fingerprint,
                "sha256": toml_readback.sha256,
                "bytes": toml_readback.bytes,
                "commerce": toml_readback.document.commerce,
                "validation": toml_readback.document.validation,
                "provenance": toml_readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn lower_tech_requirements(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: LowerTechRequirementsArgs = parse_args(LOWER_TECH_REQUIREMENTS_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("artifact_path", &args.artifact_path)?;
    confine_artifact(&args.artifact_path)?;
    let lowered = crate::object_editor::lower_tech_from_document_path(&args.document_path)?;
    let readback = crate::object_editor::write_lowered_tech_toml(&args.artifact_path, &lowered)?;
    Ok(output(
        route,
        "Calyx Aster object document -> #829 lowering valve -> graph-validated litd/data tech-tree TOML re-read from disk",
        json!({
            "lowered_artifact": {
                "artifact_path": readback.path,
                "fingerprint": readback.fingerprint,
                "sha256": readback.sha256,
                "bytes": readback.bytes,
                "tech_tree": readback.document.tech_tree,
                "validation": readback.document.validation,
                "provenance": readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn read_lowered_tech_requirements(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadLoweredTechRequirementsArgs =
        parse_args(READ_LOWERED_TECH_REQUIREMENTS_TOOL, arguments)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("expected_fingerprint", &args.expected_fingerprint)?;
    let readback = crate::object_editor::read_lowered_tech_toml(
        &args.artifact_path,
        &args.expected_fingerprint,
    )?;
    Ok(output(
        route,
        "graph-validated litd/data tech-tree TOML artifact re-read from disk + fingerprint verified",
        json!({
            "lowered_artifact": {
                "artifact_path": readback.path,
                "fingerprint": readback.fingerprint,
                "sha256": readback.sha256,
                "bytes": readback.bytes,
                "tech_tree": readback.document.tech_tree,
                "validation": readback.document.validation,
                "provenance": readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn lower_gameplay_constants(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: LowerGameplayConstantsArgs = parse_args(LOWER_GAMEPLAY_CONSTANTS_TOOL, arguments)?;
    validate_path("document_path", &args.document_path)?;
    validate_path("artifact_path", &args.artifact_path)?;
    confine_artifact(&args.artifact_path)?;
    let lowered = crate::object_editor::lower_constants_from_document_path(&args.document_path)?;
    let readback =
        crate::object_editor::write_lowered_constants_toml(&args.artifact_path, &lowered)?;
    Ok(output(
        route,
        "Calyx Aster object document -> #829 lowering valve -> fingerprinted litd/data gameplay-constants TOML re-read from disk",
        json!({
            "lowered_artifact": {
                "artifact_path": readback.path,
                "fingerprint": readback.fingerprint,
                "sha256": readback.sha256,
                "bytes": readback.bytes,
                "constants": readback.document.constants,
                "validation": readback.document.validation,
                "provenance": readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}

pub(super) fn read_lowered_gameplay_constants(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpObjectToolCallOutput, EngineError> {
    let args: ReadLoweredGameplayConstantsArgs =
        parse_args(READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL, arguments)?;
    validate_path("artifact_path", &args.artifact_path)?;
    validate_single_line("expected_fingerprint", &args.expected_fingerprint)?;
    let readback = crate::object_editor::read_lowered_constants_toml(
        &args.artifact_path,
        &args.expected_fingerprint,
    )?;
    Ok(output(
        route,
        "fingerprinted litd/data gameplay-constants TOML artifact re-read from disk + fingerprint verified",
        json!({
            "lowered_artifact": {
                "artifact_path": readback.path,
                "fingerprint": readback.fingerprint,
                "sha256": readback.sha256,
                "bytes": readback.bytes,
                "constants": readback.document.constants,
                "validation": readback.document.validation,
                "provenance": readback.document.provenance,
            }
        }),
        None,
        None,
    ))
}
