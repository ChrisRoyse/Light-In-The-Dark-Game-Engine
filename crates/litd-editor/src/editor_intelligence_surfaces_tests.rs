use std::fs;

use litd_engine::EngineError;
use serde_json::{Value, json};

use super::*;
use crate::mcp_tools::default_structured_intelligence_mcp_surface_manifest;

#[test]
fn issue1420_builds_all_editor_intelligence_surfaces() {
    let manifest = default_structured_intelligence_mcp_surface_manifest().unwrap();
    let deck = build_editor_intelligence_surface_deck(
        "issue1420-deck",
        "issue1420_structured",
        "loopback MCP payloads over structured records",
        &manifest,
        observations(),
    )
    .unwrap();

    assert_eq!(deck.issue, 1420);
    assert_eq!(deck.source_issue, 1419);
    assert_eq!(deck.surface_count, 5);
    assert_eq!(
        deck.surfaces
            .iter()
            .map(|surface| surface.kind)
            .collect::<Vec<_>>(),
        vec![
            EditorIntelligenceSurfaceKind::Abundance,
            EditorIntelligenceSurfaceKind::Bits,
            EditorIntelligenceSurfaceKind::AssociationGraph,
            EditorIntelligenceSurfaceKind::Kernel,
            EditorIntelligenceSurfaceKind::WhatIf
        ]
    );
    assert!(
        deck.surfaces
            .iter()
            .find(|surface| surface.kind == EditorIntelligenceSurfaceKind::WhatIf)
            .unwrap()
            .source_tools
            .contains(&"calyx.guard.check".to_string())
    );
    validate_editor_intelligence_surface_deck(&deck).unwrap();
}

#[test]
fn issue1420_missing_required_payload_fails_loud() {
    let manifest = default_structured_intelligence_mcp_surface_manifest().unwrap();
    let observations = observations()
        .into_iter()
        .filter(|observation| observation.tool_name != "calyx.bits")
        .collect::<Vec<_>>();
    let error = build_editor_intelligence_surface_deck(
        "issue1420-deck",
        "issue1420_structured",
        "loopback MCP payloads",
        &manifest,
        observations,
    )
    .unwrap_err();

    assert_ui_error(
        error,
        "editor.intelligence_surfaces.source_tool",
        "calyx.bits",
    );
}

#[test]
fn issue1420_unknown_tool_fails_loud() {
    let manifest = default_structured_intelligence_mcp_surface_manifest().unwrap();
    let mut observations = observations();
    observations.push(EditorIntelligenceObservation::new(
        "calyx.ghost",
        "mcp://ghost",
        json!({"ok": true}),
    ));
    let error = build_editor_intelligence_surface_deck(
        "issue1420-deck",
        "issue1420_structured",
        "loopback MCP payloads",
        &manifest,
        observations,
    )
    .unwrap_err();

    assert_ui_error(
        error,
        "editor.intelligence_surfaces.observation.tool_name",
        "not advertised",
    );
}

#[test]
fn issue1420_empty_association_graph_fails_loud() {
    let manifest = default_structured_intelligence_mcp_surface_manifest().unwrap();
    let observations = observations()
        .into_iter()
        .map(|mut observation| {
            if observation.tool_name == "calyx.neighbors" {
                observation.payload = json!({"neighbors": []});
            }
            observation
        })
        .collect::<Vec<_>>();
    let error = build_editor_intelligence_surface_deck(
        "issue1420-deck",
        "issue1420_structured",
        "loopback MCP payloads",
        &manifest,
        observations,
    )
    .unwrap_err();

    assert_ui_error(
        error,
        "editor.intelligence_surfaces.payload",
        "must be non-empty",
    );
}

#[test]
fn issue1420_store_round_trips_deck_bytes() {
    let manifest = default_structured_intelligence_mcp_surface_manifest().unwrap();
    let deck = build_editor_intelligence_surface_deck(
        "issue1420-deck",
        "issue1420_structured",
        "loopback MCP payloads over structured records",
        &manifest,
        observations(),
    )
    .unwrap();
    let root = temp_root("issue1420-store");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let writeback = write_editor_intelligence_surface_deck(&root, &root, &deck).unwrap();
    let loaded = read_editor_intelligence_surface_deck(&root, &deck.deck_id)
        .unwrap()
        .unwrap();
    let parsed = parse_editor_intelligence_surface_deck(&loaded.bytes).unwrap();

    assert_eq!(writeback.bytes, loaded.bytes);
    assert_eq!(writeback.sha256, loaded.sha256);
    assert_eq!(parsed, deck);
    let _ = fs::remove_dir_all(&root);
}

fn observations() -> Vec<EditorIntelligenceObservation> {
    vec![
        observation(
            "calyx.abundance",
            json!({
                "n": 60,
                "pairs": 1770,
                "materialized": 60,
                "n_eff": 1.0,
                "dpi_ceiling": 5.93,
                "panel_size": 1
            }),
        ),
        observation(
            "calyx.bits",
            json!({
                "anchor": "test_pass",
                "panel_sufficiency": 0.02,
                "n": 60,
                "dpi_ceiling": 5.93,
                "per_slot": [{
                    "slot": 8,
                    "name": "summary.text_embedding",
                    "bits": 0.12,
                    "ci": [0.10, 0.13],
                    "estimator": "centroid_cosine_v1",
                    "state": "active",
                    "low_signal": false
                }]
            }),
        ),
        observation(
            "calyx.neighbors",
            json!({
                "neighbors": [
                    {"cx_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "score": 1.0, "slot": 8},
                    {"cx_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "score": 0.98, "slot": 8}
                ]
            }),
        ),
        observation(
            "calyx.kernel",
            json!({
                "kernel_size": 1,
                "recall": 1.0,
                "total_cx": 60,
                "kernel_cx_ids": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
                "grounding_gaps": []
            }),
        ),
        observation(
            "calyx.kernel_answer",
            json!({
                "answer": "grounded kernel answer over structured constellations",
                "kernel_cx_ids": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
                "recall": 1.0,
                "gaps": []
            }),
        ),
        observation(
            "calyx.guard.check",
            json!({
                "verdict": "pass",
                "tau": 0.9,
                "distance": 0.0
            }),
        ),
    ]
}

fn observation(tool: &str, payload: Value) -> EditorIntelligenceObservation {
    EditorIntelligenceObservation::new(tool, format!("mcp://loopback/{tool}/issue1420"), payload)
}

fn assert_ui_error(error: EngineError, field: &str, reason_contains: &str) {
    match error {
        EngineError::UiRenderInvalidConfig {
            field: actual,
            reason,
        } => {
            assert_eq!(actual, field);
            assert!(reason.contains(reason_contains), "{reason}");
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn temp_root(label: &str) -> litd_engine::test_temp::TestTempDir {
    litd_engine::test_temp::TestTempDir::new(label)
}
