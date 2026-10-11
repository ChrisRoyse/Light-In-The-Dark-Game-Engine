use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::history::stable_fingerprint;

use super::{
    EditorIntelligenceDock, EditorIntelligenceMetric, EditorIntelligenceObservation,
    EditorIntelligenceSurfaceKind, EditorIntelligenceSurfacePanel,
};

pub(super) fn normalize_observations(
    observations: Vec<EditorIntelligenceObservation>,
    advertised: &BTreeSet<&str>,
) -> Result<Vec<EditorIntelligenceObservation>, EngineError> {
    let mut by_tool = BTreeMap::new();
    for observation in observations {
        validate_observation(&observation)?;
        if !advertised.contains(observation.tool_name.as_str()) {
            return invalid(
                "observation.tool_name",
                format!(
                    "{} is not advertised by the structured manifest",
                    observation.tool_name
                ),
            );
        }
        if by_tool
            .insert(observation.tool_name.clone(), observation)
            .is_some()
        {
            return invalid("observation.tool_name", "duplicate tool observation");
        }
    }
    Ok(by_tool.into_values().collect())
}

pub(super) fn validate_observation(
    observation: &EditorIntelligenceObservation,
) -> Result<(), EngineError> {
    if observation.tool_name.trim().is_empty()
        || observation.source_ref.trim().is_empty()
        || !observation.payload.is_object()
    {
        return invalid(
            "observation",
            "tool name, source ref, and object payload are required",
        );
    }
    Ok(())
}

pub(super) fn build_panel(
    spec: SurfaceSpec,
    by_tool: &BTreeMap<&str, &EditorIntelligenceObservation>,
) -> Result<EditorIntelligenceSurfacePanel, EngineError> {
    let observations = spec
        .tools
        .iter()
        .map(|tool| {
            by_tool
                .get(tool)
                .copied()
                .ok_or_else(|| error("source_tool", format!("missing required tool {tool}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for observation in &observations {
        validate_tool_payload(observation.tool_name.as_str(), &observation.payload)?;
    }
    Ok(EditorIntelligenceSurfacePanel {
        kind: spec.kind,
        panel_id: spec.panel_id.to_string(),
        title: spec.title.to_string(),
        dock: spec.dock,
        order: spec.order,
        min_width_px: spec.min_width_px,
        min_height_px: spec.min_height_px,
        source_tools: spec.tools.iter().map(|tool| (*tool).to_string()).collect(),
        readback_refs: observations
            .iter()
            .map(|observation| observation.source_ref.clone())
            .collect(),
        metrics: metrics_for(spec.kind, by_tool)?,
        focus_cx_ids: focus_cx_ids(spec.kind, by_tool)?,
        warnings: warnings_for(spec.kind, by_tool)?,
        payload_fingerprint: stable_fingerprint(&observations)?,
    })
}

fn validate_tool_payload(tool: &str, payload: &Value) -> Result<(), EngineError> {
    match tool {
        "calyx.abundance" => {
            required_u64(payload, tool, "/n")?;
            required_u64(payload, tool, "/materialized")?;
            required_u64(payload, tool, "/pairs")?;
            required_u64(payload, tool, "/panel_size")?;
        }
        "calyx.bits" => {
            required_str(payload, tool, "/anchor")?;
            required_f64(payload, tool, "/panel_sufficiency")?;
            non_empty_array(payload, tool, "/per_slot")?;
        }
        "calyx.neighbors" => {
            let neighbors = non_empty_array(payload, tool, "/neighbors")?;
            required_str(&neighbors[0], tool, "/cx_id")?;
            required_f64(&neighbors[0], tool, "/score")?;
        }
        "calyx.kernel" => {
            required_u64(payload, tool, "/kernel_size")?;
            required_f64(payload, tool, "/recall")?;
            non_empty_array(payload, tool, "/kernel_cx_ids")?;
            required_array(payload, tool, "/grounding_gaps")?;
        }
        "calyx.kernel_answer" => {
            required_str(payload, tool, "/answer")?;
            required_f64(payload, tool, "/recall")?;
            non_empty_array(payload, tool, "/kernel_cx_ids")?;
            required_array(payload, tool, "/gaps")?;
        }
        "calyx.guard.check" => {
            required_str(payload, tool, "/verdict")?;
            required_f64(payload, tool, "/distance")?;
            required_f64(payload, tool, "/tau")?;
        }
        _ => return invalid("payload.tool", format!("{tool} is not a deck source tool")),
    }
    Ok(())
}

fn metrics_for(
    kind: EditorIntelligenceSurfaceKind,
    by_tool: &BTreeMap<&str, &EditorIntelligenceObservation>,
) -> Result<Vec<EditorIntelligenceMetric>, EngineError> {
    let metrics = match kind {
        EditorIntelligenceSurfaceKind::Abundance => {
            let payload = payload(by_tool, "calyx.abundance")?;
            vec![
                metric(
                    "constellations",
                    json!(required_u64(payload, "calyx.abundance", "/n")?),
                ),
                metric(
                    "materialized",
                    json!(required_u64(payload, "calyx.abundance", "/materialized")?),
                ),
                metric(
                    "pairs",
                    json!(required_u64(payload, "calyx.abundance", "/pairs")?),
                ),
                metric(
                    "panel_size",
                    json!(required_u64(payload, "calyx.abundance", "/panel_size")?),
                ),
            ]
        }
        EditorIntelligenceSurfaceKind::Bits => {
            let payload = payload(by_tool, "calyx.bits")?;
            let slots = non_empty_array(payload, "calyx.bits", "/per_slot")?;
            let max_bits = slots
                .iter()
                .filter_map(|slot| slot.get("bits").and_then(Value::as_f64))
                .fold(0.0_f64, f64::max);
            vec![
                metric(
                    "anchor",
                    json!(required_str(payload, "calyx.bits", "/anchor")?),
                ),
                metric(
                    "panel_sufficiency",
                    json!(required_f64(payload, "calyx.bits", "/panel_sufficiency")?),
                ),
                metric("active_slots", json!(slots.len())),
                metric("strongest_bits", json!(max_bits)),
            ]
        }
        EditorIntelligenceSurfaceKind::AssociationGraph => {
            let payload = payload(by_tool, "calyx.neighbors")?;
            let neighbors = non_empty_array(payload, "calyx.neighbors", "/neighbors")?;
            vec![
                metric("neighbor_count", json!(neighbors.len())),
                metric(
                    "top_cx_id",
                    json!(required_str(&neighbors[0], "calyx.neighbors", "/cx_id")?),
                ),
                metric(
                    "top_score",
                    json!(required_f64(&neighbors[0], "calyx.neighbors", "/score")?),
                ),
            ]
        }
        EditorIntelligenceSurfaceKind::Kernel => {
            let payload = payload(by_tool, "calyx.kernel")?;
            let gaps = required_array(payload, "calyx.kernel", "/grounding_gaps")?;
            vec![
                metric(
                    "kernel_size",
                    json!(required_u64(payload, "calyx.kernel", "/kernel_size")?),
                ),
                metric(
                    "recall",
                    json!(required_f64(payload, "calyx.kernel", "/recall")?),
                ),
                metric("grounding_gap_count", json!(gaps.len())),
            ]
        }
        EditorIntelligenceSurfaceKind::WhatIf => {
            let answer = payload(by_tool, "calyx.kernel_answer")?;
            let guard = payload(by_tool, "calyx.guard.check")?;
            vec![
                metric(
                    "answer_chars",
                    json!(required_str(answer, "calyx.kernel_answer", "/answer")?.len()),
                ),
                metric(
                    "answer_recall",
                    json!(required_f64(answer, "calyx.kernel_answer", "/recall")?),
                ),
                metric(
                    "guard_verdict",
                    json!(required_str(guard, "calyx.guard.check", "/verdict")?),
                ),
                metric(
                    "guard_distance",
                    json!(required_f64(guard, "calyx.guard.check", "/distance")?),
                ),
                metric(
                    "guard_tau",
                    json!(required_f64(guard, "calyx.guard.check", "/tau")?),
                ),
            ]
        }
    };
    Ok(metrics)
}

fn focus_cx_ids(
    kind: EditorIntelligenceSurfaceKind,
    by_tool: &BTreeMap<&str, &EditorIntelligenceObservation>,
) -> Result<Vec<String>, EngineError> {
    let ids = match kind {
        EditorIntelligenceSurfaceKind::AssociationGraph => non_empty_array(
            payload(by_tool, "calyx.neighbors")?,
            "calyx.neighbors",
            "/neighbors",
        )?
        .iter()
        .filter_map(|item| {
            item.get("cx_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect(),
        EditorIntelligenceSurfaceKind::Kernel => string_array(
            payload(by_tool, "calyx.kernel")?,
            "calyx.kernel",
            "/kernel_cx_ids",
        )?,
        EditorIntelligenceSurfaceKind::WhatIf => string_array(
            payload(by_tool, "calyx.kernel_answer")?,
            "calyx.kernel_answer",
            "/kernel_cx_ids",
        )?,
        _ => Vec::new(),
    };
    Ok(ids)
}

fn warnings_for(
    kind: EditorIntelligenceSurfaceKind,
    by_tool: &BTreeMap<&str, &EditorIntelligenceObservation>,
) -> Result<Vec<String>, EngineError> {
    let mut warnings = Vec::new();
    match kind {
        EditorIntelligenceSurfaceKind::Abundance => {
            let payload = payload(by_tool, "calyx.abundance")?;
            if required_u64(payload, "calyx.abundance", "/materialized")?
                != required_u64(payload, "calyx.abundance", "/n")?
            {
                warnings.push("materialized count differs from total constellations".to_string());
            }
        }
        EditorIntelligenceSurfaceKind::Bits => {
            let payload = payload(by_tool, "calyx.bits")?;
            if non_empty_array(payload, "calyx.bits", "/per_slot")?
                .iter()
                .any(|slot| slot.get("low_signal").and_then(Value::as_bool) == Some(true))
            {
                warnings.push("one or more bits slots are low signal".to_string());
            }
        }
        EditorIntelligenceSurfaceKind::Kernel => {
            let gaps = required_array(
                payload(by_tool, "calyx.kernel")?,
                "calyx.kernel",
                "/grounding_gaps",
            )?;
            if !gaps.is_empty() {
                warnings.push("kernel returned grounding gaps".to_string());
            }
        }
        EditorIntelligenceSurfaceKind::WhatIf => {
            let guard = payload(by_tool, "calyx.guard.check")?;
            if required_str(guard, "calyx.guard.check", "/verdict")? != "pass" {
                warnings.push("guard verdict blocks this what-if candidate".to_string());
            }
            let gaps = required_array(
                payload(by_tool, "calyx.kernel_answer")?,
                "calyx.kernel_answer",
                "/gaps",
            )?;
            if !gaps.is_empty() {
                warnings.push("kernel answer returned gaps".to_string());
            }
        }
        EditorIntelligenceSurfaceKind::AssociationGraph => {}
    }
    Ok(warnings)
}

fn payload<'a>(
    by_tool: &'a BTreeMap<&str, &EditorIntelligenceObservation>,
    tool: &str,
) -> Result<&'a Value, EngineError> {
    by_tool
        .get(tool)
        .map(|observation| &observation.payload)
        .ok_or_else(|| error("source_tool", format!("missing required tool {tool}")))
}

fn required_u64(payload: &Value, tool: &str, path: &str) -> Result<u64, EngineError> {
    payload
        .pointer(path)
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            error(
                "payload",
                format!("{tool} missing unsigned integer at {path}"),
            )
        })
}

fn required_f64(payload: &Value, tool: &str, path: &str) -> Result<f64, EngineError> {
    payload
        .pointer(path)
        .and_then(Value::as_f64)
        .ok_or_else(|| error("payload", format!("{tool} missing number at {path}")))
}

fn required_str<'a>(payload: &'a Value, tool: &str, path: &str) -> Result<&'a str, EngineError> {
    payload
        .pointer(path)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| error("payload", format!("{tool} missing text at {path}")))
}

fn required_array<'a>(
    payload: &'a Value,
    tool: &str,
    path: &str,
) -> Result<&'a Vec<Value>, EngineError> {
    payload
        .pointer(path)
        .and_then(Value::as_array)
        .ok_or_else(|| error("payload", format!("{tool} missing array at {path}")))
}

fn non_empty_array<'a>(
    payload: &'a Value,
    tool: &str,
    path: &str,
) -> Result<&'a Vec<Value>, EngineError> {
    let array = required_array(payload, tool, path)?;
    if array.is_empty() {
        return invalid(
            "payload",
            format!("{tool} array at {path} must be non-empty"),
        );
    }
    Ok(array)
}

fn string_array(payload: &Value, tool: &str, path: &str) -> Result<Vec<String>, EngineError> {
    non_empty_array(payload, tool, path)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|item| !item.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    error(
                        "payload",
                        format!("{tool} array {path} contains non-text id"),
                    )
                })
        })
        .collect()
}

fn metric(name: &str, value: Value) -> EditorIntelligenceMetric {
    EditorIntelligenceMetric {
        name: name.to_string(),
        value,
    }
}

#[derive(Clone, Copy)]
pub(super) struct SurfaceSpec {
    kind: EditorIntelligenceSurfaceKind,
    panel_id: &'static str,
    title: &'static str,
    dock: EditorIntelligenceDock,
    order: u8,
    min_width_px: u16,
    min_height_px: u16,
    tools: &'static [&'static str],
}

pub(super) fn surface_specs() -> Vec<SurfaceSpec> {
    vec![
        SurfaceSpec {
            kind: EditorIntelligenceSurfaceKind::Abundance,
            panel_id: "calyx-abundance",
            title: "Abundance",
            dock: EditorIntelligenceDock::RightInspector,
            order: 1,
            min_width_px: 280,
            min_height_px: 132,
            tools: &["calyx.abundance"],
        },
        SurfaceSpec {
            kind: EditorIntelligenceSurfaceKind::Bits,
            panel_id: "calyx-bits",
            title: "Bits",
            dock: EditorIntelligenceDock::RightInspector,
            order: 2,
            min_width_px: 280,
            min_height_px: 168,
            tools: &["calyx.bits"],
        },
        SurfaceSpec {
            kind: EditorIntelligenceSurfaceKind::AssociationGraph,
            panel_id: "calyx-association-graph",
            title: "Associations",
            dock: EditorIntelligenceDock::CenterOverlay,
            order: 3,
            min_width_px: 360,
            min_height_px: 220,
            tools: &["calyx.neighbors"],
        },
        SurfaceSpec {
            kind: EditorIntelligenceSurfaceKind::Kernel,
            panel_id: "calyx-kernel",
            title: "Kernel",
            dock: EditorIntelligenceDock::BottomPanel,
            order: 4,
            min_width_px: 420,
            min_height_px: 160,
            tools: &["calyx.kernel"],
        },
        SurfaceSpec {
            kind: EditorIntelligenceSurfaceKind::WhatIf,
            panel_id: "calyx-what-if",
            title: "What If",
            dock: EditorIntelligenceDock::RightInspector,
            order: 5,
            min_width_px: 320,
            min_height_px: 188,
            tools: &["calyx.kernel_answer", "calyx.guard.check"],
        },
    ]
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(error(field, reason))
}

pub(super) fn error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.intelligence_surfaces.{}", field.into()),
        reason: reason.into(),
    }
}
