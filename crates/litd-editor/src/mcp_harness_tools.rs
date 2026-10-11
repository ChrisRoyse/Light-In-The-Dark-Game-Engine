//! Agent harness MCP tools over deterministic playtest state.

use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths;
use crate::mcp_tools::McpDomainTool;

use self::{
    mcp_harness_tools_assertions::build_assertion_verdict,
    mcp_harness_tools_bot::build_bot_record,
    mcp_harness_tools_bot_validation::validate_bot_args,
    mcp_harness_tools_build::{build_run_record, build_snapshot_branch_record},
    mcp_harness_tools_calyx_signals::{build_calyx_signals_record, validate_calyx_signals_record},
    mcp_harness_tools_feedback::{
        emit_feedback, feedback_from_assertion_verdict, feedback_from_bot_drive_record,
        feedback_from_calyx_signals, feedback_from_oracle_run, feedback_from_pev_transcript,
    },
    mcp_harness_tools_finish::{
        FinishMutationArgs, finish_mutation, finish_mutation_with_feedback,
        finish_mutation_with_transcript_and_feedback,
    },
    mcp_harness_tools_helpers::{
        harness_route, output, parse_args, upsert_by, validate_mutating_route,
    },
    mcp_harness_tools_oracles::build_oracle_record,
    mcp_harness_tools_pev_loop::build_pev_loop_transcript,
    mcp_harness_tools_sandbox::run_sandboxes,
    mcp_harness_tools_store::{
        McpHarnessLedgerReadback, McpHarnessState, McpHarnessStateReadback,
        load_or_create_mcp_harness_state,
    },
    mcp_harness_tools_validation::{
        invalid_harness, validate_assert_args, validate_calyx_args, validate_oracle_args,
        validate_pev_args, validate_read_events_args, validate_run_args, validate_screenshot_args,
        validate_snapshot_args,
    },
    mcp_harness_tools_visual::build_screenshot_diff_record,
};

use mcp_harness_tools_handlers::*;

#[path = "mcp_harness_tools_assertions.rs"]
mod mcp_harness_tools_assertions;
#[path = "mcp_harness_tools_bot.rs"]
mod mcp_harness_tools_bot;
#[path = "mcp_harness_tools_bot_types.rs"]
mod mcp_harness_tools_bot_types;
#[path = "mcp_harness_tools_bot_validation.rs"]
mod mcp_harness_tools_bot_validation;
#[path = "mcp_harness_tools_build.rs"]
mod mcp_harness_tools_build;
#[path = "mcp_harness_tools_calyx_signals.rs"]
mod mcp_harness_tools_calyx_signals;
#[path = "mcp_harness_tools_feedback.rs"]
mod mcp_harness_tools_feedback;
#[path = "mcp_harness_tools_finish.rs"]
mod mcp_harness_tools_finish;
#[path = "mcp_harness_tools_handlers.rs"]
mod mcp_harness_tools_handlers;
#[path = "mcp_harness_tools_helpers.rs"]
mod mcp_harness_tools_helpers;
#[path = "mcp_harness_tools_oracle_types.rs"]
mod mcp_harness_tools_oracle_types;
#[path = "mcp_harness_tools_oracles.rs"]
mod mcp_harness_tools_oracles;
#[path = "mcp_harness_tools_pev_loop.rs"]
mod mcp_harness_tools_pev_loop;
#[path = "mcp_harness_tools_project_playtest.rs"]
mod mcp_harness_tools_project_playtest;
#[path = "mcp_harness_tools_run_support.rs"]
mod mcp_harness_tools_run_support;
#[path = "mcp_harness_tools_sandbox.rs"]
mod mcp_harness_tools_sandbox;
#[path = "mcp_harness_tools_store.rs"]
pub(crate) mod mcp_harness_tools_store;
#[path = "mcp_harness_tools_types.rs"]
mod mcp_harness_tools_types;
#[path = "mcp_harness_tools_validation.rs"]
mod mcp_harness_tools_validation;
#[path = "mcp_harness_tools_visual.rs"]
mod mcp_harness_tools_visual;

pub use mcp_harness_tools_bot_types::*;
pub use mcp_harness_tools_oracle_types::*;
pub use mcp_harness_tools_pev_loop::validate_harness_pev_transcript;
pub(crate) use mcp_harness_tools_project_playtest::build_project_headless_playtest_verdict;
pub use mcp_harness_tools_sandbox::{
    HarnessSandboxBatchRecord, HarnessSandboxInput, HarnessSandboxResult, HarnessSandboxStatus,
    RunSandboxesHarnessArgs,
};
pub use mcp_harness_tools_store::{
    MCP_HARNESS_LEDGER_SCHEMA, MCP_HARNESS_STATE_SCHEMA, McpHarnessLedger,
    read_mcp_harness_aster_ledger, read_mcp_harness_ledger, read_mcp_harness_state,
    validate_mcp_harness_ledger, validate_mcp_harness_state,
};
pub use mcp_harness_tools_types::*;

pub const RUN_HEADLESS_TOOL: &str = "litd.editor.harness.run_headless";
pub const RUN_SANDBOXES_TOOL: &str = "litd.editor.harness.run_sandboxes";
pub const SNAPSHOT_BRANCH_TOOL: &str = "litd.editor.harness.snapshot_restore_branch";
pub const ASSERT_STATE_TOOL: &str = "litd.editor.harness.assert_state";
pub const READ_EVENTS_TOOL: &str = "litd.editor.harness.read_events";
pub const RUN_ORACLES_TOOL: &str = "litd.editor.harness.run_oracles";
pub const DRIVE_BOT_TOOL: &str = "litd.editor.harness.drive_bot";
pub const SCREENSHOT_DIFF_TOOL: &str = "litd.editor.harness.screenshot_diff";
pub const CALYX_SIGNALS_TOOL: &str = "litd.editor.harness.calyx_signals";
pub const EMIT_FEEDBACK_TOOL: &str = "litd.editor.harness.emit_feedback";
pub const RUN_PEV_TOOL: &str = "litd.editor.harness.run_pev";
pub const READ_HARNESS_TOOL: &str = "litd.editor.harness.read_harness";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpHarnessToolCallReadback {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub state_readback: Option<McpHarnessStateReadback>,
    pub ledger_readback: Option<McpHarnessLedgerReadback>,
}

pub fn call_mcp_harness_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpHarnessToolCallReadback, EngineError> {
    let route = harness_route(tool_name)?;
    match tool_name {
        RUN_HEADLESS_TOOL => run_headless(route, arguments),
        RUN_SANDBOXES_TOOL => run_sandboxes(route, arguments),
        SNAPSHOT_BRANCH_TOOL => snapshot_restore_branch(route, arguments),
        ASSERT_STATE_TOOL => assert_state(route, arguments),
        READ_EVENTS_TOOL => read_events(route, arguments),
        RUN_ORACLES_TOOL => run_oracles(route, arguments),
        DRIVE_BOT_TOOL => drive_bot(route, arguments),
        SCREENSHOT_DIFF_TOOL => screenshot_diff(route, arguments),
        CALYX_SIGNALS_TOOL => calyx_signals(route, arguments),
        EMIT_FEEDBACK_TOOL => emit_feedback(route, arguments),
        RUN_PEV_TOOL => run_pev(route, arguments),
        READ_HARNESS_TOOL => read_harness(route, arguments),
        other => invalid_harness(
            "tool_name",
            format!("{other} is not implemented by #1178 agent harness tools"),
        ),
    }
}

#[cfg(test)]
#[path = "mcp_harness_tools_test_modules.rs"]
mod test_modules;
