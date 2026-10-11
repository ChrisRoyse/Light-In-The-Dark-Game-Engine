use std::collections::{BTreeSet, VecDeque};

use litd_engine::EngineError;

use crate::world_graph_navigation::{
    WorldGraphTraversalMode, WorldGraphTraversalStatus, WorldGraphTraverseQuery,
    validate_world_graph_navigation_ledger,
};

use super::{
    WAYFINDING_AUTHORING_SCHEMA, WayfindingAuthoringLedger, WayfindingAuthoringRequest,
    WayfindingChain, WayfindingCueKind, WayfindingObjective, WayfindingRouteStep,
    WayfindingUiProjection, WayfindingValidationReport, WayfindingValidationStatus,
};

#[path = "wayfinding_authoring_validate_support.rs"]
mod wayfinding_authoring_validate_support;
use wayfinding_authoring_validate_support::{
    require_some, sorted_cue_kinds, ui_projection, validate_direction_text, validate_optional,
    validate_refs, validate_refs_allow_empty, validate_single_line,
};

const MAX_TEXT_LEN: usize = 2048;
const MAX_LIST_LEN: usize = 64;
const REQUIRED_CUES: [WayfindingCueKind; 4] = [
    WayfindingCueKind::NpcRumor,
    WayfindingCueKind::Signpost,
    WayfindingCueKind::MapLandmark,
    WayfindingCueKind::EnvironmentalSilhouette,
];

pub fn validate_wayfinding_request(
    request: &WayfindingAuthoringRequest,
) -> Result<WayfindingValidationReport, EngineError> {
    validate_request_shape(request)?;
    let objective = &request.objective;
    let chain = request.chain.as_ref().ok_or_else(|| {
        wayfinding_error(
            "request.chain",
            "hidden/no-marker RPG objectives require a diegetic wayfinding chain",
        )
    })?;
    validate_chain_identity(objective, chain)?;
    let route_anchor_path = validate_steps(chain)?;
    validate_required_cues(chain)?;
    let traversal = crate::world_graph_navigation::traverse_world_graph(
        &request.graph,
        &WorldGraphTraverseQuery {
            query_id: format!("{}-world-graph", request.request_id),
            start_node_id: chain.start_anchor_id.clone(),
            target_node_id: chain.destination_anchor_id.clone(),
            max_hops: request.max_hops,
            mode: WorldGraphTraversalMode::FewestHops,
            requested_by: request.requested_by.clone(),
        },
    )?;
    if traversal.status != WorldGraphTraversalStatus::Reached {
        return Err(wayfinding_error(
            "graph.route",
            format!(
                "world graph did not connect authored route: {}",
                traversal.code
            ),
        ));
    }
    let graph_path = traversal
        .path
        .iter()
        .map(|node| node.node_id.clone())
        .collect::<Vec<_>>();
    if graph_path != route_anchor_path {
        return Err(wayfinding_error(
            "graph.route",
            format!(
                "authored route {:?} does not match graph path {:?}",
                route_anchor_path, graph_path
            ),
        ));
    }
    let cue_kinds = sorted_cue_kinds(chain);
    Ok(WayfindingValidationReport {
        schema: WAYFINDING_AUTHORING_SCHEMA.to_string(),
        request_id: request.request_id.clone(),
        quest_id: objective.quest_id.clone(),
        objective_id: objective.objective_id.clone(),
        status: WayfindingValidationStatus::Validated,
        code: "LITD-WAYFINDING-VALIDATED".to_string(),
        reasons: vec![
            "hidden RPG objective has a reachable, in-world wayfinding chain".to_string(),
            "UI projection contains no GPS, quest marker, route line, or player dot".to_string(),
        ],
        route_anchor_path,
        cue_kinds,
        traversal,
        ui_projection: ui_projection(objective, chain),
    })
}

pub fn validate_wayfinding_ledger(ledger: &WayfindingAuthoringLedger) -> Result<(), EngineError> {
    if ledger.schema != WAYFINDING_AUTHORING_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.report_count == 0
        || ledger.report_count != ledger.reports.len()
    {
        return Err(wayfinding_error(
            "ledger",
            "schema, row-safe id, and report count are required",
        ));
    }
    let graph_ledger = crate::world_graph_navigation::WorldGraphNavigationLedger {
        schema: crate::world_graph_navigation::WORLD_GRAPH_NAVIGATION_SCHEMA.to_string(),
        ledger_id: format!("{}-graph", ledger.ledger_id),
        graph: ledger.graph.clone(),
        traversal_count: ledger.reports.len(),
        traversals: ledger
            .reports
            .iter()
            .map(|report| report.traversal.clone())
            .collect(),
    };
    validate_world_graph_navigation_ledger(&graph_ledger)?;
    let mut ids = BTreeSet::new();
    for report in &ledger.reports {
        validate_report(report)?;
        if !ids.insert(report.request_id.as_str()) {
            return Err(wayfinding_error(
                "ledger.duplicate",
                format!("duplicate request id {}", report.request_id),
            ));
        }
    }
    Ok(())
}

pub fn build_wayfinding_ledger(
    ledger_id: impl Into<String>,
    request: &WayfindingAuthoringRequest,
    reports: Vec<WayfindingValidationReport>,
) -> Result<WayfindingAuthoringLedger, EngineError> {
    let ledger = WayfindingAuthoringLedger {
        schema: WAYFINDING_AUTHORING_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        graph: request.graph.clone(),
        report_count: reports.len(),
        reports,
    };
    validate_wayfinding_ledger(&ledger)?;
    Ok(ledger)
}

fn validate_request_shape(request: &WayfindingAuthoringRequest) -> Result<(), EngineError> {
    validate_single_line("request.request_id", &request.request_id)?;
    validate_single_line("request.requested_by", &request.requested_by)?;
    validate_objective(&request.objective)?;
    if !request.objective.marker_policy.requires_diegetic_chain() {
        return Err(wayfinding_error(
            "objective.marker_policy",
            "issue1447 validates hidden/no automatic marker RPG objectives only",
        ));
    }
    if request.max_hops == 0 || request.max_hops > MAX_LIST_LEN {
        return Err(wayfinding_error(
            "request.max_hops",
            "max_hops must be in 1..=64",
        ));
    }
    Ok(())
}

fn validate_objective(objective: &WayfindingObjective) -> Result<(), EngineError> {
    validate_single_line("objective.quest_id", &objective.quest_id)?;
    validate_single_line("objective.objective_id", &objective.objective_id)?;
    validate_single_line("objective.title", &objective.title)?;
    validate_single_line("objective.description", &objective.description)?;
    validate_single_line("objective.target_ref", &objective.target_ref)?;
    validate_refs("objective.provenance_refs", &objective.provenance_refs)?;
    if objective
        .start_anchor_id
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(wayfinding_error(
            "objective.start_anchor_id",
            "hidden objective needs a known start/source anchor",
        ));
    }
    if objective
        .destination_anchor_id
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(wayfinding_error(
            "objective.destination_anchor_id",
            "hidden objective needs a known destination anchor",
        ));
    }
    if objective
        .chain_id
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(wayfinding_error(
            "objective.chain_id",
            "hidden objective needs an authored wayfinding chain id",
        ));
    }
    Ok(())
}

fn validate_chain_identity(
    objective: &WayfindingObjective,
    chain: &WayfindingChain,
) -> Result<(), EngineError> {
    validate_single_line("chain.chain_id", &chain.chain_id)?;
    validate_refs("chain.provenance_refs", &chain.provenance_refs)?;
    if objective.chain_id.as_deref() != Some(chain.chain_id.as_str())
        || objective.quest_id != chain.quest_id
        || objective.objective_id != chain.objective_id
        || objective.start_anchor_id.as_deref() != Some(chain.start_anchor_id.as_str())
        || objective.destination_anchor_id.as_deref() != Some(chain.destination_anchor_id.as_str())
    {
        return Err(wayfinding_error(
            "chain.identity",
            "chain must match objective quest, objective, start, and destination",
        ));
    }
    if chain.route_steps.is_empty() || chain.route_steps.len() > MAX_LIST_LEN {
        return Err(wayfinding_error(
            "chain.route_steps",
            "route step count must be in 1..=64",
        ));
    }
    Ok(())
}

fn validate_steps(chain: &WayfindingChain) -> Result<Vec<String>, EngineError> {
    let mut seen = BTreeSet::new();
    let mut path = VecDeque::from([chain.start_anchor_id.clone()]);
    let mut expected_from = chain.start_anchor_id.as_str();
    for step in &chain.route_steps {
        validate_step(step)?;
        if !seen.insert(step.step_id.as_str()) {
            return Err(wayfinding_error(
                "step.step_id",
                format!("duplicate route step {}", step.step_id),
            ));
        }
        if step.from_anchor_id != expected_from {
            return Err(wayfinding_error(
                "step.from_anchor_id",
                format!(
                    "step {} starts at {}, expected {}",
                    step.step_id, step.from_anchor_id, expected_from
                ),
            ));
        }
        expected_from = &step.to_anchor_id;
        path.push_back(step.to_anchor_id.clone());
    }
    if expected_from != chain.destination_anchor_id {
        return Err(wayfinding_error(
            "chain.destination_anchor_id",
            "last route step must arrive at the destination anchor",
        ));
    }
    Ok(path.into_iter().collect())
}

fn validate_step(step: &WayfindingRouteStep) -> Result<(), EngineError> {
    validate_single_line("step.step_id", &step.step_id)?;
    validate_single_line("step.from_anchor_id", &step.from_anchor_id)?;
    validate_single_line("step.to_anchor_id", &step.to_anchor_id)?;
    validate_direction_text("step.direction_text", &step.direction_text, step)?;
    validate_refs("step.provenance_refs", &step.provenance_refs)?;
    validate_refs_allow_empty("step.landmark_refs", &step.landmark_refs)?;
    validate_refs_allow_empty("step.danger_notes", &step.danger_notes)?;
    if step.reliability_milli == 0 || step.reliability_milli > 1000 {
        return Err(wayfinding_error(
            "step.reliability_milli",
            "reliability must be in 1..=1000",
        ));
    }
    if step.reliability_milli < 850
        && step
            .ambiguity_note
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        return Err(wayfinding_error(
            "step.ambiguity_note",
            "ambiguous directions need an explicit in-world ambiguity note",
        ));
    }
    validate_optional("step.signpost_text", &step.signpost_text)?;
    validate_optional("step.dialogue_text", &step.dialogue_text)?;
    validate_optional("step.journal_text", &step.journal_text)?;
    validate_optional("step.environmental_text", &step.environmental_text)?;
    validate_cue_payload(step)
}

fn validate_cue_payload(step: &WayfindingRouteStep) -> Result<(), EngineError> {
    match step.cue_kind {
        WayfindingCueKind::RoadCue | WayfindingCueKind::Signpost => {
            require_some("step.signpost_text", &step.signpost_text)
        }
        WayfindingCueKind::NpcRumor => require_some("step.dialogue_text", &step.dialogue_text),
        WayfindingCueKind::JournalClue => require_some("step.journal_text", &step.journal_text),
        WayfindingCueKind::EnvironmentalSilhouette => {
            require_some("step.environmental_text", &step.environmental_text)
        }
        WayfindingCueKind::Landmark | WayfindingCueKind::MapLandmark => {
            if step.landmark_refs.is_empty() {
                return Err(wayfinding_error(
                    "step.landmark_refs",
                    "landmark and map cues need at least one landmark ref",
                ));
            }
            Ok(())
        }
        WayfindingCueKind::DangerNote => {
            if step.danger_notes.is_empty() {
                return Err(wayfinding_error(
                    "step.danger_notes",
                    "danger cues need at least one danger note",
                ));
            }
            Ok(())
        }
    }
}

fn validate_required_cues(chain: &WayfindingChain) -> Result<(), EngineError> {
    let cues = sorted_cue_kinds(chain);
    for required in REQUIRED_CUES {
        if !cues.contains(&required) {
            return Err(wayfinding_error(
                "chain.required_cues",
                format!("missing required issue1447 cue kind {required:?}"),
            ));
        }
    }
    Ok(())
}

fn validate_report(report: &WayfindingValidationReport) -> Result<(), EngineError> {
    if report.schema != WAYFINDING_AUTHORING_SCHEMA
        || report.code != "LITD-WAYFINDING-VALIDATED"
        || report.reasons.is_empty()
        || report.route_anchor_path.len() < 2
        || report.ui_projection.gps_marker_count != 0
        || report.ui_projection.quest_marker_count != 0
        || report.ui_projection.player_position_indicator
        || report.ui_projection.route_line_count != 0
    {
        return Err(wayfinding_error(
            "report",
            "validated report must carry route proof and no GPS UI fallback",
        ));
    }
    Ok(())
}

pub(crate) fn wayfinding_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.wayfinding_authoring.{}", field.into()),
        reason: reason.to_string(),
    }
}
