//! Evidence/log/readback builders, validators and error helpers for immersive play (#1466
//! split of `immersive_play.rs`).
//!
//! The screenshot/input-log/pause/readback constructors, the display/viewport/snapshot
//! validators, input-command translation and the immersive error minting. The ledger builder,
//! render/HUD snapshot builders and public API stay in `immersive_play.rs`; these helpers are
//! `pub(super)` so it calls them unchanged.

use std::{collections::BTreeSet, fs, path::PathBuf};

use litd_hash::sha256_hex;
use winit::{event::MouseButton, keyboard::KeyCode};

use litd_engine::{
    EngineError,
    input::{ACTION_CAMERA_FORWARD, ACTION_CONFIRM, ACTION_ORBIT, InputState},
    math::Vec3,
};

use super::{
    IMMERSIVE_PLAY_SCHEMA, ImmersiveDisplayMode, ImmersiveDisplayState,
    ImmersiveDisplayToggleProof, ImmersiveEditorStateSnapshot, ImmersiveInputAction,
    ImmersiveInputCommand, ImmersiveInputEvent, ImmersiveInputLog, ImmersivePauseProof,
    ImmersivePlayLedger, ImmersivePlayReadback, ImmersiveScreenshotEvidence,
    ImmersiveScreenshotInput, ImmersiveViewport,
};
use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
};

pub(super) fn read_screenshot_evidence(
    input: &ImmersiveScreenshotInput,
) -> Result<ImmersiveScreenshotEvidence, EngineError> {
    validate_single_line("screenshot.expected_sha256", &input.expected_sha256)?;
    let path = PathBuf::from(&input.path);
    let bytes = fs::read(&path).map_err(|error| {
        immersive_error(
            "screenshot.read",
            format!("read screenshot {}: {error}", path.display()),
        )
    })?;
    let sha256 = sha256_hex(&bytes);
    if !sha256.eq_ignore_ascii_case(&input.expected_sha256) {
        return invalid_immersive(
            "screenshot.sha256",
            format!(
                "screenshot hash mismatch at {} expected {} got {sha256}",
                path.display(),
                input.expected_sha256
            ),
        );
    }
    let canonical = path.canonicalize().map_err(|error| {
        immersive_error(
            "screenshot.path",
            format!("canonicalize {}: {error}", path.display()),
        )
    })?;
    Ok(ImmersiveScreenshotEvidence {
        path: canonical.display().to_string(),
        sha256,
        byte_len: bytes.len(),
        verified: true,
    })
}

pub(super) fn build_input_log(
    commands: &[ImmersiveInputCommand],
) -> Result<ImmersiveInputLog, EngineError> {
    let mut input = InputState::with_editor_defaults();
    let mut last_timestamp = f64::NEG_INFINITY;
    let mut events = Vec::with_capacity(commands.len());
    for command in commands {
        validate_single_line("input.command_id", &command.command_id)?;
        if !command.timestamp_ms.is_finite() || command.timestamp_ms < last_timestamp {
            return invalid_immersive(
                "input.timestamp_ms",
                "input timestamps must be finite and non-decreasing",
            );
        }
        last_timestamp = command.timestamp_ms;
        apply_input_command(&mut input, command);
        let action_name = engine_action(command.action);
        let action_state = input.action_state(action_name);
        let stats = input.stats();
        events.push(ImmersiveInputEvent {
            command_id: command.command_id.clone(),
            action: command.action,
            engine_action: action_name.to_string(),
            pressed: command.pressed,
            timestamp_ms: command.timestamp_ms,
            frame_index: stats.frame_index,
            events_seen: stats.events_seen,
            active_binding_count: stats.active_binding_count,
            action_down: action_state.down,
            just_pressed: action_state.just_pressed,
            just_released: action_state.just_released,
        });
        input.finish_frame();
    }
    let input_active = events
        .iter()
        .any(|event| event.just_pressed && event.action_down);
    Ok(ImmersiveInputLog {
        events,
        input_active,
    })
}

pub(super) fn build_pause_proof(session: &PlaySession) -> Result<ImmersivePauseProof, EngineError> {
    let snapshot = stable_fingerprint(&session.build.snapshot)?;
    Ok(ImmersivePauseProof {
        paused: true,
        simulation_fingerprint_before_pause: snapshot.clone(),
        simulation_fingerprint_after_pause: snapshot,
        stable_while_paused: true,
    })
}

pub(super) fn build_readback(ledger: &ImmersivePlayLedger) -> ImmersivePlayReadback {
    ImmersivePlayReadback {
        schema: IMMERSIVE_PLAY_SCHEMA.to_string(),
        ledger_id: ledger.ledger_id.clone(),
        project_id: ledger.project_id.clone(),
        document_id: ledger.document_id.clone(),
        source_revision_id: ledger.source_revision_id.clone(),
        source_document_fingerprint: ledger.source_document_fingerprint.clone(),
        play_build_fingerprint: ledger.render.play_build_fingerprint.clone(),
        camera_rig: ledger.render.camera_rig,
        entry_world_position: ledger.entry.world_position,
        render_active: ledger.render.full_fidelity_render_active,
        visible_instances: ledger.render.visible_instances,
        draw_batches: ledger.render.draw_batches,
        hud_active: ledger.hud.hud_active,
        hud_regions: ledger.hud.region_count,
        screenshot_verified: ledger.screenshot.verified,
        screenshot_sha256: ledger.screenshot.sha256.clone(),
        input_active: ledger.input_log.input_active,
        input_event_count: ledger.input_log.events.len(),
        pause_verified: ledger.pause.paused
            && ledger.pause.stable_while_paused
            && ledger.pause.simulation_fingerprint_before_pause
                == ledger.pause.simulation_fingerprint_after_pause,
        fullscreen_toggle_stable: ledger.display_toggle.stable,
        editor_restored_exact: ledger.exit.restored_exact,
        gui_route: ledger.gui_route.clone(),
        capture_tool_name: ledger.capture_tool_name.clone(),
        readback_tool_name: ledger.readback_tool_name.clone(),
    }
}

pub(super) fn empty_readback() -> ImmersivePlayReadback {
    ImmersivePlayReadback {
        schema: String::new(),
        ledger_id: String::new(),
        project_id: String::new(),
        document_id: String::new(),
        source_revision_id: String::new(),
        source_document_fingerprint: String::new(),
        play_build_fingerprint: String::new(),
        camera_rig: PlayCameraRig::FirstPerson,
        entry_world_position: Vec3::ZERO,
        render_active: false,
        visible_instances: 0,
        draw_batches: 0,
        hud_active: false,
        hud_regions: 0,
        screenshot_verified: false,
        screenshot_sha256: String::new(),
        input_active: false,
        input_event_count: 0,
        pause_verified: false,
        fullscreen_toggle_stable: false,
        editor_restored_exact: false,
        gui_route: String::new(),
        capture_tool_name: String::new(),
        readback_tool_name: String::new(),
    }
}

pub(super) fn validate_editor_snapshot_matches_document(
    field: &str,
    snapshot: &ImmersiveEditorStateSnapshot,
    document: &EditorDocument,
    fingerprint: &str,
) -> Result<(), EngineError> {
    if snapshot.document_id != document.calyx_document_id
        || snapshot.revision_id != document.revision_id
        || snapshot.document_fingerprint != fingerprint
    {
        return invalid_immersive(
            field,
            "editor snapshot must match the source document source of truth",
        );
    }
    validate_single_line(&format!("{field}.active_panel"), &snapshot.active_panel)?;
    validate_single_line(&format!("{field}.viewport_mode"), &snapshot.viewport_mode)?;
    let mut seen = BTreeSet::new();
    for id in &snapshot.selected_object_ids {
        validate_single_line(&format!("{field}.selected_object_ids"), id)?;
        if !document.objects.contains_key(id) {
            return invalid_immersive(field, format!("selected object {id} is missing"));
        }
        if !seen.insert(id) {
            return invalid_immersive(field, "selected object ids must be unique");
        }
    }
    Ok(())
}

pub(super) fn validate_display_toggle(
    toggle: &ImmersiveDisplayToggleProof,
) -> Result<(), EngineError> {
    validate_display_state("display.before", &toggle.before)?;
    validate_display_state("display.entered_fullscreen", &toggle.entered_fullscreen)?;
    validate_display_state("display.restored", &toggle.restored)?;
    if toggle.entered_fullscreen.mode != ImmersiveDisplayMode::Fullscreen {
        return invalid_immersive(
            "display.entered_fullscreen",
            "immersive toggle must enter fullscreen",
        );
    }
    let restored = toggle.before == toggle.restored;
    if !toggle.stable || !restored {
        return invalid_immersive(
            "display.stable",
            "fullscreen toggle must restore the original display state",
        );
    }
    Ok(())
}

pub(super) fn validate_display_state(
    field: &str,
    state: &ImmersiveDisplayState,
) -> Result<(), EngineError> {
    if state.monitor_count == 0
        || state.monitor_index >= state.monitor_count
        || state.width_px == 0
        || state.height_px == 0
        || state.scale_factor_milli == 0
    {
        return invalid_immersive(
            field,
            "display state dimensions and monitor index are invalid",
        );
    }
    Ok(())
}

pub(super) fn validate_viewport(viewport: &ImmersiveViewport) -> Result<(), EngineError> {
    if viewport.width_px < 640 || viewport.height_px < 360 {
        return invalid_immersive("viewport", "immersive viewport must be at least 640x360");
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_immersive(field, "value must be non-empty and single-line");
    }
    Ok(())
}

pub(super) fn apply_input_command(input: &mut InputState, command: &ImmersiveInputCommand) {
    match command.action {
        ImmersiveInputAction::MoveForward => input.set_key(KeyCode::KeyW, command.pressed),
        ImmersiveInputAction::Confirm => {
            input.set_mouse_button(MouseButton::Left, command.pressed);
        }
        ImmersiveInputAction::Orbit => {
            input.set_mouse_button(MouseButton::Right, command.pressed);
        }
    }
}

pub(super) fn engine_action(action: ImmersiveInputAction) -> &'static str {
    match action {
        ImmersiveInputAction::MoveForward => ACTION_CAMERA_FORWARD,
        ImmersiveInputAction::Confirm => ACTION_CONFIRM,
        ImmersiveInputAction::Orbit => ACTION_ORBIT,
    }
}

pub(super) fn invalid_immersive<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(immersive_error(field, reason.into()))
}

pub(super) fn immersive_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.immersive_play.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-IMMERSIVE-PLAY-INVALID",
        "immersive play rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
