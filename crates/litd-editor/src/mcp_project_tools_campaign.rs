//! #853: campaign-editor MCP tools — author a campaign, author/insert a chapter, and
//! read back the persisted campaign structure. Campaign data is the single source of
//! truth in its own Calyx Aster vault; these tools also record into the project
//! state/ledger like every other guarded project route.

use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::campaign::{
    Campaign, CampaignChapter, CampaignStoreRecord, read_campaign, read_campaign_ledger,
    write_campaign, write_campaign_ledger,
};
use crate::mcp_tools::McpDomainTool;

use super::{
    McpProjectToolCallReadback, invalid_mcp_project,
    mcp_project_tools_readback::{call_readback, validate_mutating_route, write_state_and_ledger},
    mcp_project_tools_store::{load_or_create_mcp_project_state, read_mcp_project_state},
    mcp_project_tools_validation, parse_args,
};

pub const AUTHOR_CAMPAIGN_TOOL: &str = "litd.editor.project.author_campaign";
pub const AUTHOR_CAMPAIGN_CHAPTER_TOOL: &str = "litd.editor.project.author_campaign_chapter";
pub const READ_CAMPAIGN_TOOL: &str = "litd.editor.project.read_campaign";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorCampaignArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub campaign_path: String,
    pub campaign_ledger_path: String,
    pub campaign_store_ledger_id: String,
    pub campaign: Campaign,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorCampaignChapterArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub campaign_path: String,
    pub campaign_ledger_path: String,
    pub campaign_store_ledger_id: String,
    #[serde(default)]
    pub entry_chapter_id: Option<String>,
    pub chapter: CampaignChapter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadCampaignArgs {
    pub state_path: String,
    pub campaign_path: String,
    pub campaign_ledger_path: String,
}

pub(super) fn author_campaign(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_CAMPAIGN_TOOL)?;
    let args: AuthorCampaignArgs = parse_args(&route.tool_name, arguments)?;
    confine(&[
        ("state_path", &args.state_path),
        ("ledger_path", &args.ledger_path),
        ("campaign_path", &args.campaign_path),
        ("campaign_ledger_path", &args.campaign_ledger_path),
    ])?;
    persist_campaign(
        &route,
        PersistArgs {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            state_path: &args.state_path,
            state_id: &args.state_id,
            ledger_path: &args.ledger_path,
            ledger_id: &args.ledger_id,
            campaign_path: &args.campaign_path,
            campaign_ledger_path: &args.campaign_ledger_path,
            campaign_store_ledger_id: &args.campaign_store_ledger_id,
            campaign: args.campaign,
            action_summary: "authored campaign and persisted to Calyx Aster vault",
        },
    )
}

pub(super) fn author_campaign_chapter(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_CAMPAIGN_TOOL)?;
    let args: AuthorCampaignChapterArgs = parse_args(&route.tool_name, arguments)?;
    confine(&[
        ("state_path", &args.state_path),
        ("ledger_path", &args.ledger_path),
        ("campaign_path", &args.campaign_path),
        ("campaign_ledger_path", &args.campaign_ledger_path),
    ])?;
    let mut campaign = read_campaign(&args.campaign_path)?.campaign;
    if let Some(entry) = args.entry_chapter_id {
        campaign.entry_chapter_id = entry;
    }
    campaign.upsert_chapter(args.chapter);
    persist_campaign(
        &route,
        PersistArgs {
            request_id: &args.request_id,
            requested_by: &args.requested_by,
            state_path: &args.state_path,
            state_id: &args.state_id,
            ledger_path: &args.ledger_path,
            ledger_id: &args.ledger_id,
            campaign_path: &args.campaign_path,
            campaign_ledger_path: &args.campaign_ledger_path,
            campaign_store_ledger_id: &args.campaign_store_ledger_id,
            campaign,
            action_summary: "inserted/updated campaign chapter and re-persisted campaign",
        },
    )
}

pub(super) fn read_campaign_tool(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    if route.mutates_state {
        return Err(invalid_mcp_project(
            "route",
            "read_campaign cannot mutate state",
        ));
    }
    let args: ReadCampaignArgs = parse_args(&route.tool_name, arguments)?;
    confine(&[
        ("state_path", &args.state_path),
        ("campaign_path", &args.campaign_path),
        ("campaign_ledger_path", &args.campaign_ledger_path),
    ])?;
    let campaign_readback = read_campaign(&args.campaign_path)?;
    let ledger_readback = read_campaign_ledger(&args.campaign_ledger_path)?;
    let state_readback = read_mcp_project_state(&args.state_path)?;
    Ok(call_readback(
        &route,
        "Calyx Aster campaign document row + campaign write ledger row",
        json!({
            "campaign_readback": campaign_readback,
            "campaign_store_ledger_readback": ledger_readback
        }),
        state_readback,
        None,
    ))
}

struct PersistArgs<'a> {
    request_id: &'a str,
    requested_by: &'a str,
    state_path: &'a str,
    state_id: &'a str,
    ledger_path: &'a str,
    ledger_id: &'a str,
    campaign_path: &'a str,
    campaign_ledger_path: &'a str,
    campaign_store_ledger_id: &'a str,
    campaign: Campaign,
    action_summary: &'a str,
}

fn persist_campaign(
    route: &McpDomainTool,
    args: PersistArgs<'_>,
) -> Result<McpProjectToolCallReadback, EngineError> {
    args.campaign.validate()?;
    let campaign_readback = write_campaign(args.campaign_path, &args.campaign)?;
    let store_ledger_readback = write_campaign_ledger(
        args.campaign_ledger_path,
        args.campaign_store_ledger_id,
        CampaignStoreRecord {
            request_id: args.request_id.to_string(),
            requested_by: args.requested_by.to_string(),
            action_summary: args.action_summary.to_string(),
            campaign_id: args.campaign.campaign_id.clone(),
            campaign_sha256: campaign_readback.sha256.clone(),
            campaign_bytes: campaign_readback.bytes,
            campaign_fingerprint: campaign_readback.campaign_fingerprint.clone(),
            chapter_count: campaign_readback.chapter_count,
        },
    )?;
    let mut state = load_or_create_mcp_project_state(args.state_path, args.state_id)?;
    state.update_count = state.update_count.saturating_add(1);
    let (state_readback, project_ledger_readback) = write_state_and_ledger(
        route,
        args.state_path,
        args.ledger_path,
        args.ledger_id,
        args.request_id,
        args.requested_by,
        args.action_summary,
        vec![args.campaign.campaign_id.clone()],
        state,
    )?;
    Ok(call_readback(
        route,
        "Calyx Aster campaign document row + campaign write ledger + project state/ledger",
        json!({
            "campaign_readback": campaign_readback,
            "campaign_store_ledger_readback": store_ledger_readback
        }),
        state_readback,
        Some(project_ledger_readback),
    ))
}

fn confine(paths: &[(&str, &str)]) -> Result<(), EngineError> {
    let confined: Vec<(&str, &Path)> = paths
        .iter()
        .map(|(field, value)| (*field, Path::new(*value)))
        .collect();
    mcp_project_tools_validation::confine_tool_paths(&confined)
}
