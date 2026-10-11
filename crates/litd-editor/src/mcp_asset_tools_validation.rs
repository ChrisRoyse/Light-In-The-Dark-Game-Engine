use std::collections::BTreeSet;

use litd_engine::EngineError;

use super::{
    AssetBudgetConformance, AssetPipelineRecord, AssetPreparation, invalid_mcp_asset,
    mcp_asset_tools_store::McpAssetState,
};

const MAX_TEXT_LEN: usize = 8192;
const MAX_LIST_LEN: usize = 128;

pub(super) fn validate_request_identity(
    request_id: &str,
    requested_by: &str,
) -> Result<(), EngineError> {
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)
}

pub(super) fn validate_asset(asset: &AssetPipelineRecord) -> Result<(), EngineError> {
    validate_id("asset.asset_id", &asset.asset_id)?;
    validate_single_line("asset.display_name", &asset.display_name)?;
    validate_id("asset.asset_type", &asset.asset_type)?;
    match asset.source_kind.as_str() {
        "generated" | "imported" => {}
        other => {
            return Err(invalid_mcp_asset(
                "asset.source_kind",
                format!("unsupported source kind {other}"),
            ));
        }
    }
    validate_text("asset.prompt", &asset.prompt)?;
    validate_single_line("asset.source_ref", &asset.source_ref)?;
    validate_sha256("asset.content_sha256", &asset.content_sha256)?;
    validate_single_line("asset.license", &asset.license)?;
    if asset.provenance_refs.is_empty() {
        return Err(invalid_mcp_asset(
            "asset.provenance_refs",
            "at least one provenance ref is required",
        ));
    }
    validate_id_list("asset.provenance_refs", &asset.provenance_refs)
}

pub(super) fn validate_preparation(
    state: &McpAssetState,
    preparation: &AssetPreparation,
) -> Result<(), EngineError> {
    validate_asset_exists(state, &preparation.asset_id, "preparation.asset_id")?;
    validate_single_line("preparation.mesh_ref", &preparation.mesh_ref)?;
    validate_single_line("preparation.rig_ref", &preparation.rig_ref)?;
    if preparation.texture_refs.is_empty() || preparation.animation_refs.is_empty() {
        return Err(invalid_mcp_asset(
            "preparation.refs",
            "texture and animation refs are required",
        ));
    }
    if !preparation.ready_for_runtime {
        return Err(invalid_mcp_asset(
            "preparation.ready_for_runtime",
            "prepared assets must be marked runtime-ready",
        ));
    }
    validate_id_list("preparation.texture_refs", &preparation.texture_refs)?;
    validate_id_list("preparation.animation_refs", &preparation.animation_refs)
}

pub(super) fn validate_budget(
    state: &McpAssetState,
    budget: &AssetBudgetConformance,
) -> Result<(), EngineError> {
    validate_asset_exists(state, &budget.asset_id, "budget.asset_id")?;
    if !state
        .preparations
        .iter()
        .any(|preparation| preparation.asset_id == budget.asset_id)
    {
        return Err(invalid_mcp_asset(
            "budget.asset_id",
            format!("missing runtime-ready preparation {}", budget.asset_id),
        ));
    }
    validate_single_line("budget.calyx_ingest_ref", &budget.calyx_ingest_ref)?;
    for (field, value, max) in [
        (
            "budget.triangle_count",
            budget.triangle_count as u64,
            budget.max_triangle_count as u64,
        ),
        (
            "budget.material_count",
            budget.material_count as u64,
            budget.max_material_count as u64,
        ),
        (
            "budget.texture_bytes",
            budget.texture_bytes,
            budget.max_texture_bytes,
        ),
        (
            "budget.draw_call_estimate",
            budget.draw_call_estimate as u64,
            budget.max_draw_calls as u64,
        ),
    ] {
        if value == 0 || max == 0 || value > max {
            return Err(invalid_mcp_asset(
                field,
                format!("actual {value} must be in 1..={max}"),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_asset_state(state: &McpAssetState) -> Result<(), EngineError> {
    let mut asset_ids = BTreeSet::new();
    for asset in &state.assets {
        validate_asset(asset)?;
        if !asset_ids.insert(asset.asset_id.as_str()) {
            return Err(invalid_mcp_asset(
                "state.assets",
                format!("duplicate asset {}", asset.asset_id),
            ));
        }
    }
    let mut prepared = BTreeSet::new();
    for preparation in &state.preparations {
        validate_preparation(state, preparation)?;
        if !prepared.insert(preparation.asset_id.as_str()) {
            return Err(invalid_mcp_asset(
                "state.preparations",
                format!("duplicate preparation {}", preparation.asset_id),
            ));
        }
    }
    let mut budgets = BTreeSet::new();
    for budget in &state.budgets {
        validate_budget(state, budget)?;
        if !budgets.insert(budget.asset_id.as_str()) {
            return Err(invalid_mcp_asset(
                "state.budgets",
                format!("duplicate budget {}", budget.asset_id),
            ));
        }
    }
    Ok(())
}

fn validate_asset_exists(
    state: &McpAssetState,
    asset_id: &str,
    field: &str,
) -> Result<(), EngineError> {
    validate_id(field, asset_id)?;
    if !state.assets.iter().any(|asset| asset.asset_id == asset_id) {
        return Err(invalid_mcp_asset(
            field,
            format!("missing asset {asset_id}"),
        ));
    }
    Ok(())
}

fn validate_id_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    if values.len() > MAX_LIST_LEN {
        return Err(invalid_mcp_asset(
            field,
            format!("list length must be <= {MAX_LIST_LEN}"),
        ));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_id(field, value)?;
        if !seen.insert(value.as_str()) {
            return Err(invalid_mcp_asset(field, format!("duplicate value {value}")));
        }
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    if !litd_hash::is_sha256_hex(value) {
        return Err(invalid_mcp_asset(
            field,
            "value must be a 64-character lowercase SHA-256 hex",
        ));
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains('\0') {
        return Err(invalid_mcp_asset(
            field,
            "value must be bounded non-empty text",
        ));
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_asset(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_mcp_asset(field, "id cannot be empty"));
    };
    if !(first.is_ascii_alphanumeric() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(invalid_mcp_asset(
            field,
            "id must use ASCII letters, digits, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}
