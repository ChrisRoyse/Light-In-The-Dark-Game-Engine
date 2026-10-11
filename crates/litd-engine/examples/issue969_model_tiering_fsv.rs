use std::{process::Command, str};

use litd_engine::{
    EngineError,
    ai::{
        AI_AGENT_MODEL_HOT_PATH_POLICY, AgentModelRegistry, AgentModelRole,
        AgentModelSelectionRequest, default_agent_model_registry, select_agent_model,
        validate_agent_model_registry, validate_agent_model_selection_readback,
    },
    fsv_artifacts::begin_issue_run,
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue969_model_tiering")?;
    let registry = default_agent_model_registry();
    validate_agent_model_registry(&registry)?;
    let registry_value = serde_json::to_value(&registry)?;
    let registry_readback =
        run.write_json_artifact("agent-model-registry.json", &registry_value)?;
    let hardware = nvidia_smi_readback()?;

    let object = select_agent_model(
        &registry,
        &request(
            "issue969-object",
            "agent.object.lantern",
            AgentModelRole::ObjectVisual,
            512,
            32,
            0,
        ),
    )?;
    let creature = select_agent_model(
        &registry,
        &request(
            "issue969-creature",
            "agent.creature.guard",
            AgentModelRole::CreatureCombat,
            1_536,
            4,
            object.warm_pool_after_mib,
        ),
    )?;
    let dialogue = select_agent_model(
        &registry,
        &request(
            "issue969-dialogue",
            "agent.npc.barkeep",
            AgentModelRole::NpcDialogue,
            4_096,
            2,
            creature.warm_pool_after_mib,
        ),
    )?;
    let director = select_agent_model(
        &registry,
        &request(
            "issue969-director",
            "agent.immortal.lyrus",
            AgentModelRole::ImmortalDirector,
            8_192,
            1,
            0,
        ),
    )?;
    for selection in [&object, &creature, &dialogue, &director] {
        validate_agent_model_selection_readback(selection)?;
    }
    let selection_value = json!({
        "object": object,
        "creature": creature,
        "dialogue": dialogue,
        "director": director
    });
    let selection_readback =
        run.write_json_artifact("agent-model-selection-readbacks.json", &selection_value)?;

    let edges = vec![
        edge_duplicate_model(&registry),
        edge_context_overflow(&registry),
        edge_warm_pool_exhausted(&registry),
        edge_zero_concurrency(&registry),
        edge_tampered_fingerprint(&registry),
    ];
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let accepted = hardware["matches_issue_hardware"]
        .as_bool()
        .unwrap_or(false)
        && registry_readback.json == registry_value
        && selection_readback.json == selection_value
        && selection_value["object"]["selected_model_id"] == "model.local.tiny_agent"
        && selection_value["creature"]["selected_model_id"] == "model.local.small_creature_agent"
        && selection_value["dialogue"]["selected_model_id"] == "model.local.larger_agent"
        && selection_value["director"]["selected_model_id"] == "model.local.immortal_director"
        && selection_value["dialogue"]["warm_pool_after_mib"]
            .as_u64()
            .unwrap_or(u64::MAX)
            <= u64::from(registry.hardware.warm_pool_budget_mib)
        && [
            &selection_value["object"],
            &selection_value["creature"],
            &selection_value["dialogue"],
        ]
        .iter()
        .all(|selection| {
            selection["hot_path_policy"] == AI_AGENT_MODEL_HOT_PATH_POLICY
                && selection["calyx_live_query_allowed_in_hot_path"] == false
        })
        && all_edges_matched;
    if !accepted {
        return Err("issue969 model tiering acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.engine.issue969.model_tiering_fsv.v1",
        "issue": 969,
        "source_of_truth": {
            "registry": registry_readback.path.display().to_string(),
            "selections": selection_readback.path.display().to_string(),
            "hardware": "nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits",
            "manual_gate": "manual Synapse/browser readback must inspect the persisted registry, selections, hardware readback, hashes, and edge failures"
        },
        "research_basis": [
            "vLLM and TensorRT-LLM both treat batching and KV-cache memory as first-class serving constraints.",
            "Google's Gemma small-model guidance supports 2B/4B-class local edge deployment targets.",
            "llama.cpp/GGUF quantization guidance supports tiny and small quantized local model artifacts."
        ],
        "hardware": hardware,
        "registry": {
            "path": registry_readback.path.display().to_string(),
            "sha256": registry_readback.sha256,
            "bytes": registry_readback.bytes.len(),
            "model_count": registry.models.len(),
            "vram_budget_mib": registry.hardware.vram_budget_mib,
            "warm_pool_budget_mib": registry.hardware.warm_pool_budget_mib
        },
        "selections": {
            "path": selection_readback.path.display().to_string(),
            "sha256": selection_readback.sha256,
            "bytes": selection_readback.bytes.len(),
            "readbacks": selection_value
        },
        "acceptance": {
            "accepted": accepted,
            "hardware_matches_issue_hardware": hardware["matches_issue_hardware"],
            "registry_readback_match": registry_readback.json == registry_value,
            "selection_readback_match": selection_readback.json == selection_value,
            "object_visual_model": selection_value["object"]["selected_model_id"],
            "creature_model": selection_value["creature"]["selected_model_id"],
            "npc_dialogue_model": selection_value["dialogue"]["selected_model_id"],
            "director_model": selection_value["director"]["selected_model_id"],
            "dialogue_warm_pool_after_mib": selection_value["dialogue"]["warm_pool_after_mib"],
            "hot_path_policy": AI_AGENT_MODEL_HOT_PATH_POLICY,
            "all_edges_matched": all_edges_matched
        },
        "edges": edges,
        "sufficiency_argument": "The minimal set covers every #969 role class needed by the next issues: object/visual tiny models, creature/combat small models, Gemma-class NPC dialogue, and high-tier Immortal director selection. The five edges cover duplicate registry rows, context overflow, warm-pool exhaustion, invalid concurrency, and tampered selection readback; adding more rows would not exercise a distinct selection branch."
    });
    let report_readback = run.write_report(&report)?;
    println!(
        "ISSUE969_MODEL_TIERING_FSV report={} sha256={} registry_sha256={} selections_sha256={} accepted={} edges={}",
        report_readback.path.display(),
        report_readback.sha256,
        registry_readback.sha256,
        selection_readback.sha256,
        accepted,
        edges.len()
    );
    Ok(())
}

fn request(
    run_id: &str,
    agent_id: &str,
    role: AgentModelRole,
    required_context_tokens: u32,
    concurrent_requests: u16,
    existing_warm_pool_mib: u32,
) -> AgentModelSelectionRequest {
    AgentModelSelectionRequest {
        run_id: run_id.to_string(),
        agent_id: agent_id.to_string(),
        role,
        required_context_tokens,
        concurrent_requests,
        existing_warm_pool_mib,
        prefer_warm_pool: true,
    }
}

fn edge_duplicate_model(registry: &AgentModelRegistry) -> Value {
    let before = registry_hash(registry);
    let mut duplicate = registry.clone();
    duplicate.models[1].model_id = duplicate.models[0].model_id.clone();
    let result = validate_agent_model_registry(&duplicate);
    edge(
        "duplicate_model",
        result,
        "registry.model_id",
        "duplicate",
        before,
        registry_hash(registry),
    )
}

fn edge_context_overflow(registry: &AgentModelRegistry) -> Value {
    let before = registry_hash(registry);
    let result = select_agent_model(
        registry,
        &request(
            "issue969-context-edge",
            "agent.object.too-long",
            AgentModelRole::ObjectVisual,
            2_048,
            1,
            0,
        ),
    );
    edge(
        "context_overflow",
        result,
        "selection",
        "context",
        before,
        registry_hash(registry),
    )
}

fn edge_warm_pool_exhausted(registry: &AgentModelRegistry) -> Value {
    let before = registry_hash(registry);
    let result = select_agent_model(
        registry,
        &request(
            "issue969-warm-edge",
            "agent.npc.crowd",
            AgentModelRole::NpcDialogue,
            4_096,
            8,
            19_000,
        ),
    );
    edge(
        "warm_pool_exhausted",
        result,
        "selection",
        "warm pool",
        before,
        registry_hash(registry),
    )
}

fn edge_zero_concurrency(registry: &AgentModelRegistry) -> Value {
    let before = registry_hash(registry);
    let result = select_agent_model(
        registry,
        &request(
            "issue969-zero-concurrency",
            "agent.npc.zero",
            AgentModelRole::NpcDialogue,
            4_096,
            0,
            0,
        ),
    );
    edge(
        "zero_concurrency",
        result,
        "selection.concurrent_requests",
        "positive",
        before,
        registry_hash(registry),
    )
}

fn edge_tampered_fingerprint(registry: &AgentModelRegistry) -> Value {
    let before = registry_hash(registry);
    let mut selected = select_agent_model(
        registry,
        &request(
            "issue969-tamper-edge",
            "agent.npc.tampered",
            AgentModelRole::NpcDialogue,
            4_096,
            1,
            0,
        ),
    )
    .expect("valid selection before tamper");
    selected.selection_fingerprint = "0000".to_string();
    let result = validate_agent_model_selection_readback(&selected);
    edge(
        "tampered_fingerprint",
        result,
        "selection.selection_fingerprint",
        "fingerprint mismatch",
        before,
        registry_hash(registry),
    )
}

fn edge<T>(
    label: &str,
    result: Result<T, EngineError>,
    expected_field: &str,
    expected_reason: &str,
    before_registry_sha256: String,
    after_registry_sha256: String,
) -> Value {
    match result {
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "error": "unexpected success",
            "before_registry_sha256": before_registry_sha256,
            "after_registry_sha256": after_registry_sha256
        }),
        Err(EngineError::AgentModelTieringInvalidConfig { field, reason }) => {
            let matched = field.ends_with(expected_field)
                && reason.contains(expected_reason)
                && before_registry_sha256 == after_registry_sha256;
            json!({
                "label": label,
                "matched": matched,
                "field": field,
                "reason": reason,
                "before_registry_sha256": before_registry_sha256,
                "after_registry_sha256": after_registry_sha256,
                "state_unchanged": before_registry_sha256 == after_registry_sha256
            })
        }
        Err(error) => json!({
            "label": label,
            "matched": false,
            "error": error.to_string(),
            "before_registry_sha256": before_registry_sha256,
            "after_registry_sha256": after_registry_sha256
        }),
    }
}

fn registry_hash(registry: &AgentModelRegistry) -> String {
    let bytes = serde_json::to_vec(registry).expect("registry serializes");
    sha256_hex(&bytes)
}

fn nvidia_smi_readback() -> Result<Value, Box<dyn std::error::Error>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()?;
    let stdout = str::from_utf8(&output.stdout)?.trim().to_string();
    let stderr = str::from_utf8(&output.stderr)?.trim().to_string();
    if !output.status.success() {
        return Err(format!("nvidia-smi failed: {stderr}").into());
    }
    let line = stdout
        .lines()
        .next()
        .ok_or("nvidia-smi returned no GPU rows")?;
    let mut parts = line.split(',').map(str::trim);
    let gpu_name = parts.next().ok_or("missing GPU name")?.to_string();
    let memory_total_mib = parts.next().ok_or("missing GPU memory")?.parse::<u32>()?;
    Ok(json!({
        "command": "nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits",
        "stdout": stdout,
        "gpu_name": gpu_name,
        "memory_total_mib": memory_total_mib,
        "matches_issue_hardware": gpu_name.contains("5090") && memory_total_mib >= 30_000
    }))
}
