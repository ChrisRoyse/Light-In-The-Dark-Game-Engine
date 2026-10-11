//! Project lifecycle, headless playtest, package, and publish MCP dispatch.

use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::{
    history::stable_fingerprint, mcp_harness_tools::build_project_headless_playtest_verdict,
    mcp_tools::McpDomainTool,
};

use self::mcp_project_tools_store::{
    McpProjectStateReadback, McpProjectToolLedgerReadback, McpProjectToolRecord,
    load_or_create_mcp_project_state, write_litdworld_project, write_mcp_project_state,
    write_mcp_project_tool_ledger,
};

#[path = "mcp_project_aster_store.rs"]
mod mcp_project_aster_store;
#[path = "mcp_project_tools_campaign.rs"]
mod mcp_project_tools_campaign;
#[path = "mcp_project_tools_competition.rs"]
mod mcp_project_tools_competition;
#[path = "mcp_project_tools_content_trust.rs"]
mod mcp_project_tools_content_trust;
#[path = "mcp_project_tools_delivery.rs"]
mod mcp_project_tools_delivery;
#[path = "mcp_project_tools_hub.rs"]
mod mcp_project_tools_hub;
#[path = "mcp_project_tools_litdworld_archive.rs"]
mod mcp_project_tools_litdworld_archive;
#[path = "mcp_project_tools_moderation.rs"]
mod mcp_project_tools_moderation;
#[path = "mcp_project_tools_multiplayer.rs"]
mod mcp_project_tools_multiplayer;
#[path = "mcp_project_tools_publish.rs"]
mod mcp_project_tools_publish;
#[path = "mcp_project_tools_readback.rs"]
mod mcp_project_tools_readback;
#[path = "mcp_project_tools_record.rs"]
mod mcp_project_tools_record;
#[path = "mcp_project_tools_route.rs"]
mod mcp_project_tools_route;
#[path = "mcp_project_tools_server_browser.rs"]
mod mcp_project_tools_server_browser;
#[path = "mcp_project_tools_social.rs"]
mod mcp_project_tools_social;
#[path = "mcp_project_tools_store.rs"]
pub(crate) mod mcp_project_tools_store;
#[path = "mcp_project_tools_types.rs"]
mod mcp_project_tools_types;
#[path = "mcp_project_tools_updates.rs"]
mod mcp_project_tools_updates;
#[path = "mcp_project_tools_validation.rs"]
pub(crate) mod mcp_project_tools_validation;

// Test-only: let delivery/social fixtures set the confinement root (#1487) via a
// stable path without reaching into the private validation submodule.
#[cfg(test)]
pub(crate) use mcp_project_tools_validation::ensure_project_root_env_for_tests;

pub use mcp_project_tools_campaign::{
    AUTHOR_CAMPAIGN_CHAPTER_TOOL, AUTHOR_CAMPAIGN_TOOL, AuthorCampaignArgs,
    AuthorCampaignChapterArgs, READ_CAMPAIGN_TOOL, ReadCampaignArgs,
};
use mcp_project_tools_campaign::{author_campaign, author_campaign_chapter, read_campaign_tool};
use mcp_project_tools_competition::manage_leaderboards_reviews;

#[path = "mcp_project_tools_args.rs"]
mod mcp_project_tools_args;
use mcp_project_tools_args::*;
pub use mcp_project_tools_competition::{
    LeaderboardEntryRecord, LeaderboardRecord, LeaderboardSort, RANKINGS_REVIEWS_STATE_SCHEMA,
    RANKINGS_REVIEWS_TOOL, RankingsReviewsAction, RankingsReviewsState,
    RankingsReviewsStateReadback, RatingSummaryRecord, ReviewRecord, ReviewVoteRecord,
    ScoreUpdateMethod, read_rankings_reviews_state,
};
pub use mcp_project_tools_content_trust::{
    CONTENT_TRUST_TOOL, ContentTrustToolArgs, READ_CONTENT_TRUST_TOOL, ReadContentTrustArgs,
};
use mcp_project_tools_content_trust::{read_content_trust_gate, run_content_trust_gate};
use mcp_project_tools_delivery::deliver_litdgame;
pub use mcp_project_tools_delivery::{
    DELIVERY_LITDGAME_TOOL, DELIVERY_STORE_SCHEMA, DeliveryStoreManifest,
    DeliveryStoreManifestReadback, InstalledGameRecord, LITDGAME_INSTALL_MANIFEST_SCHEMA,
    LitdGameInstallManifest, LitdGameInstallManifestReadback, read_delivery_store_manifest,
    read_litdgame_install_manifest,
};
use mcp_project_tools_hub::open_hub_launcher;
pub use mcp_project_tools_hub::{
    HUB_LAUNCHER_STATE_SCHEMA, HUB_LAUNCHER_TOOL, HubGameCard, HubLaunchRecord, HubLauncherState,
    HubLauncherStateReadback, read_hub_launcher_state,
};
pub use mcp_project_tools_litdworld_archive::{
    LITDWORLD_ARCHIVE_SCHEMA, LitdWorldArchiveEntry, LitdWorldArchiveManifest,
    LitdWorldArchiveReadback, inspect_litdworld_archive,
};
use mcp_project_tools_moderation::manage_moderation_safety;
pub use mcp_project_tools_moderation::{
    AccessRuleKind, AccessRuleRecord, AdminActionKind, AdminActionRecord, AppealStatus,
    ConnectionDecisionRecord, ContentReportRecord, HostControlKind, HostControlRecord,
    MODERATION_SAFETY_STATE_SCHEMA, MODERATION_SAFETY_TOOL, ModerationAppealRecord,
    ModerationSafetyAction, ModerationSafetyState, ModerationSafetyStateReadback, ReportReason,
    ReportStatus, RoleplayPolicyEvaluationRecord, RoleplayPolicyMode, RoleplayPolicyRecord,
    RoleplayPolicyVerdict, ServerRole, ServerRoleAssignmentRecord, read_moderation_safety_state,
};
use mcp_project_tools_multiplayer::manage_multiplayer_session;
pub use mcp_project_tools_multiplayer::{
    AuthoritativeServerRecord, AuthoritativeServerStatus, MULTIPLAYER_SESSION_STATE_SCHEMA,
    MULTIPLAYER_SESSION_TOOL, MatchTicketRecord, MatchTicketStatus, MultiplayerAction,
    MultiplayerEndpoint, MultiplayerLobbyRecord, MultiplayerLobbySlotRecord,
    MultiplayerLobbyStatus, MultiplayerSessionState, MultiplayerSessionStateReadback,
    MultiplayerTransport, PartyRecord, ServerAuthorityMode, read_multiplayer_session_state,
};
use mcp_project_tools_publish::{package_project, publish_litdgame};
use mcp_project_tools_readback::{call_readback, validate_mutating_route, write_state_and_ledger};
use mcp_project_tools_record::project_record;
use mcp_project_tools_route::project_route;
use mcp_project_tools_server_browser::manage_server_browser;
pub use mcp_project_tools_server_browser::{
    SERVER_BROWSER_STATE_SCHEMA, SERVER_BROWSER_TOOL, ServerBrowserAction,
    ServerBrowserListingRecord, ServerBrowserQueryRecord, ServerBrowserResultRecord,
    ServerBrowserSort, ServerBrowserState, ServerBrowserStateReadback, ServerVisibility,
    read_server_browser_state,
};
use mcp_project_tools_social::manage_social_graph;
pub use mcp_project_tools_social::{
    AccountRecord, FriendshipRecord, FriendshipStatus, GameInviteRecord, GameInviteStatus,
    JoinConnectionMode, JoinConnectionRecord, PresenceRecord, PresenceStatus, ProfileRecord,
    SOCIAL_GRAPH_STATE_SCHEMA, SOCIAL_GRAPH_TOOL, SocialGraphAction, SocialGraphState,
    SocialGraphStateReadback, read_social_graph_state,
};
pub use mcp_project_tools_store::{
    ECOSYSTEM_PUBLISH_CATALOG_SCHEMA, LitdWorldProjectReadback, MCP_PROJECT_LEDGER_SCHEMA,
    MCP_PROJECT_STATE_SCHEMA, McpProjectToolLedger, read_litdworld_project,
    read_litdworld_project_readback, read_mcp_project_state, read_mcp_project_tool_ledger,
    read_publish_catalog, validate_mcp_project_state, validate_mcp_project_tool_ledger,
};
pub use mcp_project_tools_types::{
    EcosystemPublishCatalog, HeadlessPlaytestRecord, LitdGamePackageRecord, LitdWorldProject,
    NewOpenSaveProjectArgs, PackageLitdGameArgs, ProjectLifecycleAction, ProjectLifecycleRecord,
    PublishLitdGameArgs, PublishedGameRecord, ReadProjectArgs, RunHeadlessPlaytestArgs,
};
use mcp_project_tools_updates::manage_game_updates;
pub use mcp_project_tools_updates::{
    CompatibilityRecord, GAME_UPDATES_STATE_SCHEMA, GAME_UPDATES_TOOL, GameUpdateAction,
    GameUpdateReleaseRecord, GameUpdatesState, GameUpdatesStateReadback, UpdateChannelRecord,
    UpdateNotificationRecord, UpdateNotificationStatus, read_game_updates_state,
};
use mcp_project_tools_validation::{
    validate_playtest_ticks, validate_project_metadata, validate_request_identity,
};

pub const MCP_LITDWORLD_SCHEMA: &str = "litd.project.litdworld.v1";
pub const MCP_LITDWORLD_SOURCE_OF_TRUTH: &str = "Calyx Aster LitD project document";
pub const NEW_OPEN_SAVE_TOOL: &str = "litd.editor.project.new_open_save";
pub const RUN_HEADLESS_PLAYTEST_TOOL: &str = "litd.editor.play.run_headless_playtest";
pub const PACKAGE_LITDGAME_TOOL: &str = "litd.editor.project.package_litdgame";
pub const PUBLISH_LITDGAME_TOOL: &str = "litd.editor.project.publish_litdgame";
pub const READ_PROJECT_TOOL: &str = "litd.editor.project.read_project";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpProjectToolCallReadback {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub state_readback: McpProjectStateReadback,
    pub ledger_readback: Option<McpProjectToolLedgerReadback>,
}

pub fn call_mcp_project_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    let route = project_route(tool_name)?;
    match tool_name {
        NEW_OPEN_SAVE_TOOL => new_open_save_project(route, arguments),
        RUN_HEADLESS_PLAYTEST_TOOL => run_headless_playtest(route, arguments),
        PACKAGE_LITDGAME_TOOL => package_litdgame(route, arguments),
        PUBLISH_LITDGAME_TOOL => publish_litdgame(route, arguments),
        DELIVERY_LITDGAME_TOOL => deliver_litdgame(route, arguments),
        HUB_LAUNCHER_TOOL => open_hub_launcher(route, arguments),
        MULTIPLAYER_SESSION_TOOL => manage_multiplayer_session(route, arguments),
        SOCIAL_GRAPH_TOOL => manage_social_graph(route, arguments),
        RANKINGS_REVIEWS_TOOL => manage_leaderboards_reviews(route, arguments),
        GAME_UPDATES_TOOL => manage_game_updates(route, arguments),
        MODERATION_SAFETY_TOOL => manage_moderation_safety(route, arguments),
        CONTENT_TRUST_TOOL => run_content_trust_gate(route, arguments),
        READ_CONTENT_TRUST_TOOL => read_content_trust_gate(route, arguments),
        SERVER_BROWSER_TOOL => manage_server_browser(route, arguments),
        AUTHOR_CAMPAIGN_TOOL => author_campaign(route, arguments),
        AUTHOR_CAMPAIGN_CHAPTER_TOOL => author_campaign_chapter(route, arguments),
        READ_CAMPAIGN_TOOL => read_campaign_tool(route, arguments),
        READ_PROJECT_TOOL => read_project(route, arguments),
        other => Err(invalid_mcp_project(
            "tool_name",
            format!("{other} is not implemented by #926 core project tools"),
        )),
    }
}

fn new_open_save_project(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: NewOpenSaveProjectArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", Path::new(&args.state_path)),
        ("ledger_path", Path::new(&args.ledger_path)),
        ("project_path", Path::new(&args.project_path)),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_project_metadata(
        &args.project_id,
        &args.title,
        &args.version,
        &args.tags,
        &args.snapshot_label,
    )?;
    let project_path = Path::new(&args.project_path);
    let project = match args.action {
        ProjectLifecycleAction::Open => read_litdworld_project(project_path)?,
        ProjectLifecycleAction::New | ProjectLifecycleAction::SaveSnapshot => {
            let document = args.document.ok_or_else(|| {
                invalid_mcp_project("project.document", "document is required for new/save")
            })?;
            document.validate()?;
            let project = LitdWorldProject {
                schema: MCP_LITDWORLD_SCHEMA.to_string(),
                issue: 926,
                project_id: args.project_id.clone(),
                title: args.title.clone(),
                version: args.version.clone(),
                tags: args.tags.clone(),
                source_of_truth: MCP_LITDWORLD_SOURCE_OF_TRUTH.to_string(),
                snapshot_label: args.snapshot_label.clone(),
                document_fingerprint: stable_fingerprint(&document)?,
                document,
            };
            write_litdworld_project(project_path, &project)?
        }
    };
    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    let record = project_record(&project, project_path);
    upsert_by(
        &mut state.projects,
        |existing| existing.project_id == record.project_id,
        record.clone(),
    );
    state.update_count = state.update_count.saturating_add(1);
    write_with_ledger(WriteWithLedgerArgs {
        route: &route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: &args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "saved/opened LitD world project with source-of-truth readback",
        affected_ids: vec![record.project_id.clone()],
        state,
        result: json!({ "project": record }),
    })
}

fn run_headless_playtest(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: RunHeadlessPlaytestArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", Path::new(&args.state_path)),
        ("ledger_path", Path::new(&args.ledger_path)),
        ("project_path", Path::new(&args.project_path)),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_playtest_ticks(args.ticks)?;
    let run_id = format!("{}-run", args.request_id);
    let oracle_run_id = format!("{}-oracle", args.request_id);
    let evaluation = build_project_headless_playtest_verdict(
        &run_id,
        &oracle_run_id,
        &args.project_path,
        args.camera_rig,
        args.ticks,
    )?;
    let run = evaluation.run;
    let oracle_verdict = evaluation.oracle_verdict;
    let record = HeadlessPlaytestRecord {
        project_id: run.project_id.clone(),
        project_path: args.project_path.clone(),
        camera_rig: args.camera_rig,
        player_id: evaluation.player_id,
        start_tick: run.start_tick,
        final_tick: run.final_tick,
        steps: run.steps,
        bodies_updated: run.bodies_updated,
        initial_state_hash: run.initial_state_hash,
        final_state_hash: run.final_state_hash,
        spawned_bodies: run.spawned_bodies,
        visible_instance_count: run.visible_instance_count,
        draw_batches: run.draw_batches,
        verdict: oracle_verdict.verdict.clone(),
        oracle_verdict,
        verdict_reason: evaluation.verdict_reason,
    };
    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    upsert_by(
        &mut state.playtests,
        |existing| existing.project_id == record.project_id,
        record.clone(),
    );
    state.update_count = state.update_count.saturating_add(1);
    write_with_ledger(WriteWithLedgerArgs {
        route: &route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: &args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "ran deterministic headless playtest and persisted verdict",
        affected_ids: vec![record.project_id.clone()],
        state,
        result: json!({ "playtest": record }),
    })
}

fn package_litdgame(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_PROJECT_TOOL)?;
    let args: PackageLitdGameArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", Path::new(&args.state_path)),
        ("ledger_path", Path::new(&args.ledger_path)),
        ("project_path", Path::new(&args.project_path)),
        ("package_path", Path::new(&args.package_path)),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let project = read_litdworld_project(&args.project_path)?;
    let record = package_project(
        &project,
        &args.package_path,
        &args.share_id,
        &args.created_by,
        args.host_port,
        args.max_package_bytes,
    )?;
    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    upsert_by(
        &mut state.packages,
        |existing| existing.package_id == record.package_id,
        record.clone(),
    );
    state.update_count = state.update_count.saturating_add(1);
    write_with_ledger(WriteWithLedgerArgs {
        route: &route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: &args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "packaged runtime-locked .litdgame artifact and verified readback",
        affected_ids: vec![record.project_id.clone(), record.package_id.clone()],
        state,
        result: json!({ "package": record }),
    })
}

fn read_project(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    if route.mutates_state {
        return Err(invalid_mcp_project(
            "route",
            "read_project cannot mutate state",
        ));
    }
    let args: ReadProjectArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine the read path to the project root before reading from disk.
    mcp_project_tools_validation::confine_tool_paths(&[(
        "state_path",
        Path::new(&args.state_path),
    )])?;
    let state_readback = read_mcp_project_state(&args.state_path)?;
    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: "Calyx Aster MCP project/playtest/package state".to_string(),
        result: json!({ "state_readback": state_readback }),
        state_readback,
        ledger_readback: None,
    })
}

pub(super) fn parse_args<T: DeserializeOwned>(
    tool_name: &str,
    arguments: Value,
) -> Result<T, EngineError> {
    serde_json::from_value(arguments).map_err(|error| invalid_mcp_project(tool_name, error))
}

pub(super) fn upsert_by<T, F>(values: &mut Vec<T>, mut predicate: F, value: T)
where
    F: FnMut(&T) -> bool,
{
    if let Some(existing) = values.iter_mut().find(|existing| predicate(existing)) {
        *existing = value;
    } else {
        values.push(value);
    }
}

pub(super) fn invalid_mcp_project(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> EngineError {
    let field = format!("editor.mcp_project_tools.{}", field.into());
    tracing::error!(
        target: "litd_editor::mcp_project_tools",
        field,
        reason = %reason,
        "invalid project/playtest MCP tool call"
    );
    EngineError::UiRenderInvalidConfig {
        field,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
#[path = "mcp_project_tools_test_modules.rs"]
mod test_modules;
