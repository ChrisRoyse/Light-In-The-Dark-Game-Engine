use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    agent_latency_budget::{
        AgentLatencyBudgetRequest, AgentLatencyTierKind, build_agent_latency_budget_ledger,
        parse_agent_latency_budget_ledger, read_agent_latency_budget_ledger,
        write_agent_latency_budget_ledger,
    },
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    play::PlayCameraRig,
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1179_FSV_ROOT",
        "issue1179_agent_latency",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("agent-latency-ledger.json");
    let artifact_path = root.join("issue1179-agent-latency-readback.json");
    let document = fixture_document(96)?;
    let request = standard_request("issue1179-manual-ledger");
    println!(
        "happy_path before_tiers={} scenario={} expected_kinds=3 expected_cache_modes=2",
        request.tiers.len(),
        request.scenario_id
    );
    let ledger = build_agent_latency_budget_ledger(&document, request)?;
    let writeback = write_agent_latency_budget_ledger(&ledger_path, &aster_dir, &ledger)?;
    let aster = read_agent_latency_budget_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing agent-latency ledger in Aster")?;
    let parsed = parse_agent_latency_budget_ledger(&aster.bytes)?;
    let state_bytes = fs::read(&ledger_path)?;
    if state_bytes != aster.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    println!(
        "happy_path after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} tiers={} max_ms={:.3} slowest_stage={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_bytes == aster.bytes,
        parsed.readback.tier_count,
        parsed.readback.max_elapsed_ms,
        parsed.readback.slowest_stage_name
    );

    let edge_breach = edge_budget_breach(&document, &ledger_path, &state_bytes)?;
    let edge_cold_warm = edge_cold_warm_required(&document, &ledger_path, &state_bytes)?;
    let edge_invalid = edge_invalid_state(&ledger_path, &state_bytes)?;
    let edge_capacity = edge_capacity_guard(&document, &ledger_path, &state_bytes)?;
    let artifact = json!({
        "issue": 1179,
        "source_of_truth": "Aster Graph CF agent latency-budget ledger row plus JSON state-file readback",
        "artifact_path": artifact_path,
        "happy_path": {
            "ledger": {
                "path": ledger_path,
                "state_file_matches_aster": state_bytes == aster.bytes,
                "state_sha256": sha256_hex(&state_bytes),
                "aster_sha256": aster.sha256,
                "aster_seq": aster.seq,
                "aster_graph_rows": aster.graph_rows,
                "cell_key": writeback.cell_key,
                "index_key": writeback.index_key,
                "bytes": state_bytes.len()
            },
            "latency_budget": {
                "measurements": parsed.measurements,
                "readback": parsed.readback
            }
        },
        "edge_cases": [edge_breach, edge_cold_warm, edge_invalid, edge_capacity]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1179 agent latency FSV artifact={} bytes={} artifact_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256
    );
    Ok(())
}

fn edge_budget_breach(
    document: &EditorDocument,
    ledger_path: &std::path::Path,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let error = build_agent_latency_budget_ledger(
        document,
        breach_request("issue1179-manual-budget-breach"),
    )
    .expect_err("budget breach should fail")
    .to_string();
    let after_state = fs::read(ledger_path)?;
    println!(
        "edge_breach before_bytes={} after_bytes={} after_error={} state_unchanged={}",
        before_state.len(),
        after_state.len(),
        error,
        after_state == before_state
    );
    Ok(json!({
        "name": "budget_breach_names_slow_stage",
        "before_state_bytes": before_state.len(),
        "after_state_bytes": after_state.len(),
        "after_error": error,
        "state_unchanged": after_state == before_state
    }))
}

fn edge_cold_warm_required(
    document: &EditorDocument,
    ledger_path: &std::path::Path,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let error = build_agent_latency_budget_ledger(
        document,
        cold_only_request("issue1179-manual-cold-only"),
    )
    .expect_err("cold-only gate should fail")
    .to_string();
    let after_state = fs::read(ledger_path)?;
    println!(
        "edge_cold_warm before_bytes={} after_bytes={} after_error={} state_unchanged={}",
        before_state.len(),
        after_state.len(),
        error,
        after_state == before_state
    );
    Ok(json!({
        "name": "cold_and_warm_cache_required",
        "before_state_bytes": before_state.len(),
        "after_state_bytes": after_state.len(),
        "after_error": error,
        "state_unchanged": after_state == before_state
    }))
}

fn edge_invalid_state(
    ledger_path: &std::path::Path,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let invalid = EditorDocument::new("calyx://world/issue1179-invalid", "rev-invalid")?;
    let error = build_agent_latency_budget_ledger(
        &invalid,
        standard_request("issue1179-manual-invalid-document"),
    )
    .expect_err("invalid document should fail")
    .to_string();
    let after_state = fs::read(ledger_path)?;
    println!(
        "edge_invalid before_bytes={} after_bytes={} after_error={} state_unchanged={}",
        before_state.len(),
        after_state.len(),
        error,
        after_state == before_state
    );
    Ok(json!({
        "name": "invalid_empty_world_blocks_gate",
        "before_state_bytes": before_state.len(),
        "after_state_bytes": after_state.len(),
        "after_error": error,
        "state_unchanged": after_state == before_state
    }))
}

fn edge_capacity_guard(
    document: &EditorDocument,
    ledger_path: &std::path::Path,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut request = standard_request("issue1179-manual-oversized");
    request.tiers[0].body_count = 1_025;
    let error = build_agent_latency_budget_ledger(document, request)
        .expect_err("oversized sim plan should fail before stack allocation")
        .to_string();
    let after_state = fs::read(ledger_path)?;
    println!(
        "edge_capacity before_bytes={} after_bytes={} after_error={} state_unchanged={}",
        before_state.len(),
        after_state.len(),
        error,
        after_state == before_state
    );
    Ok(json!({
        "name": "oversized_sim_body_count_rejected",
        "before_state_bytes": before_state.len(),
        "after_state_bytes": after_state.len(),
        "after_error": error,
        "state_unchanged": after_state == before_state
    }))
}

fn standard_request(ledger_id: &str) -> AgentLatencyBudgetRequest {
    super_request(ledger_id, true)
}

fn breach_request(ledger_id: &str) -> AgentLatencyBudgetRequest {
    let mut request = standard_request(ledger_id);
    request.tiers[0].budget_ms = 0.000_001;
    request
}

fn cold_only_request(ledger_id: &str) -> AgentLatencyBudgetRequest {
    super_request(ledger_id, false)
}

fn super_request(ledger_id: &str, include_warm: bool) -> AgentLatencyBudgetRequest {
    use litd_editor::agent_latency_budget::{
        AgentLatencyCacheMode, AgentLatencyTierPlan, FOCUSED_SCENARIO_BUDGET_MS,
        FULL_WORLD_SMOKE_BUDGET_MS, SINGLE_CHANGE_REVALIDATE_BUDGET_MS,
    };
    let mut tiers = vec![
        AgentLatencyTierPlan {
            tier_id: "single-cold".to_string(),
            kind: AgentLatencyTierKind::SingleChangeRevalidate,
            cache_mode: AgentLatencyCacheMode::Cold,
            budget_ms: SINGLE_CHANGE_REVALIDATE_BUDGET_MS,
            sim_ticks: 16,
            body_count: 16,
        },
        AgentLatencyTierPlan {
            tier_id: "focused-cold".to_string(),
            kind: AgentLatencyTierKind::FocusedScenario,
            cache_mode: AgentLatencyCacheMode::Cold,
            budget_ms: FOCUSED_SCENARIO_BUDGET_MS,
            sim_ticks: 240,
            body_count: 128,
        },
        AgentLatencyTierPlan {
            tier_id: "smoke-cold".to_string(),
            kind: AgentLatencyTierKind::FullWorldSmoke,
            cache_mode: AgentLatencyCacheMode::Cold,
            budget_ms: FULL_WORLD_SMOKE_BUDGET_MS,
            sim_ticks: 900,
            body_count: 512,
        },
    ];
    if include_warm {
        tiers.push(AgentLatencyTierPlan {
            tier_id: "single-warm".to_string(),
            kind: AgentLatencyTierKind::SingleChangeRevalidate,
            cache_mode: AgentLatencyCacheMode::Warm,
            budget_ms: SINGLE_CHANGE_REVALIDATE_BUDGET_MS,
            sim_ticks: 16,
            body_count: 16,
        });
        tiers.push(AgentLatencyTierPlan {
            tier_id: "focused-warm".to_string(),
            kind: AgentLatencyTierKind::FocusedScenario,
            cache_mode: AgentLatencyCacheMode::Warm,
            budget_ms: FOCUSED_SCENARIO_BUDGET_MS,
            sim_ticks: 240,
            body_count: 128,
        });
        tiers.push(AgentLatencyTierPlan {
            tier_id: "smoke-warm".to_string(),
            kind: AgentLatencyTierKind::FullWorldSmoke,
            cache_mode: AgentLatencyCacheMode::Warm,
            budget_ms: FULL_WORLD_SMOKE_BUDGET_MS,
            sim_ticks: 900,
            body_count: 512,
        });
    }
    AgentLatencyBudgetRequest {
        ledger_id: ledger_id.to_string(),
        project_id: "litd.project.issue1179.manual".to_string(),
        actor_id: "agent.qa.ember".to_string(),
        scenario_id: "issue1179-agent-loop-representative-world".to_string(),
        camera_rig: PlayCameraRig::TopDown,
        tiers,
    }
}

fn fixture_document(extra_doodads: usize) -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/issue1179-agent-latency", "issue1179-rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_playtest_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    for index in 0..extra_doodads {
        document.upsert_object(EditorObject::new(
            format!("doodad_{index:04}"),
            "Doodad",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(index as f32, 0.0, -(index as f32))),
        ))?;
    }
    let record = AssetVaultCatalog::load_default_cached()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.agent_latency_budget.fixture.asset_vault".to_string(),
            reason: error.to_string(),
        })?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.agent_latency_budget.fixture.asset".to_string(),
            reason: "missing rematerialized unit mesh asset-vault record (Stan.gltf)".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
