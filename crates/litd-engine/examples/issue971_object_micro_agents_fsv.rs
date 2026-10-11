#[path = "issue971_object_micro_agents_fsv/support.rs"]
mod support;

use litd_engine::{
    ai::{
        AI_AGENT_MODEL_HOT_PATH_POLICY, AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY, AgentModelRole,
        AgentModelSelectionRequest, default_agent_model_registry, evaluate_object_micro_agents,
        select_agent_model, validate_object_micro_agent_readback,
    },
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    render::vfx::validate_effect,
};
use serde_json::json;
use support::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue971_object_micro_agents")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    let model_blob = write_embedded_model_blob(run.run_dir())?;
    let model_blob_value = serde_json::to_value(&model_blob)?;
    let model_blob_readback =
        run.write_json_artifact("embedded-model-blob.json", &model_blob_value)?;
    let model_content = store.put_content(&content(
        EngineContentKind::Asset,
        "embedded-lantern-glow-model",
        "Embedded Lantern Glow 5MiB Model",
        json!({
            "model_id": model_blob.model_id,
            "bytes": model_blob.bytes,
            "sha256": model_blob.sha256,
            "path": model_blob.path.display().to_string(),
            "media_type": model_blob.media_type
        }),
    ))?;

    let effect = lantern_glow_effect()?;
    let validation = validate_effect(&effect)?;
    let effect_value = serde_json::to_value(&effect)?;
    let validation_value = serde_json::to_value(&validation)?;
    let effect_readback = run.write_json_artifact("lantern-glow-vfx-effect.json", &effect_value)?;
    let validation_readback =
        run.write_json_artifact("lantern-glow-vfx-validation.json", &validation_value)?;
    let vfx_content = store.put_content(&content(
        EngineContentKind::Asset,
        "vfx-micro-lantern-glow",
        "Micro-Agent Lantern Glow VFX",
        json!({
            "effect_id": effect.effect_id,
            "effect_sha256": effect_readback.sha256,
            "validation_sha256": validation_readback.sha256
        }),
    ))?;
    let agent_content = store.put_content(&content(
        EngineContentKind::Agent,
        "object-lantern-micro-agent-template",
        "Lantern Object Micro-Agent Template",
        json!({
            "embedded_model": model_content.id,
            "vfx_effect": vfx_content.id,
            "spawnable_count": OBJECT_COUNT,
            "ward_tags": ["lantern", "proximity", "awake_visual"]
        }),
    ))?;

    let registry = default_agent_model_registry();
    let selection = select_agent_model(
        &registry,
        &AgentModelSelectionRequest {
            run_id: "issue971-object-model-selection".to_string(),
            agent_id: "agent.object.lantern.swarm".to_string(),
            role: AgentModelRole::ObjectVisual,
            required_context_tokens: 512,
            concurrent_requests: OBJECT_COUNT as u16,
            existing_warm_pool_mib: 0,
            prefer_warm_pool: false,
        },
    )?;
    let selection_value = serde_json::to_value(&selection)?;
    let selection_readback =
        run.write_json_artifact("object-visual-model-selection.json", &selection_value)?;

    let provenance_refs = vec![
        format!("calyx://engine-content/asset/{}", model_content.id),
        format!("calyx://engine-content/asset/{}", vfx_content.id),
        format!("calyx://engine-content/agent/{}", agent_content.id),
    ];
    let input = object_micro_agent_input(&model_blob, &provenance_refs);
    let input_value = serde_json::to_value(&input)?;
    let input_readback = run.write_json_artifact("object-micro-agent-input.json", &input_value)?;
    let readback = evaluate_object_micro_agents(&input)?;
    validate_object_micro_agent_readback(&readback)?;
    let readback_value = serde_json::to_value(&readback)?;
    let micro_readback =
        run.write_json_artifact("object-micro-agent-readback.json", &readback_value)?;

    let edges = edge_cases(&input);
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let accepted = model_blob.bytes == MODEL_BYTES
        && model_blob_readback.json == model_blob_value
        && effect_readback.json == effect_value
        && validation.total_max_particles == 8
        && selection.selected_model_id == "model.local.tiny_agent"
        && selection.role == AgentModelRole::ObjectVisual
        && selection.concurrent_object_policy_matches()
        && readback.agent_count == OBJECT_COUNT
        && readback.accepted_count == OBJECT_COUNT
        && readback.visual_effect_count == OBJECT_COUNT
        && readback.total_particles_reserved == 2_048
        && readback.total_embedded_model_bytes == OBJECT_COUNT as u64 * MODEL_BYTES
        && readback.hot_path_policy == AI_OBJECT_MICRO_AGENT_HOT_PATH_POLICY
        && !readback.calyx_live_query_allowed_in_hot_path
        && all_edges_matched;
    if !accepted {
        return Err("issue971 object micro-agent FSV acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.engine.issue971.object_micro_agents_fsv.v1",
        "issue": 971,
        "source_of_truth": {
            "vault_dir": vault_dir.display().to_string(),
            "embedded_model_blob": model_blob_readback.path.display().to_string(),
            "vfx_effect": effect_readback.path.display().to_string(),
            "vfx_validation": validation_readback.path.display().to_string(),
            "model_selection": selection_readback.path.display().to_string(),
            "object_micro_agent_input": input_readback.path.display().to_string(),
            "object_micro_agent_readback": micro_readback.path.display().to_string(),
            "manual_gate": "manual Synapse/browser readback must inspect the persisted report, real 5MiB model blob metadata, Calyx rows, VFX validation, model selection, deterministic lowered effects, hashes, and edge failures"
        },
        "readme_visual_floor": [
            "docs/readme/06-living-agents.png",
            "docs/readme/09-vfx.png",
            "docs/readme/ED03-object-editor.png",
            "docs/readme/ED08-vfx-editor.png",
            "docs/readme/S17-agent-binding.png"
        ],
        "calyx": {
            "model_asset": model_content,
            "vfx_asset": vfx_content,
            "agent_template": agent_content
        },
        "embedded_model_blob": {
            "path": model_blob.path.display().to_string(),
            "artifact_json": model_blob_readback.path.display().to_string(),
            "artifact_json_sha256": model_blob_readback.sha256,
            "bytes": model_blob.bytes,
            "sha256": model_blob.sha256,
            "model_id": model_blob.model_id
        },
        "vfx": {
            "effect_path": effect_readback.path.display().to_string(),
            "effect_sha256": effect_readback.sha256,
            "validation_path": validation_readback.path.display().to_string(),
            "validation_sha256": validation_readback.sha256,
            "validation": validation
        },
        "model_selection": {
            "path": selection_readback.path.display().to_string(),
            "sha256": selection_readback.sha256,
            "hot_path_policy": AI_AGENT_MODEL_HOT_PATH_POLICY,
            "selection": selection
        },
        "object_micro_agents": {
            "input_path": input_readback.path.display().to_string(),
            "input_sha256": input_readback.sha256,
            "readback_path": micro_readback.path.display().to_string(),
            "readback_sha256": micro_readback.sha256,
            "readback": readback
        },
        "acceptance": {
            "accepted": accepted,
            "real_model_blob_bytes": model_blob.bytes,
            "real_model_blob_is_5mib": model_blob.bytes == MODEL_BYTES,
            "object_visual_model": selection_value["selected_model_id"],
            "concurrent_requests": OBJECT_COUNT,
            "agent_count": readback_value["agent_count"],
            "accepted_count": readback_value["accepted_count"],
            "visual_effect_count": readback_value["visual_effect_count"],
            "total_particles_reserved": readback_value["total_particles_reserved"],
            "total_embedded_model_bytes": readback_value["total_embedded_model_bytes"],
            "hot_path_policy": readback_value["hot_path_policy"],
            "live_calyx_hot_path": readback_value["calyx_live_query_allowed_in_hot_path"],
            "all_edges_matched": all_edges_matched
        },
        "edges": edges,
        "sufficiency_argument": "The run proves the #971 branch that matters for play: a real 5MiB embedded object model artifact is referenced by 256 object micro-agents, Calyx-origin signals are Ward-gated and lowered into deterministic presentation-only VFX effects, the #969 ObjectVisual tiny tier accepts a 256-request swarm, and every failure edge closes loudly without fallback."
    });
    let report_readback = run.write_report(&report)?;
    println!(
        "ISSUE971_OBJECT_MICRO_AGENTS_FSV report={} sha256={} model_blob_sha256={} readback_sha256={} accepted_count={} total_model_bytes={} edges={}",
        report_readback.path.display(),
        report_readback.sha256,
        model_blob.sha256,
        micro_readback.sha256,
        readback.accepted_count,
        readback.total_embedded_model_bytes,
        edges.len()
    );
    Ok(())
}

trait SelectionPolicy {
    fn concurrent_object_policy_matches(&self) -> bool;
}

impl SelectionPolicy for litd_engine::ai::AgentModelSelectionReadback {
    fn concurrent_object_policy_matches(&self) -> bool {
        self.selected_model_id == "model.local.tiny_agent"
            && self.hot_path_policy == AI_AGENT_MODEL_HOT_PATH_POLICY
            && !self.calyx_live_query_allowed_in_hot_path
            && self.warm_pool_slots_reserved == OBJECT_COUNT as u16
    }
}
