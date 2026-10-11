use std::time::Instant;

use litd_engine::{EngineError, math::Vec3};
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraConfig, PlayCameraRig, PlaySession},
};

#[path = "instant_play_store.rs"]
mod instant_play_store;

pub use instant_play_store::{
    InstantPlayAsterReadback, parse_instant_play_ledger, read_instant_play_ledger,
    write_instant_play_ledger,
};

pub const INSTANT_PLAY_SCHEMA: &str = "litd.instant_play.readback.v1";
pub const INSTANT_PLAY_LEDGER_SCHEMA: &str = "litd.instant_play.ledger.v1";
pub const INSTANT_PLAY_GUI_ROUTE: &str = "egui://editor.project/instant_play";
pub const INSTANT_PLAY_CAPTURE_TOOL: &str = "litd.editor.play.enter_world_instant";
pub const INSTANT_PLAY_READBACK_TOOL: &str = "litd.editor.play.read_instant_play";
pub const DEFAULT_INSTANT_PLAY_BUDGET_MS: f64 = 1000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstantPlayRequest {
    pub ledger_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub player_object_id: String,
    pub camera_rig: PlayCameraRig,
    pub cursor_world: Vec3,
    pub latency_budget_ms: f64,
    pub repeat_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstantPlayRun {
    pub run_id: String,
    pub sequence: usize,
    pub elapsed_ms: f64,
    pub latency_budget_ms: f64,
    pub within_budget: bool,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub player_transform: Vec3,
    pub camera_target_world: Vec3,
    pub visible_instances: usize,
    pub draw_batches: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstantPlayReadback {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub source_document_fingerprint: String,
    pub cursor_world: Vec3,
    pub run_count: usize,
    pub max_elapsed_ms: f64,
    pub latency_budget_ms: f64,
    pub all_runs_within_budget: bool,
    pub spawn_transform_matches_cursor: bool,
    pub repeated_enter_exit_stable: bool,
    pub visible_instances: usize,
    pub draw_batches: usize,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstantPlayLedger {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
    pub player_object_id: String,
    pub camera_rig: PlayCameraRig,
    pub source_document: EditorDocument,
    pub cursor_world: Vec3,
    pub runs: Vec<InstantPlayRun>,
    pub readback: InstantPlayReadback,
}

pub fn build_instant_play_ledger(
    document: &EditorDocument,
    request: InstantPlayRequest,
) -> Result<InstantPlayLedger, EngineError> {
    validate_request(&request)?;
    document.validate()?;
    let mut runs = Vec::with_capacity(request.repeat_count);
    for sequence in 1..=request.repeat_count {
        let run = run_instant_play(document, &request, sequence)?;
        if !run.within_budget {
            return invalid_instant_play(
                "latency",
                format!(
                    "{} exceeded {:.3}ms budget with {:.3}ms",
                    run.run_id, run.latency_budget_ms, run.elapsed_ms
                ),
            );
        }
        runs.push(run);
    }
    let ledger = InstantPlayLedger {
        schema: INSTANT_PLAY_LEDGER_SCHEMA.to_string(),
        ledger_id: request.ledger_id,
        project_id: request.project_id,
        actor_id: request.actor_id,
        gui_route: INSTANT_PLAY_GUI_ROUTE.to_string(),
        capture_tool_name: INSTANT_PLAY_CAPTURE_TOOL.to_string(),
        readback_tool_name: INSTANT_PLAY_READBACK_TOOL.to_string(),
        player_object_id: request.player_object_id,
        camera_rig: request.camera_rig,
        source_document: document.clone(),
        cursor_world: request.cursor_world,
        runs,
        readback: empty_readback(),
    };
    let ledger = InstantPlayLedger {
        readback: build_readback(&ledger)?,
        ..ledger
    };
    validate_instant_play_ledger(&ledger)?;
    tracing::info!(
        ledger_id = %ledger.ledger_id,
        max_elapsed_ms = ledger.readback.max_elapsed_ms,
        run_count = ledger.readback.run_count,
        code = "LITD-INSTANT-PLAY-BUILT",
        "instant play ledger built"
    );
    Ok(ledger)
}

pub fn validate_instant_play_ledger(ledger: &InstantPlayLedger) -> Result<(), EngineError> {
    if ledger.schema != INSTANT_PLAY_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.project_id.trim().is_empty()
        || ledger.actor_id.trim().is_empty()
        || ledger.gui_route != INSTANT_PLAY_GUI_ROUTE
        || ledger.capture_tool_name != INSTANT_PLAY_CAPTURE_TOOL
        || ledger.readback_tool_name != INSTANT_PLAY_READBACK_TOOL
        || ledger.runs.is_empty()
    {
        return invalid_instant_play("ledger", "required instant-play fields are invalid");
    }
    for (field, value) in [
        ("ledger_id", ledger.ledger_id.as_str()),
        ("project_id", ledger.project_id.as_str()),
        ("actor_id", ledger.actor_id.as_str()),
        ("player_object_id", ledger.player_object_id.as_str()),
    ] {
        validate_single_line(field, value)?;
    }
    ledger.source_document.validate()?;
    for (index, run) in ledger.runs.iter().enumerate() {
        validate_run(run, index + 1, ledger)?;
    }
    let expected = build_readback(ledger)?;
    if expected != ledger.readback {
        return invalid_instant_play("readback", "readback does not match instant-play ledger");
    }
    if !ledger.readback.all_runs_within_budget
        || !ledger.readback.spawn_transform_matches_cursor
        || !ledger.readback.repeated_enter_exit_stable
    {
        return invalid_instant_play(
            "readback.invariants",
            "instant play requires budget, cursor spawn, and repeated stability",
        );
    }
    Ok(())
}

fn run_instant_play(
    document: &EditorDocument,
    request: &InstantPlayRequest,
    sequence: usize,
) -> Result<InstantPlayRun, EngineError> {
    let mut play_document = document.clone();
    let Some(player) = play_document.objects.get_mut(&request.player_object_id) else {
        return invalid_instant_play(
            "player_object_id",
            format!("missing player object {}", request.player_object_id),
        );
    };
    if player.kind != "Unit" {
        return invalid_instant_play("player_object_id", "player object must be a Unit");
    }
    player.transform.translation = request.cursor_world;
    play_document.validate()?;
    let start = Instant::now();
    let session = PlaySession::start_with_camera_config(
        &play_document,
        PlayCameraConfig::preset(request.camera_rig).for_player(request.player_object_id.clone()),
    )?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let player_transform = session
        .source_document
        .objects
        .get(&request.player_object_id)
        .map(|object| object.transform.translation)
        .ok_or_else(|| instant_play_error("player.readback", "player missing after play start"))?;
    Ok(InstantPlayRun {
        run_id: format!("{}-run-{sequence}", request.ledger_id),
        sequence,
        elapsed_ms,
        latency_budget_ms: request.latency_budget_ms,
        within_budget: elapsed_ms <= request.latency_budget_ms,
        document_fingerprint: stable_fingerprint(&session.source_document)?,
        play_build_fingerprint: stable_fingerprint(&session.build)?,
        player_transform,
        camera_target_world: session.build.camera_pose.target_world,
        visible_instances: session.build.visible_instance_ids.len(),
        draw_batches: session.build.draw_batches,
    })
}

fn build_readback(ledger: &InstantPlayLedger) -> Result<InstantPlayReadback, EngineError> {
    let max_elapsed_ms = ledger
        .runs
        .iter()
        .map(|run| run.elapsed_ms)
        .fold(0.0_f64, f64::max);
    let first = ledger
        .runs
        .first()
        .ok_or_else(|| instant_play_error("runs", "missing instant play run"))?;
    Ok(InstantPlayReadback {
        schema: INSTANT_PLAY_SCHEMA.to_string(),
        ledger_id: ledger.ledger_id.clone(),
        project_id: ledger.project_id.clone(),
        source_document_fingerprint: stable_fingerprint(&ledger.source_document)?,
        cursor_world: ledger.cursor_world,
        run_count: ledger.runs.len(),
        max_elapsed_ms,
        latency_budget_ms: first.latency_budget_ms,
        all_runs_within_budget: ledger.runs.iter().all(|run| run.within_budget),
        spawn_transform_matches_cursor: ledger
            .runs
            .iter()
            .all(|run| run.player_transform == ledger.cursor_world),
        repeated_enter_exit_stable: ledger.runs.iter().all(|run| {
            run.document_fingerprint == first.document_fingerprint
                && run.play_build_fingerprint == first.play_build_fingerprint
                && run.player_transform == first.player_transform
                && run.visible_instances == first.visible_instances
                && run.draw_batches == first.draw_batches
        }),
        visible_instances: first.visible_instances,
        draw_batches: first.draw_batches,
        readback_tool_name: ledger.readback_tool_name.clone(),
    })
}

fn validate_request(request: &InstantPlayRequest) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.project_id", &request.project_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_single_line("request.player_object_id", &request.player_object_id)?;
    if !request.cursor_world.is_finite() {
        return invalid_instant_play("request.cursor_world", "cursor world must be finite");
    }
    if !request.latency_budget_ms.is_finite()
        || request.latency_budget_ms <= 0.0
        || request.latency_budget_ms > DEFAULT_INSTANT_PLAY_BUDGET_MS
    {
        return invalid_instant_play(
            "request.latency_budget_ms",
            "latency budget must be within 0..=1000ms",
        );
    }
    if request.repeat_count == 0 || request.repeat_count > 32 {
        return invalid_instant_play("request.repeat_count", "repeat count must be 1..=32");
    }
    Ok(())
}

fn validate_run(
    run: &InstantPlayRun,
    expected_sequence: usize,
    ledger: &InstantPlayLedger,
) -> Result<(), EngineError> {
    validate_single_line("run.run_id", &run.run_id)?;
    if run.sequence != expected_sequence
        || !run.elapsed_ms.is_finite()
        || run.elapsed_ms < 0.0
        || !run.latency_budget_ms.is_finite()
        || run.latency_budget_ms <= 0.0
        || run.within_budget != (run.elapsed_ms <= run.latency_budget_ms)
        || run.player_transform != ledger.cursor_world
        || run.visible_instances == 0
        || run.draw_batches == 0
    {
        return invalid_instant_play("run", "instant play run readback is invalid");
    }
    Ok(())
}

fn empty_readback() -> InstantPlayReadback {
    InstantPlayReadback {
        schema: String::new(),
        ledger_id: String::new(),
        project_id: String::new(),
        source_document_fingerprint: String::new(),
        cursor_world: Vec3::ZERO,
        run_count: 0,
        max_elapsed_ms: 0.0,
        latency_budget_ms: 0.0,
        all_runs_within_budget: false,
        spawn_transform_matches_cursor: false,
        repeated_enter_exit_stable: false,
        visible_instances: 0,
        draw_batches: 0,
        readback_tool_name: String::new(),
    }
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_instant_play(field, "value must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_instant_play<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(instant_play_error(field, reason.into()))
}

fn instant_play_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.instant_play.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-INSTANT-PLAY-INVALID",
        "instant play rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "instant_play_tests.rs"]
mod tests;
