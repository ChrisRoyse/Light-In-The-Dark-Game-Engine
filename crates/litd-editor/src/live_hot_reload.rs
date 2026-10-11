use litd_engine::{EngineError, math::FixedVec3, sim::SimBodyId};
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{HotReloadPlan, PlayCameraRig, PlaySession, plan_hot_reload},
};

#[path = "live_hot_reload_patch.rs"]
mod live_hot_reload_patch;
#[path = "live_hot_reload_store.rs"]
mod live_hot_reload_store;
#[path = "live_hot_reload_support.rs"]
mod live_hot_reload_support;

pub use live_hot_reload_store::{
    LiveHotReloadAsterReadback, parse_live_hot_reload_ledger, read_live_hot_reload_ledger,
    write_live_hot_reload_ledger,
};

use live_hot_reload_support::{
    apply_patch_to_document, patch_reflected, player_body_id, runtime_readback,
};

pub const LIVE_HOT_RELOAD_SCHEMA: &str = "litd.live_hot_reload.readback.v1";
pub const LIVE_HOT_RELOAD_LEDGER_SCHEMA: &str = "litd.live_hot_reload.ledger.v1";
pub const LIVE_HOT_RELOAD_GUI_ROUTE: &str = "egui://editor.project/live_hot_reload";
pub const LIVE_HOT_RELOAD_CAPTURE_TOOL: &str = "litd.editor.play.capture_hot_reload";
pub const LIVE_HOT_RELOAD_READBACK_TOOL: &str = "litd.editor.play.read_hot_reload";
pub const LIVE_HOT_RELOAD_NOTICE_STRUCTURAL: &str = "structural_change_requires_restart";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveHotReloadChangeClass {
    SafeLivePatch,
    StructuralRestartRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveHotReloadStatus {
    AppliedLive,
    DeferredWithNotice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveHotReloadRequest {
    pub ledger_id: String,
    pub session_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub player_object_id: String,
    pub camera_rig: PlayCameraRig,
    pub pre_reload_ticks: u64,
    pub runtime_velocity: FixedVec3,
    pub patches: Vec<LiveHotReloadPatch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveHotReloadPatch {
    UnitStat {
        patch_id: String,
        object_id: String,
        stat_key: String,
        value: String,
    },
    TriggerFlag {
        patch_id: String,
        object_id: String,
        trigger_id: String,
        enabled: bool,
    },
    TextureMaterial {
        patch_id: String,
        object_id: String,
        material_id: String,
    },
    /// #902: live dialogue line text (property on a Dialogue object).
    DialogueLine {
        patch_id: String,
        object_id: String,
        line: String,
    },
    /// #902: live cinematic subtitle text on an authored timeline.
    CinematicSubtitle {
        patch_id: String,
        timeline_id: String,
        text: String,
    },
    StructuralAddUnit {
        patch_id: String,
        object_id: String,
        parent_id: Option<String>,
        position: FixedVec3,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRuntimeSnapshot {
    pub player_object_id: String,
    pub player_body_id: u32,
    pub tick: u64,
    pub state_hash: u64,
    pub player_position: FixedVec3,
    pub body_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveHotReloadStep {
    pub sequence: usize,
    pub patch_id: String,
    pub class: LiveHotReloadChangeClass,
    pub status: LiveHotReloadStatus,
    pub notice_code: Option<String>,
    pub plan: HotReloadPlan,
    pub generation_before: u64,
    pub generation_after: u64,
    pub runtime_before: LiveRuntimeSnapshot,
    pub runtime_after: LiveRuntimeSnapshot,
    pub player_position_preserved: bool,
    pub live_world_reflects_patch: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveHotReloadReadback {
    pub schema: String,
    pub ledger_id: String,
    pub session_id: String,
    pub project_id: String,
    pub source_document_fingerprint: String,
    pub final_document_fingerprint: String,
    pub final_play_build_fingerprint: String,
    pub patch_count: usize,
    pub applied_live_count: usize,
    pub deferred_structural_count: usize,
    pub rapid_reload_count: usize,
    pub player_position_preserved_all: bool,
    pub safe_patches_reflected_all: bool,
    pub final_generation: u64,
    pub final_runtime_state_hash: u64,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveHotReloadLedger {
    pub schema: String,
    pub ledger_id: String,
    pub session_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
    pub player_object_id: String,
    pub camera_rig: PlayCameraRig,
    pub source_document: EditorDocument,
    pub final_document: EditorDocument,
    pub initial_play_build_fingerprint: String,
    pub final_play_build_fingerprint: String,
    pub initial_runtime: LiveRuntimeSnapshot,
    pub final_runtime: LiveRuntimeSnapshot,
    pub steps: Vec<LiveHotReloadStep>,
    pub readback: LiveHotReloadReadback,
}

pub fn build_live_hot_reload_ledger(
    document: &EditorDocument,
    request: LiveHotReloadRequest,
) -> Result<LiveHotReloadLedger, EngineError> {
    validate_request(&request)?;
    document.validate()?;
    let mut session = PlaySession::start(document, request.camera_rig)?;
    let source_document = session.source_document.clone();
    let initial_play_build_fingerprint = stable_fingerprint(&session.build)?;
    let player_body_id = player_body_id(&session, &request.player_object_id)?;
    session.run_runtime_ticks_with_body_velocity(
        SimBodyId::from_raw(player_body_id),
        request.runtime_velocity,
        request.pre_reload_ticks,
    )?;
    let initial_runtime = runtime_readback(
        &request.player_object_id,
        player_body_id,
        &session.build.snapshot,
    )?;
    let mut current_document = source_document.clone();
    let mut current_runtime = initial_runtime.clone();
    let mut steps = Vec::with_capacity(request.patches.len());

    for (index, patch) in request.patches.iter().enumerate() {
        let generation_before = session.generation;
        let runtime_before = current_runtime.clone();
        let plan_document = apply_patch_to_document(&current_document, patch)?;
        let plan = plan_hot_reload(
            &session.build.source_fingerprint,
            &session.source_document,
            &plan_document,
        );
        let class = patch.class();
        let (status, notice_code, live_world_reflects_patch) = match class {
            LiveHotReloadChangeClass::SafeLivePatch => {
                let reload_plan = session.hot_reload(&plan_document)?;
                if reload_plan != plan {
                    return invalid_live_hot_reload(
                        "plan",
                        "hot reload plan changed between planning and application",
                    );
                }
                current_document = plan_document;
                (
                    LiveHotReloadStatus::AppliedLive,
                    None,
                    patch_reflected(&current_document, &session, player_body_id, patch)?,
                )
            }
            LiveHotReloadChangeClass::StructuralRestartRequired => (
                LiveHotReloadStatus::DeferredWithNotice,
                Some(LIVE_HOT_RELOAD_NOTICE_STRUCTURAL.to_string()),
                false,
            ),
        };
        let runtime_after = runtime_readback(
            &request.player_object_id,
            player_body_id,
            &session.build.snapshot,
        )?;
        let step = LiveHotReloadStep {
            sequence: index + 1,
            patch_id: patch.patch_id().to_string(),
            class,
            status,
            notice_code,
            plan,
            generation_before,
            generation_after: session.generation,
            player_position_preserved: runtime_before.player_position
                == runtime_after.player_position,
            runtime_before,
            runtime_after: runtime_after.clone(),
            live_world_reflects_patch,
        };
        validate_step(&step)?;
        current_runtime = runtime_after;
        steps.push(step);
    }

    let final_play_build_fingerprint = stable_fingerprint(&session.build)?;
    let ledger = LiveHotReloadLedger {
        schema: LIVE_HOT_RELOAD_LEDGER_SCHEMA.to_string(),
        ledger_id: request.ledger_id,
        session_id: request.session_id,
        project_id: request.project_id,
        actor_id: request.actor_id,
        gui_route: LIVE_HOT_RELOAD_GUI_ROUTE.to_string(),
        capture_tool_name: LIVE_HOT_RELOAD_CAPTURE_TOOL.to_string(),
        readback_tool_name: LIVE_HOT_RELOAD_READBACK_TOOL.to_string(),
        player_object_id: request.player_object_id,
        camera_rig: request.camera_rig,
        source_document,
        final_document: current_document,
        initial_play_build_fingerprint,
        final_play_build_fingerprint,
        initial_runtime,
        final_runtime: current_runtime,
        steps,
        readback: empty_readback(),
    };
    let ledger = LiveHotReloadLedger {
        readback: build_readback(&ledger)?,
        ..ledger
    };
    validate_live_hot_reload_ledger(&ledger)?;
    tracing::info!(
        ledger_id = %ledger.ledger_id,
        applied_live_count = ledger.readback.applied_live_count,
        deferred_structural_count = ledger.readback.deferred_structural_count,
        final_generation = ledger.readback.final_generation,
        code = "LITD-LIVE-HOT-RELOAD-BUILT",
        "live hot reload ledger built"
    );
    Ok(ledger)
}

pub fn validate_live_hot_reload_ledger(ledger: &LiveHotReloadLedger) -> Result<(), EngineError> {
    if ledger.schema != LIVE_HOT_RELOAD_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.session_id.trim().is_empty()
        || ledger.project_id.trim().is_empty()
        || ledger.actor_id.trim().is_empty()
        || ledger.gui_route != LIVE_HOT_RELOAD_GUI_ROUTE
        || ledger.capture_tool_name != LIVE_HOT_RELOAD_CAPTURE_TOOL
        || ledger.readback_tool_name != LIVE_HOT_RELOAD_READBACK_TOOL
        || ledger.steps.is_empty()
    {
        return invalid_live_hot_reload("ledger", "required ledger fields are invalid");
    }
    for (field, value) in [
        ("ledger_id", ledger.ledger_id.as_str()),
        ("session_id", ledger.session_id.as_str()),
        ("project_id", ledger.project_id.as_str()),
        ("actor_id", ledger.actor_id.as_str()),
        ("player_object_id", ledger.player_object_id.as_str()),
    ] {
        validate_single_line(field, value)?;
    }
    ledger.source_document.validate()?;
    ledger.final_document.validate()?;
    for (index, step) in ledger.steps.iter().enumerate() {
        if step.sequence != index + 1 {
            return invalid_live_hot_reload("steps.sequence", "step sequence must be contiguous");
        }
        validate_step(step)?;
    }
    let expected = build_readback(ledger)?;
    if expected != ledger.readback {
        return invalid_live_hot_reload("readback", "readback does not match live reload ledger");
    }
    Ok(())
}

fn validate_request(request: &LiveHotReloadRequest) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.session_id", &request.session_id)?;
    validate_single_line("request.project_id", &request.project_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_single_line("request.player_object_id", &request.player_object_id)?;
    if request.patches.is_empty() {
        return invalid_live_hot_reload("request.patches", "at least one patch is required");
    }
    if request.pre_reload_ticks == 0 {
        return invalid_live_hot_reload(
            "request.pre_reload_ticks",
            "runtime state must advance before reload",
        );
    }
    for patch in &request.patches {
        patch.validate()?;
    }
    Ok(())
}

fn validate_step(step: &LiveHotReloadStep) -> Result<(), EngineError> {
    validate_single_line("step.patch_id", &step.patch_id)?;
    if step.runtime_before.player_body_id != step.runtime_after.player_body_id
        || step.runtime_before.player_object_id != step.runtime_after.player_object_id
    {
        return invalid_live_hot_reload("step.runtime", "runtime player identity changed");
    }
    match step.class {
        LiveHotReloadChangeClass::SafeLivePatch => {
            if step.status != LiveHotReloadStatus::AppliedLive
                || step.notice_code.is_some()
                || !step.plan.needs_reload
                || step.generation_after <= step.generation_before
                || !step.player_position_preserved
                || !step.live_world_reflects_patch
            {
                return invalid_live_hot_reload(
                    "step.safe",
                    "safe live patch must apply, preserve position, and reflect in live state",
                );
            }
        }
        LiveHotReloadChangeClass::StructuralRestartRequired => {
            if step.status != LiveHotReloadStatus::DeferredWithNotice
                || step.notice_code.as_deref() != Some(LIVE_HOT_RELOAD_NOTICE_STRUCTURAL)
                || step.generation_after != step.generation_before
                || !step.player_position_preserved
                || step.live_world_reflects_patch
            {
                return invalid_live_hot_reload(
                    "step.structural",
                    "structural reload must be deferred with a restart notice",
                );
            }
        }
    }
    Ok(())
}

fn build_readback(ledger: &LiveHotReloadLedger) -> Result<LiveHotReloadReadback, EngineError> {
    let applied_live_count = ledger
        .steps
        .iter()
        .filter(|step| step.status == LiveHotReloadStatus::AppliedLive)
        .count();
    let deferred_structural_count = ledger
        .steps
        .iter()
        .filter(|step| step.status == LiveHotReloadStatus::DeferredWithNotice)
        .count();
    Ok(LiveHotReloadReadback {
        schema: LIVE_HOT_RELOAD_SCHEMA.to_string(),
        ledger_id: ledger.ledger_id.clone(),
        session_id: ledger.session_id.clone(),
        project_id: ledger.project_id.clone(),
        source_document_fingerprint: stable_fingerprint(&ledger.source_document)?,
        final_document_fingerprint: stable_fingerprint(&ledger.final_document)?,
        final_play_build_fingerprint: ledger.final_play_build_fingerprint.clone(),
        patch_count: ledger.steps.len(),
        applied_live_count,
        deferred_structural_count,
        rapid_reload_count: applied_live_count,
        player_position_preserved_all: ledger
            .steps
            .iter()
            .all(|step| step.player_position_preserved),
        safe_patches_reflected_all: ledger
            .steps
            .iter()
            .filter(|step| step.class == LiveHotReloadChangeClass::SafeLivePatch)
            .all(|step| step.live_world_reflects_patch),
        final_generation: ledger
            .steps
            .last()
            .map(|step| step.generation_after)
            .unwrap_or(0),
        final_runtime_state_hash: ledger.final_runtime.state_hash,
        readback_tool_name: ledger.readback_tool_name.clone(),
    })
}

fn empty_readback() -> LiveHotReloadReadback {
    LiveHotReloadReadback {
        schema: String::new(),
        ledger_id: String::new(),
        session_id: String::new(),
        project_id: String::new(),
        source_document_fingerprint: String::new(),
        final_document_fingerprint: String::new(),
        final_play_build_fingerprint: String::new(),
        patch_count: 0,
        applied_live_count: 0,
        deferred_structural_count: 0,
        rapid_reload_count: 0,
        player_position_preserved_all: false,
        safe_patches_reflected_all: false,
        final_generation: 0,
        final_runtime_state_hash: 0,
        readback_tool_name: String::new(),
    }
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_live_hot_reload(field, "value must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_live_hot_reload<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(live_hot_reload_error(field, reason.into()))
}

fn live_hot_reload_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.live_hot_reload.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-LIVE-HOT-RELOAD-INVALID",
        "live hot reload rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "live_hot_reload_tests.rs"]
mod tests;
