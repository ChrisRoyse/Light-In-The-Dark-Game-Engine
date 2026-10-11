use litd_engine::{
    EngineError,
    math::Vec3,
    render::ui::{
        HudLayoutPreset, HudReferenceLayout, HudRegionKind, UiRenderSettings, reference_hud_layout,
    },
};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
};

#[path = "immersive_play_store.rs"]
mod immersive_play_store;
#[path = "immersive_play_support.rs"]
mod immersive_play_support;
#[path = "immersive_play_types.rs"]
mod immersive_play_types;

pub use immersive_play_store::{
    ImmersivePlayAsterReadback, parse_immersive_play_ledger, read_immersive_play_ledger,
    write_immersive_play_ledger,
};
use immersive_play_support::{
    build_input_log, build_pause_proof, build_readback, empty_readback, immersive_error,
    invalid_immersive, read_screenshot_evidence, validate_display_toggle,
    validate_editor_snapshot_matches_document, validate_single_line, validate_viewport,
};
pub use immersive_play_types::*;

pub const IMMERSIVE_PLAY_SCHEMA: &str = "litd.immersive_play.readback.v1";
pub const IMMERSIVE_PLAY_LEDGER_SCHEMA: &str = "litd.immersive_play.ledger.v1";
pub const IMMERSIVE_PLAY_GUI_ROUTE: &str = "egui://editor.project/immersive_play";
pub const IMMERSIVE_PLAY_CAPTURE_TOOL: &str = "litd.editor.play.enter_immersive_from_here";
pub const IMMERSIVE_PLAY_READBACK_TOOL: &str = "litd.editor.play.read_immersive_play";

pub fn build_immersive_play_ledger(
    document: &EditorDocument,
    request: ImmersivePlayRequest,
) -> Result<ImmersivePlayLedger, EngineError> {
    document.validate()?;
    validate_request(&request)?;
    let source_document_fingerprint = stable_fingerprint(document)?;
    validate_editor_snapshot_matches_document(
        "editor_before",
        &request.editor_before,
        document,
        &source_document_fingerprint,
    )?;
    validate_editor_snapshot_matches_document(
        "editor_after_exit",
        &request.editor_after_exit,
        document,
        &source_document_fingerprint,
    )?;
    let entry = resolve_entry(document, &request)?;
    let play_document = play_document_for_entry(document, &request, entry.world_position)?;
    let session = PlaySession::start_with_camera_config(
        &play_document,
        crate::play::PlayCameraConfig::preset(request.camera_rig)
            .for_player(request.player_object_id.clone()),
    )?;
    let render = build_render_snapshot(&session, entry.world_position)?;
    let hud = build_hud_snapshot(request.viewport)?;
    let screenshot = read_screenshot_evidence(&request.screenshot)?;
    let input_log = build_input_log(&request.input_commands)?;
    let pause = build_pause_proof(&session)?;
    let exit = ImmersiveExitProof {
        restored_exact: request.editor_before == request.editor_after_exit,
        before: request.editor_before,
        after: request.editor_after_exit,
    };
    validate_display_toggle(&request.display_toggle)?;
    let ledger = ImmersivePlayLedger {
        schema: IMMERSIVE_PLAY_LEDGER_SCHEMA.to_string(),
        ledger_id: request.ledger_id,
        project_id: request.project_id,
        actor_id: request.actor_id,
        gui_route: IMMERSIVE_PLAY_GUI_ROUTE.to_string(),
        capture_tool_name: IMMERSIVE_PLAY_CAPTURE_TOOL.to_string(),
        readback_tool_name: IMMERSIVE_PLAY_READBACK_TOOL.to_string(),
        document_id: document.calyx_document_id.clone(),
        source_revision_id: document.revision_id.clone(),
        source_document_fingerprint,
        entry,
        render,
        hud,
        screenshot,
        input_log,
        pause,
        display_toggle: request.display_toggle,
        exit,
        readback: empty_readback(),
    };
    let ledger = ImmersivePlayLedger {
        readback: build_readback(&ledger),
        ..ledger
    };
    validate_immersive_play_ledger(&ledger)?;
    tracing::info!(
        ledger_id = %ledger.ledger_id,
        document_id = %ledger.document_id,
        camera_rig = ?ledger.render.camera_rig,
        visible_instances = ledger.render.visible_instances,
        input_events = ledger.input_log.events.len(),
        code = "LITD-IMMERSIVE-PLAY-ENTERED",
        "immersive play-from-here ledger built"
    );
    Ok(ledger)
}

pub fn validate_immersive_play_ledger(ledger: &ImmersivePlayLedger) -> Result<(), EngineError> {
    if ledger.schema != IMMERSIVE_PLAY_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.project_id.trim().is_empty()
        || ledger.actor_id.trim().is_empty()
        || ledger.gui_route != IMMERSIVE_PLAY_GUI_ROUTE
        || ledger.capture_tool_name != IMMERSIVE_PLAY_CAPTURE_TOOL
        || ledger.readback_tool_name != IMMERSIVE_PLAY_READBACK_TOOL
    {
        return invalid_immersive("ledger", "required immersive ledger fields are invalid");
    }
    for (field, value) in [
        ("ledger_id", ledger.ledger_id.as_str()),
        ("project_id", ledger.project_id.as_str()),
        ("actor_id", ledger.actor_id.as_str()),
        ("document_id", ledger.document_id.as_str()),
        ("source_revision_id", ledger.source_revision_id.as_str()),
    ] {
        validate_single_line(field, value)?;
    }
    if !ledger.render.full_fidelity_render_active || !ledger.render.embodied_camera {
        return invalid_immersive(
            "render",
            "immersive mode requires an embodied camera and active render batches",
        );
    }
    if !ledger.hud.hud_active {
        return invalid_immersive("hud", "immersive mode requires gameplay HUD regions");
    }
    if !ledger.screenshot.verified || ledger.screenshot.byte_len == 0 {
        return invalid_immersive("screenshot", "screenshot evidence must be verified");
    }
    if !ledger.input_log.input_active || ledger.input_log.events.is_empty() {
        return invalid_immersive("input", "input log must contain active input events");
    }
    if !ledger.pause.paused
        || !ledger.pause.stable_while_paused
        || ledger.pause.simulation_fingerprint_before_pause
            != ledger.pause.simulation_fingerprint_after_pause
    {
        return invalid_immersive("pause", "pause proof must freeze the simulation snapshot");
    }
    validate_display_toggle(&ledger.display_toggle)?;
    if !ledger.exit.restored_exact {
        return invalid_immersive("exit", "exit must restore the exact editor state snapshot");
    }
    let expected = build_readback(ledger);
    if expected != ledger.readback {
        return invalid_immersive("readback", "readback does not match immersive ledger");
    }
    Ok(())
}

fn validate_request(request: &ImmersivePlayRequest) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.project_id", &request.project_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_single_line("request.player_object_id", &request.player_object_id)?;
    if !matches!(
        request.camera_rig,
        PlayCameraRig::FirstPerson | PlayCameraRig::OverShoulder
    ) {
        return invalid_immersive(
            "request.camera_rig",
            "immersive play requires first-person or over-shoulder camera",
        );
    }
    if request.input_commands.is_empty() {
        return invalid_immersive("request.input", "input command log cannot be empty");
    }
    validate_viewport(&request.viewport)?;
    Ok(())
}

fn resolve_entry(
    document: &EditorDocument,
    request: &ImmersivePlayRequest,
) -> Result<ImmersiveEntryResolved, EngineError> {
    let world_position = match request.entry.kind {
        ImmersiveEntryKind::Cursor => request.entry.cursor_world.ok_or_else(|| {
            immersive_error(
                "entry.cursor_world",
                "cursor entry requires a world position",
            )
        })?,
        ImmersiveEntryKind::ChosenSpawn => {
            let id = request.entry.spawn_object_id.as_deref().ok_or_else(|| {
                immersive_error(
                    "entry.spawn_object_id",
                    "chosen spawn requires an object id",
                )
            })?;
            document
                .objects
                .get(id)
                .ok_or_else(|| immersive_error("entry.spawn", format!("missing spawn {id}")))?
                .transform
                .translation
        }
    };
    if !world_position.is_finite() {
        return invalid_immersive("entry.world_position", "entry position must be finite");
    }
    Ok(ImmersiveEntryResolved {
        kind: request.entry.kind,
        player_object_id: request.player_object_id.clone(),
        source_object_id: request.entry.spawn_object_id.clone(),
        world_position,
    })
}

fn play_document_for_entry(
    document: &EditorDocument,
    request: &ImmersivePlayRequest,
    world_position: Vec3,
) -> Result<EditorDocument, EngineError> {
    let mut play_document = document.clone();
    let player = play_document
        .objects
        .get_mut(&request.player_object_id)
        .ok_or_else(|| {
            immersive_error(
                "player_object_id",
                format!("missing player object {}", request.player_object_id),
            )
        })?;
    if player.kind != "Unit" {
        return invalid_immersive("player_object_id", "player object must be a Unit");
    }
    player.transform.translation = world_position;
    play_document.validate()?;
    Ok(play_document)
}

fn build_render_snapshot(
    session: &PlaySession,
    entry_world_position: Vec3,
) -> Result<ImmersiveRenderSnapshot, EngineError> {
    let target = session.build.camera_pose.target_world;
    let expected = entry_world_position + session.build.camera_config.target_offset;
    if (target - expected).length() > 0.001 {
        return invalid_immersive(
            "camera.target",
            "camera target does not match resolved immersive entry",
        );
    }
    let embodied_camera = matches!(
        session.build.camera_rig,
        PlayCameraRig::FirstPerson | PlayCameraRig::OverShoulder
    );
    let visible_instances = session.build.visible_instance_ids.len();
    let presentation_instances = session.build.presentation.instances.len();
    Ok(ImmersiveRenderSnapshot {
        play_session_state: session.state,
        play_session_fingerprint: stable_fingerprint(session)?,
        play_build_fingerprint: stable_fingerprint(&session.build)?,
        camera_rig: session.build.camera_rig,
        camera_player_id: session.build.camera_config.player_id.clone(),
        camera_target_world: target,
        visible_instances,
        draw_batches: session.build.draw_batches,
        presentation_instances,
        spawned_bodies: session.build.spawned_bodies.len(),
        embodied_camera,
        full_fidelity_render_active: embodied_camera
            && visible_instances > 0
            && session.build.draw_batches > 0
            && presentation_instances > 0,
    })
}

fn build_hud_snapshot(viewport: ImmersiveViewport) -> Result<ImmersiveHudSnapshot, EngineError> {
    validate_viewport(&viewport)?;
    let layout = reference_hud_layout(
        UiRenderSettings {
            viewport_width: viewport.width_px as f32,
            viewport_height: viewport.height_px as f32,
        },
        HudLayoutPreset::PlayTopDown,
    )?;
    Ok(hud_snapshot(viewport, layout))
}

fn hud_snapshot(viewport: ImmersiveViewport, layout: HudReferenceLayout) -> ImmersiveHudSnapshot {
    let minimap_present = layout.region_rect(HudRegionKind::Minimap).is_some();
    let command_bar_present = layout.region_rect(HudRegionKind::CommandBar).is_some();
    let status_strip_present = layout.region_rect(HudRegionKind::StatusStrip).is_some();
    let hud_active = minimap_present
        && command_bar_present
        && status_strip_present
        && layout.command_slot_count > 0;
    ImmersiveHudSnapshot {
        preset: layout.preset,
        viewport_width_px: viewport.width_px,
        viewport_height_px: viewport.height_px,
        region_count: layout.regions.len(),
        command_slot_count: layout.command_slot_count,
        minimap_present,
        command_bar_present,
        status_strip_present,
        reference_images: layout.reference_images,
        hud_active,
    }
}

#[cfg(test)]
#[path = "immersive_play_tests.rs"]
mod tests;
