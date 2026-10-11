use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
};

#[path = "play_responsiveness_store.rs"]
mod play_responsiveness_store;

pub use play_responsiveness_store::{
    PlayResponsivenessAsterReadback, parse_play_responsiveness_ledger,
    read_play_responsiveness_ledger, write_play_responsiveness_ledger,
};

pub const PLAY_RESPONSIVENESS_SCHEMA: &str = "litd.play_responsiveness.readback.v1";
pub const PLAY_RESPONSIVENESS_LEDGER_SCHEMA: &str = "litd.play_responsiveness.ledger.v1";
pub const PLAY_RESPONSIVENESS_GUI_ROUTE: &str = "egui://editor.project/responsiveness_hud";
pub const PLAY_RESPONSIVENESS_CAPTURE_TOOL: &str = "litd.editor.play.capture_responsiveness_hud";
pub const PLAY_RESPONSIVENESS_READBACK_TOOL: &str = "litd.editor.play.read_responsiveness_hud";
pub const RESPONSE_THRESHOLD_MS: f64 = 100.0;
pub const INSTANT_FEEL_THRESHOLD_MS: f64 = 50.0;
pub const FRAME_SPIKE_THRESHOLD_MS: f64 = 50.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessRunInput {
    pub run_id: String,
    pub overlay_enabled: bool,
    pub juice_audit_enabled: bool,
    pub overlay_render_cost_ms: f64,
    pub frame_times_ms: Vec<f64>,
    pub input_events: Vec<PlayResponsivenessInputEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessInputEvent {
    pub input_id: String,
    pub action: String,
    pub input_timestamp_ms: f64,
    pub processing_start_ms: f64,
    pub processing_end_ms: f64,
    pub reaction_frame_index: usize,
    pub expected_reaction_timestamp_ms: f64,
    pub tolerance_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessMeasurement {
    pub input_id: String,
    pub action: String,
    pub input_timestamp_ms: f64,
    pub processing_start_ms: f64,
    pub processing_end_ms: f64,
    pub reaction_frame_index: usize,
    pub reaction_timestamp_ms: f64,
    pub input_delay_ms: f64,
    pub processing_duration_ms: f64,
    pub input_to_reaction_ms: f64,
    pub expected_delta_ms: f64,
    pub within_100ms: bool,
    pub feels_instant: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessFrameSpike {
    pub frame_index: usize,
    pub frame_time_ms: f64,
    pub threshold_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessRunReport {
    pub run_id: String,
    pub overlay_enabled: bool,
    pub juice_audit_enabled: bool,
    pub overlay_render_cost_ms: f64,
    pub frame_count: usize,
    pub input_count: usize,
    pub max_input_to_reaction_ms: f64,
    pub average_input_to_reaction_ms: f64,
    pub over_100ms_count: usize,
    pub instant_count: usize,
    pub frame_spikes: Vec<PlayResponsivenessFrameSpike>,
    pub measurements: Vec<PlayResponsivenessMeasurement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessOverlayComparison {
    pub disabled_run_id: String,
    pub enabled_run_id: String,
    pub max_latency_delta_ms: f64,
    pub max_overlay_render_cost_ms: f64,
    pub perturbs_timing: bool,
    pub compared_input_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessReadback {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub run_count: usize,
    pub input_count: usize,
    pub max_input_to_reaction_ms: f64,
    pub over_100ms_count: usize,
    pub frame_spike_count: usize,
    pub overlay_non_perturbing: bool,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayResponsivenessLedger {
    pub schema: String,
    pub ledger_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub gui_route: String,
    pub capture_tool_name: String,
    pub readback_tool_name: String,
    pub document_id: String,
    pub source_revision_id: String,
    pub source_document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub camera_rig: PlayCameraRig,
    pub run_inputs: Vec<PlayResponsivenessRunInput>,
    pub reports: Vec<PlayResponsivenessRunReport>,
    pub overlay_comparison: Option<PlayResponsivenessOverlayComparison>,
    pub readback: PlayResponsivenessReadback,
}

pub fn build_play_responsiveness_ledger(
    document: &EditorDocument,
    ledger_id: impl Into<String>,
    project_id: impl Into<String>,
    actor_id: impl Into<String>,
    camera_rig: PlayCameraRig,
    run_inputs: Vec<PlayResponsivenessRunInput>,
) -> Result<PlayResponsivenessLedger, EngineError> {
    document.validate()?;
    if run_inputs.is_empty() {
        return invalid_responsiveness("runs", "at least one responsiveness run is required");
    }
    let session = PlaySession::start(document, camera_rig)?;
    let play_build_fingerprint = stable_fingerprint(&session.build)?;
    let reports = run_inputs
        .iter()
        .map(build_run_report)
        .collect::<Result<Vec<_>, _>>()?;
    let overlay_comparison = compare_overlay_runs(&reports)?;
    let ledger_id = ledger_id.into();
    let project_id = project_id.into();
    let ledger = PlayResponsivenessLedger {
        schema: PLAY_RESPONSIVENESS_LEDGER_SCHEMA.to_string(),
        ledger_id,
        project_id,
        actor_id: actor_id.into(),
        gui_route: PLAY_RESPONSIVENESS_GUI_ROUTE.to_string(),
        capture_tool_name: PLAY_RESPONSIVENESS_CAPTURE_TOOL.to_string(),
        readback_tool_name: PLAY_RESPONSIVENESS_READBACK_TOOL.to_string(),
        document_id: session.build.calyx_document_id.clone(),
        source_revision_id: session.build.source_revision_id.clone(),
        source_document_fingerprint: session.build.source_fingerprint.clone(),
        play_build_fingerprint,
        camera_rig,
        run_inputs,
        reports,
        overlay_comparison,
        readback: PlayResponsivenessReadback {
            schema: String::new(),
            ledger_id: String::new(),
            project_id: String::new(),
            document_id: String::new(),
            source_revision_id: String::new(),
            source_document_fingerprint: String::new(),
            play_build_fingerprint: String::new(),
            run_count: 0,
            input_count: 0,
            max_input_to_reaction_ms: 0.0,
            over_100ms_count: 0,
            frame_spike_count: 0,
            overlay_non_perturbing: false,
            gui_route: String::new(),
            capture_tool_name: String::new(),
            readback_tool_name: String::new(),
        },
    };
    let readback = build_readback(&ledger)?;
    let ledger = PlayResponsivenessLedger { readback, ..ledger };
    validate_play_responsiveness_ledger(&ledger)?;
    Ok(ledger)
}

pub fn validate_play_responsiveness_ledger(
    ledger: &PlayResponsivenessLedger,
) -> Result<(), EngineError> {
    if ledger.schema != PLAY_RESPONSIVENESS_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.project_id.trim().is_empty()
        || ledger.actor_id.trim().is_empty()
        || ledger.gui_route != PLAY_RESPONSIVENESS_GUI_ROUTE
        || ledger.capture_tool_name != PLAY_RESPONSIVENESS_CAPTURE_TOOL
        || ledger.readback_tool_name != PLAY_RESPONSIVENESS_READBACK_TOOL
        || ledger.run_inputs.is_empty()
        || ledger.run_inputs.len() != ledger.reports.len()
    {
        return invalid_responsiveness("ledger", "required ledger fields are invalid");
    }
    validate_single_line("ledger.ledger_id", &ledger.ledger_id)?;
    validate_single_line("ledger.project_id", &ledger.project_id)?;
    validate_single_line("ledger.actor_id", &ledger.actor_id)?;
    validate_single_line("ledger.document_id", &ledger.document_id)?;
    validate_single_line("ledger.source_revision_id", &ledger.source_revision_id)?;
    validate_single_line(
        "ledger.source_document_fingerprint",
        &ledger.source_document_fingerprint,
    )?;
    validate_single_line(
        "ledger.play_build_fingerprint",
        &ledger.play_build_fingerprint,
    )?;
    let rebuilt = ledger
        .run_inputs
        .iter()
        .map(build_run_report)
        .collect::<Result<Vec<_>, _>>()?;
    if rebuilt != ledger.reports {
        return invalid_responsiveness("reports", "reports do not match run inputs");
    }
    if compare_overlay_runs(&ledger.reports)? != ledger.overlay_comparison {
        return invalid_responsiveness("overlay", "overlay comparison is stale");
    }
    let expected = build_readback(ledger)?;
    if expected != ledger.readback {
        return invalid_responsiveness("readback", "readback does not match reports");
    }
    Ok(())
}

fn build_run_report(
    input: &PlayResponsivenessRunInput,
) -> Result<PlayResponsivenessRunReport, EngineError> {
    validate_run_input(input)?;
    let mut measurements = Vec::with_capacity(input.input_events.len());
    for event in &input.input_events {
        let reaction = frame_end_timestamp_ms(&input.frame_times_ms, event.reaction_frame_index)?;
        let delta = (reaction - event.expected_reaction_timestamp_ms).abs();
        if delta > event.tolerance_ms {
            return invalid_responsiveness(
                "reaction.expected",
                format!(
                    "{} expected {:.3}ms but measured {:.3}ms",
                    event.input_id, event.expected_reaction_timestamp_ms, reaction
                ),
            );
        }
        let input_delay = event.processing_start_ms - event.input_timestamp_ms;
        let processing = event.processing_end_ms - event.processing_start_ms;
        let latency = reaction - event.input_timestamp_ms;
        if input_delay < 0.0 || processing < 0.0 || latency < 0.0 {
            return invalid_responsiveness(
                "latency",
                "derived latency values must be non-negative",
            );
        }
        measurements.push(PlayResponsivenessMeasurement {
            input_id: event.input_id.clone(),
            action: event.action.clone(),
            input_timestamp_ms: event.input_timestamp_ms,
            processing_start_ms: event.processing_start_ms,
            processing_end_ms: event.processing_end_ms,
            reaction_frame_index: event.reaction_frame_index,
            reaction_timestamp_ms: reaction,
            input_delay_ms: input_delay,
            processing_duration_ms: processing,
            input_to_reaction_ms: latency,
            expected_delta_ms: delta,
            within_100ms: latency <= RESPONSE_THRESHOLD_MS,
            feels_instant: latency <= INSTANT_FEEL_THRESHOLD_MS,
        });
    }
    let frame_spikes = input
        .frame_times_ms
        .iter()
        .enumerate()
        .filter(|(_, value)| **value > FRAME_SPIKE_THRESHOLD_MS)
        .map(
            |(frame_index, frame_time_ms)| PlayResponsivenessFrameSpike {
                frame_index,
                frame_time_ms: *frame_time_ms,
                threshold_ms: FRAME_SPIKE_THRESHOLD_MS,
            },
        )
        .collect::<Vec<_>>();
    let max_input_to_reaction_ms = measurements
        .iter()
        .map(|measurement| measurement.input_to_reaction_ms)
        .fold(0.0, f64::max);
    let average_input_to_reaction_ms = measurements
        .iter()
        .map(|measurement| measurement.input_to_reaction_ms)
        .sum::<f64>()
        / measurements.len() as f64;
    Ok(PlayResponsivenessRunReport {
        run_id: input.run_id.clone(),
        overlay_enabled: input.overlay_enabled,
        juice_audit_enabled: input.juice_audit_enabled,
        overlay_render_cost_ms: input.overlay_render_cost_ms,
        frame_count: input.frame_times_ms.len(),
        input_count: input.input_events.len(),
        max_input_to_reaction_ms,
        average_input_to_reaction_ms,
        over_100ms_count: measurements
            .iter()
            .filter(|measurement| !measurement.within_100ms)
            .count(),
        instant_count: measurements
            .iter()
            .filter(|measurement| measurement.feels_instant)
            .count(),
        frame_spikes,
        measurements,
    })
}

fn compare_overlay_runs(
    reports: &[PlayResponsivenessRunReport],
) -> Result<Option<PlayResponsivenessOverlayComparison>, EngineError> {
    let Some(disabled) = reports.iter().find(|report| !report.overlay_enabled) else {
        return Ok(None);
    };
    let Some(enabled) = reports.iter().find(|report| report.overlay_enabled) else {
        return Ok(None);
    };
    let mut compared = Vec::new();
    let mut max_delta = 0.0_f64;
    for left in &disabled.measurements {
        let right = enabled
            .measurements
            .iter()
            .find(|measurement| measurement.input_id == left.input_id)
            .ok_or_else(|| {
                responsiveness_error(
                    "overlay.input",
                    format!("missing overlay measurement {}", left.input_id),
                )
            })?;
        max_delta = max_delta.max((left.input_to_reaction_ms - right.input_to_reaction_ms).abs());
        compared.push(left.input_id.clone());
    }
    Ok(Some(PlayResponsivenessOverlayComparison {
        disabled_run_id: disabled.run_id.clone(),
        enabled_run_id: enabled.run_id.clone(),
        max_latency_delta_ms: max_delta,
        max_overlay_render_cost_ms: enabled.overlay_render_cost_ms,
        perturbs_timing: max_delta > 0.5 || enabled.overlay_render_cost_ms > 1.0,
        compared_input_ids: compared,
    }))
}

fn build_readback(
    ledger: &PlayResponsivenessLedger,
) -> Result<PlayResponsivenessReadback, EngineError> {
    let input_count = ledger.reports.iter().map(|report| report.input_count).sum();
    let max_input_to_reaction_ms = ledger
        .reports
        .iter()
        .map(|report| report.max_input_to_reaction_ms)
        .fold(0.0, f64::max);
    Ok(PlayResponsivenessReadback {
        schema: PLAY_RESPONSIVENESS_SCHEMA.to_string(),
        ledger_id: ledger.ledger_id.clone(),
        project_id: ledger.project_id.clone(),
        document_id: ledger.document_id.clone(),
        source_revision_id: ledger.source_revision_id.clone(),
        source_document_fingerprint: ledger.source_document_fingerprint.clone(),
        play_build_fingerprint: ledger.play_build_fingerprint.clone(),
        run_count: ledger.reports.len(),
        input_count,
        max_input_to_reaction_ms,
        over_100ms_count: ledger
            .reports
            .iter()
            .map(|report| report.over_100ms_count)
            .sum(),
        frame_spike_count: ledger
            .reports
            .iter()
            .map(|report| report.frame_spikes.len())
            .sum(),
        overlay_non_perturbing: ledger
            .overlay_comparison
            .as_ref()
            .map(|comparison| !comparison.perturbs_timing)
            .unwrap_or(false),
        gui_route: PLAY_RESPONSIVENESS_GUI_ROUTE.to_string(),
        capture_tool_name: PLAY_RESPONSIVENESS_CAPTURE_TOOL.to_string(),
        readback_tool_name: PLAY_RESPONSIVENESS_READBACK_TOOL.to_string(),
    })
}

fn frame_end_timestamp_ms(frame_times: &[f64], frame_index: usize) -> Result<f64, EngineError> {
    if frame_index >= frame_times.len() {
        return invalid_responsiveness(
            "reaction.frame",
            "reaction frame is outside frame timeline",
        );
    }
    Ok(frame_times.iter().take(frame_index + 1).sum())
}

fn validate_run_input(input: &PlayResponsivenessRunInput) -> Result<(), EngineError> {
    validate_single_line("run.id", &input.run_id)?;
    if input.frame_times_ms.is_empty() || input.input_events.is_empty() {
        return invalid_responsiveness("run.samples", "frames and input events are required");
    }
    validate_ms("run.overlay_render_cost_ms", input.overlay_render_cost_ms)?;
    if input.overlay_render_cost_ms > 16.0 {
        return invalid_responsiveness(
            "run.overlay_render_cost_ms",
            "overlay render cost must stay within one frame",
        );
    }
    for (index, frame_time) in input.frame_times_ms.iter().copied().enumerate() {
        validate_ms(format!("frame.{index}.time"), frame_time)?;
    }
    let mut previous = f64::NEG_INFINITY;
    for event in &input.input_events {
        validate_event(event)?;
        if event.input_timestamp_ms < previous {
            return invalid_responsiveness("input.cadence", "input timestamps must be monotonic");
        }
        previous = event.input_timestamp_ms;
        frame_end_timestamp_ms(&input.frame_times_ms, event.reaction_frame_index)?;
    }
    Ok(())
}

fn validate_event(event: &PlayResponsivenessInputEvent) -> Result<(), EngineError> {
    validate_single_line("input.id", &event.input_id)?;
    validate_single_line("input.action", &event.action)?;
    validate_ms("input.timestamp", event.input_timestamp_ms)?;
    validate_ms("input.processing_start", event.processing_start_ms)?;
    validate_ms("input.processing_end", event.processing_end_ms)?;
    validate_ms(
        "input.expected_reaction",
        event.expected_reaction_timestamp_ms,
    )?;
    if event.tolerance_ms < 0.0 || event.tolerance_ms > 5.0 {
        return invalid_responsiveness("input.tolerance", "tolerance must be within 0..=5ms");
    }
    Ok(())
}

fn validate_ms(field: impl Into<String>, value: f64) -> Result<(), EngineError> {
    if !value.is_finite() || !(0.0..=60_000.0).contains(&value) {
        return invalid_responsiveness(field, "millisecond value must be finite and bounded");
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_responsiveness(field, "value must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_responsiveness<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(responsiveness_error(field, reason))
}

fn responsiveness_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.play_responsiveness.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-PLAY-RESPONSIVENESS-VALIDATION",
        "play responsiveness capture rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "play_responsiveness_tests.rs"]
mod tests;
