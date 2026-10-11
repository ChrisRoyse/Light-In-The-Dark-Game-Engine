use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
};
use litd_engine::math::{Transform, Vec3};

use super::*;

#[test]
fn issue1179_latency_budget_source_of_truth_fsv() {
    let document = fixture_document(96);
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("agent-latency-ledger.json");
    let aster_dir = root.join("aster");
    let request = standard_request("issue1179-latency-ledger");
    println!(
        "issue1179 happy before_tiers={} scenario={} expected_kinds=3 expected_cache_modes=2",
        request.tiers.len(),
        request.scenario_id
    );

    let ledger = build_agent_latency_budget_ledger(&document, request).unwrap();
    let writeback = write_agent_latency_budget_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let aster = read_agent_latency_budget_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_agent_latency_budget_ledger(&aster.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert_eq!(parsed.readback.tier_count, 6);
    assert_eq!(parsed.readback.cold_tier_count, 3);
    assert_eq!(parsed.readback.warm_tier_count, 3);
    assert!(parsed.readback.all_tiers_within_budget);
    assert!(parsed.readback.single_change_under_budget);
    assert!(parsed.readback.focused_scenario_under_budget);
    assert!(parsed.readback.full_world_smoke_under_budget);
    assert!(parsed.readback.cold_and_warm_measured);
    println!(
        "issue1179 happy after_aster seq={} graph_rows={} ledger_sha={} legacy_state_exists={} tiers={} max_ms={:.3} slowest_stage={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_path.exists(),
        parsed.readback.tier_count,
        parsed.readback.max_elapsed_ms,
        parsed.readback.slowest_stage_name
    );
}

#[test]
fn issue1179_edges_breach_cold_warm_and_invalid_state() {
    let document = fixture_document(48);
    let root = test_root("edges");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("agent-latency-ledger.json");
    let aster_dir = root.join("aster");
    let baseline =
        build_agent_latency_budget_ledger(&document, standard_request("issue1179-edge-baseline"))
            .unwrap();
    write_agent_latency_budget_ledger(&state_path, &aster_dir, &baseline).unwrap();
    let before_state = read_agent_latency_budget_ledger(&aster_dir, &baseline.ledger_id)
        .unwrap()
        .unwrap()
        .bytes;
    assert!(!state_path.exists());

    let breach_error =
        build_agent_latency_budget_ledger(&document, breach_request("issue1179-breach"))
            .unwrap_err()
            .to_string();
    let after_breach = read_agent_latency_budget_ledger(&aster_dir, &baseline.ledger_id)
        .unwrap()
        .unwrap()
        .bytes;
    assert_eq!(after_breach, before_state);
    assert!(breach_error.contains("slowest_stage="));
    println!(
        "issue1179 edge_breach before_bytes={} after_bytes={} after_error={} state_unchanged=true",
        before_state.len(),
        after_breach.len(),
        breach_error
    );

    let cold_only_error =
        build_agent_latency_budget_ledger(&document, cold_only_request("issue1179-cold-only"))
            .unwrap_err()
            .to_string();
    let after_cold_only = read_agent_latency_budget_ledger(&aster_dir, &baseline.ledger_id)
        .unwrap()
        .unwrap()
        .bytes;
    assert_eq!(after_cold_only, before_state);
    println!(
        "issue1179 edge_cold_warm before_bytes={} after_bytes={} after_error={} state_unchanged=true",
        before_state.len(),
        after_cold_only.len(),
        cold_only_error
    );

    let invalid_error = build_agent_latency_budget_ledger(
        &EditorDocument::new("calyx://issue1179-invalid", "rev-invalid").unwrap(),
        standard_request("issue1179-invalid-document"),
    )
    .unwrap_err()
    .to_string();
    let after_invalid = read_agent_latency_budget_ledger(&aster_dir, &baseline.ledger_id)
        .unwrap()
        .unwrap()
        .bytes;
    assert_eq!(after_invalid, before_state);
    println!(
        "issue1179 edge_invalid before_bytes={} after_bytes={} after_error={} state_unchanged=true",
        before_state.len(),
        after_invalid.len(),
        invalid_error
    );

    let mut oversized = standard_request("issue1179-oversized");
    oversized.tiers[0].body_count = 1_025;
    let capacity_error = build_agent_latency_budget_ledger(&document, oversized)
        .unwrap_err()
        .to_string();
    let after_capacity = read_agent_latency_budget_ledger(&aster_dir, &baseline.ledger_id)
        .unwrap()
        .unwrap()
        .bytes;
    assert_eq!(after_capacity, before_state);
    assert!(capacity_error.contains("body_count"));
    println!(
        "issue1179 edge_capacity before_bytes={} after_bytes={} after_error={} state_unchanged=true",
        before_state.len(),
        after_capacity.len(),
        capacity_error
    );
}

pub(super) fn standard_request(ledger_id: &str) -> AgentLatencyBudgetRequest {
    AgentLatencyBudgetRequest {
        ledger_id: ledger_id.to_string(),
        project_id: "litd.project.issue1179.fsv".to_string(),
        actor_id: "agent.qa.ember".to_string(),
        scenario_id: "issue1179-agent-loop-representative-world".to_string(),
        camera_rig: PlayCameraRig::TopDown,
        tiers: vec![
            tier(
                "single-cold",
                AgentLatencyTierKind::SingleChangeRevalidate,
                AgentLatencyCacheMode::Cold,
                SINGLE_CHANGE_REVALIDATE_BUDGET_MS,
                16,
                16,
            ),
            tier(
                "single-warm",
                AgentLatencyTierKind::SingleChangeRevalidate,
                AgentLatencyCacheMode::Warm,
                SINGLE_CHANGE_REVALIDATE_BUDGET_MS,
                16,
                16,
            ),
            tier(
                "focused-cold",
                AgentLatencyTierKind::FocusedScenario,
                AgentLatencyCacheMode::Cold,
                FOCUSED_SCENARIO_BUDGET_MS,
                240,
                128,
            ),
            tier(
                "focused-warm",
                AgentLatencyTierKind::FocusedScenario,
                AgentLatencyCacheMode::Warm,
                FOCUSED_SCENARIO_BUDGET_MS,
                240,
                128,
            ),
            tier(
                "smoke-cold",
                AgentLatencyTierKind::FullWorldSmoke,
                AgentLatencyCacheMode::Cold,
                FULL_WORLD_SMOKE_BUDGET_MS,
                900,
                512,
            ),
            tier(
                "smoke-warm",
                AgentLatencyTierKind::FullWorldSmoke,
                AgentLatencyCacheMode::Warm,
                FULL_WORLD_SMOKE_BUDGET_MS,
                900,
                512,
            ),
        ],
    }
}

fn breach_request(ledger_id: &str) -> AgentLatencyBudgetRequest {
    let mut request = standard_request(ledger_id);
    request.tiers[0].budget_ms = 0.000_001;
    request
}

fn cold_only_request(ledger_id: &str) -> AgentLatencyBudgetRequest {
    let mut request = standard_request(ledger_id);
    request
        .tiers
        .retain(|tier| tier.cache_mode == AgentLatencyCacheMode::Cold);
    request
}

fn tier(
    tier_id: &str,
    kind: AgentLatencyTierKind,
    cache_mode: AgentLatencyCacheMode,
    budget_ms: f64,
    sim_ticks: u64,
    body_count: usize,
) -> AgentLatencyTierPlan {
    AgentLatencyTierPlan {
        tier_id: tier_id.to_string(),
        kind,
        cache_mode,
        budget_ms,
        sim_ticks,
        body_count,
    }
}

fn fixture_document(extra_doodads: usize) -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1179-agent-latency", "issue1179-rev-1").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_playtest_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    for index in 0..extra_doodads {
        document
            .upsert_object(EditorObject::new(
                format!("doodad_{index:04}"),
                "Doodad",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(index as f32, 0.0, -(index as f32))),
            ))
            .unwrap();
    }
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    document
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1179", label)
}
