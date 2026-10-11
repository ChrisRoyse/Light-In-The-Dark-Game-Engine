#[path = "issue970_local_dialogue_fsv/support.rs"]
mod support;

use litd_engine::{
    EngineError,
    ai::{
        AI_LOCAL_NPC_DIALOGUE_HOT_PATH_POLICY, AgentModelRole, AgentModelSelectionRequest,
        CalyxDialogueFact, LocalNpcDialogueInput, LocalNpcDialogueIntent,
        LocalNpcDialogueTelemetry, NpcAgentDefinition, NpcAgentIntent, NpcAgentRules,
        NpcAgentRuntimeState, NpcAgentSignalKind, NpcWorldSignal, build_local_npc_dialogue_prompt,
        default_agent_model_registry, evaluate_npc_agent_activations,
        lower_local_npc_dialogue_response, select_agent_model,
        validate_local_npc_dialogue_readback,
    },
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    sim::SimBodyId,
};
use serde_json::{Value, json};
use support::*;

const MODEL: &str = "gemma3:4b-it-q4_K_M";
const ENDPOINT: &str = "http://127.0.0.1:11434/api/generate";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue970_local_dialogue")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let agent_content = store.put_content(&content(
        EngineContentKind::Agent,
        "mira-barkeep",
        "Mira Barkeep",
        json!({"npc_id":"npc.barkeep.mira","role":"bridge-market barkeep"}),
    ))?;
    let lore_content = store.put_content(&content(
        EngineContentKind::Lore,
        "market-hours",
        "Marrow Bridge Market Hours",
        json!({
            "fact_id": "fact.market_hours",
            "text": "Marrow Bridge market closes at dusk.",
            "ward_tags": ["settlement", "trade"]
        }),
    ))?;

    let tags = ollama_get_json("/api/tags")?;
    let tags_readback = run.write_json_artifact("ollama-tags.json", &tags)?;
    let model_meta = model_meta(&tags, MODEL)?;
    let preload = ollama_generate(MODEL, "", None, 120)?;
    let ps_after_preload = ollama_get_json("/api/ps")?;
    let ps_readback = run.write_json_artifact("ollama-ps-after-preload.json", &ps_after_preload)?;
    let hardware = nvidia_smi_readback()?;

    let activation = evaluate_npc_agent_activations(&activation_input(&lore_content))?;
    let decision = activation
        .decisions
        .iter()
        .find(|decision| decision.accepted && decision.intent == NpcAgentIntent::PromptDialogue)
        .ok_or("missing accepted dialogue activation")?;
    let selection = select_agent_model(
        &default_agent_model_registry(),
        &AgentModelSelectionRequest {
            run_id: "issue970-dialogue-selection".to_string(),
            agent_id: "agent.npc.barkeep".to_string(),
            role: AgentModelRole::NpcDialogue,
            required_context_tokens: 2_048,
            concurrent_requests: 1,
            existing_warm_pool_mib: 0,
            prefer_warm_pool: true,
        },
    )?;
    let input = dialogue_input(decision, &selection, &lore_content);
    let prompt = build_local_npc_dialogue_prompt(&input)?;
    let prompt_readback =
        run.write_json_artifact("dialogue-prompt.json", &json!({ "prompt": prompt }))?;
    let generation = ollama_generate(MODEL, &prompt, Some("json"), 180)?;
    let generation_readback = run.write_json_artifact(
        "ollama-generate-response.json",
        &serde_json::to_value(&generation)?,
    )?;
    let ps_after_generate = ollama_get_json("/api/ps")?;
    let ps_generate_readback =
        run.write_json_artifact("ollama-ps-after-generate.json", &ps_after_generate)?;

    let telemetry = LocalNpcDialogueTelemetry {
        model_name: generation.model.clone(),
        model_digest: model_meta.digest.clone(),
        done_reason: generation
            .done_reason
            .clone()
            .unwrap_or_else(|| "stop".to_string()),
        total_duration_ms: nanos_to_ms(generation.total_duration.unwrap_or(0)),
        load_duration_ms: nanos_to_ms(generation.load_duration.unwrap_or(0)),
        prompt_eval_count: generation.prompt_eval_count.unwrap_or(0),
        eval_count: generation.eval_count.unwrap_or(0),
        warm_pool_loaded: running_model_present(&ps_after_preload, MODEL)
            && running_model_present(&ps_after_generate, MODEL),
    };
    let dialogue =
        lower_local_npc_dialogue_response(&input, &prompt, &generation.response, &telemetry)?;
    validate_local_npc_dialogue_readback(&dialogue)?;
    let dialogue_readback = run.write_json_artifact(
        "local-dialogue-readback.json",
        &serde_json::to_value(&dialogue)?,
    )?;
    let edges = edge_cases(&input, &prompt, &telemetry)?;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let accepted = model_meta.present
        && generation.done
        && telemetry.warm_pool_loaded
        && telemetry.total_duration_ms <= 5_000
        && dialogue.accepted
        && dialogue.dialogue.contains("Marrow Bridge")
        && dialogue
            .dialogue
            .to_ascii_lowercase()
            .contains("market closes")
        && dialogue.dialogue.to_ascii_lowercase().contains("dusk")
        && dialogue.hot_path_policy == AI_LOCAL_NPC_DIALOGUE_HOT_PATH_POLICY
        && !dialogue.calyx_live_query_allowed_in_hot_path
        && activation.accepted_count == 1
        && all_edges_matched;
    if !accepted {
        return Err("issue970 local dialogue FSV acceptance failed".into());
    }

    let report = json!({
        "schema": "litd.engine.issue970.local_dialogue_fsv.v1",
        "issue": 970,
        "source_of_truth": {
            "vault_dir": vault_dir.display().to_string(),
            "ollama_tags": tags_readback.path.display().to_string(),
            "ollama_ps_after_preload": ps_readback.path.display().to_string(),
            "ollama_ps_after_generate": ps_generate_readback.path.display().to_string(),
            "prompt": prompt_readback.path.display().to_string(),
            "generate_response": generation_readback.path.display().to_string(),
            "dialogue_readback": dialogue_readback.path.display().to_string(),
            "manual_gate": "manual Synapse/browser readback must inspect the persisted report, Calyx rows, Ollama tags/ps, generated response, dialogue readback, hashes, latency, and edges"
        },
        "research_basis": [
            "Ollama exposes a local API at http://localhost:11434/api and /api/generate accepts stream=false, format=json, and keep_alive.",
            "Ollama documents keep_alive preloading for warm model residency through /api/generate and /api/chat.",
            "Ollama library lists gemma3:4b-it-q4_K_M as a 4B Q4_K_M Gemma model."
        ],
        "hardware": hardware,
        "calyx": {
            "agent": agent_content,
            "lore": lore_content
        },
        "ollama": {
            "version_model": MODEL,
            "model_meta": model_meta,
            "preload": preload,
            "warm_loaded_after_preload": running_model_present(&ps_after_preload, MODEL),
            "warm_loaded_after_generate": running_model_present(&ps_after_generate, MODEL)
        },
        "activation": activation,
        "model_selection": selection,
        "dialogue": dialogue,
        "acceptance": {
            "accepted": accepted,
            "model_present": model_meta.present,
            "generation_done": generation.done,
            "warm_pool_loaded": telemetry.warm_pool_loaded,
            "warm_latency_ms": telemetry.total_duration_ms,
            "warm_latency_under_5000_ms": telemetry.total_duration_ms <= 5000,
            "dialogue_mentions_marrow_bridge": dialogue.dialogue.contains("Marrow Bridge"),
            "dialogue_mentions_market_closes": dialogue.dialogue.to_ascii_lowercase().contains("market closes"),
            "dialogue_mentions_dusk": dialogue.dialogue.to_ascii_lowercase().contains("dusk"),
            "hot_path_policy": dialogue.hot_path_policy,
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let report_readback = run.write_report(&report)?;
    println!(
        "ISSUE970_LOCAL_DIALOGUE_FSV report={} sha256={} model={} digest={} latency_ms={} accepted={} dialogue_sha256={} edges={}",
        report_readback.path.display(),
        report_readback.sha256,
        MODEL,
        model_meta.digest,
        telemetry.total_duration_ms,
        accepted,
        dialogue_readback.sha256,
        edges.len()
    );
    Ok(())
}

fn dialogue_input(
    decision: &litd_engine::ai::NpcAgentActivationDecision,
    selection: &litd_engine::ai::AgentModelSelectionReadback,
    lore: &litd_engine::calyx_content::EngineContentConstellationReadback,
) -> LocalNpcDialogueInput {
    LocalNpcDialogueInput {
        run_id: "issue970-real-gemma".to_string(),
        prompt_request_id: decision.prompt_request_id.clone().expect("accepted prompt"),
        npc_id: "npc.barkeep.mira".to_string(),
        display_name: "Mira".to_string(),
        world_id: "world.ashen_reach".to_string(),
        prompt_profile_id: "profile.mira_market".to_string(),
        role_profile: "practical bridge-market barkeep".to_string(),
        player_utterance: "When does the market close?".to_string(),
        signal_summary: "player asks about local market hours".to_string(),
        ward_tags: vec!["settlement".to_string(), "trade".to_string()],
        allowed_intents: vec![
            LocalNpcDialogueIntent::ShareGroundedFact,
            LocalNpcDialogueIntent::RefuseOutOfDistribution,
        ],
        grounding_facts: vec![CalyxDialogueFact {
            fact_id: "fact.market_hours".to_string(),
            text: "Marrow Bridge market closes at dusk.".to_string(),
            ward_tags: vec!["settlement".to_string(), "trade".to_string()],
            provenance_ref: format!("calyx://engine-content/lore/{}", lore.id),
            required_dialogue_terms: vec![
                "Marrow Bridge".to_string(),
                "market closes".to_string(),
                "dusk".to_string(),
            ],
        }],
        blocked_terms: vec!["spaceship".to_string(), "nuclear".to_string()],
        model_selection: selection.clone(),
        serving_model: MODEL.to_string(),
        serving_endpoint: ENDPOINT.to_string(),
        max_dialogue_chars: 160,
    }
}

fn activation_input(
    lore: &litd_engine::calyx_content::EngineContentConstellationReadback,
) -> litd_engine::ai::NpcAgentActivationInput {
    litd_engine::ai::NpcAgentActivationInput {
        runtime_id: "issue970-runtime".to_string(),
        catalog_id: "issue970-npc-catalog".to_string(),
        rules: NpcAgentRules::living_world_defaults(),
        npcs: vec![NpcAgentDefinition {
            npc_id: "npc.barkeep.mira".to_string(),
            display_name: "Mira".to_string(),
            body_id: SimBodyId::from_raw(42),
            default_agent_id: 970,
            prompt_profile_id: "profile.mira_market".to_string(),
            ward_tags: vec!["settlement".to_string(), "trade".to_string()],
            activation_tags: vec!["settlement".to_string(), "trade".to_string()],
            allowed_intents: vec![NpcAgentIntent::PromptDialogue],
            order_velocity: FixedVec3::ZERO,
        }],
        states: vec![NpcAgentRuntimeState {
            npc_id: "npc.barkeep.mira".to_string(),
            spawned: true,
            last_activation_tick: None,
        }],
        signals: vec![NpcWorldSignal {
            signal_id: "signal.market-question".to_string(),
            world_id: "world.ashen_reach".to_string(),
            tick: 970,
            kind: NpcAgentSignalKind::DialogueOpportunity,
            strength_milli: 950,
            relevance_milli: 980,
            tags: vec!["settlement".to_string(), "trade".to_string()],
            summary: "player asks about local market hours".to_string(),
            target_npc_id: Some("npc.barkeep.mira".to_string()),
            target_body: Some(SimBodyId::from_raw(42)),
            provenance_refs: vec![format!("calyx://engine-content/lore/{}", lore.id)],
        }],
    }
}

fn edge_cases(
    input: &LocalNpcDialogueInput,
    prompt: &str,
    telemetry: &LocalNpcDialogueTelemetry,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let before = input_hash(input)?;
    let mut readback = lower_local_npc_dialogue_response(
        input,
        prompt,
        r#"{"dialogue":"Dusk, dearie. The Marrow Bridge market closes with the last light.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.market_hours"],"ward_tags":["settlement","trade"]}"#,
        telemetry,
    )?;
    readback.readback_fingerprint = "0000".to_string();
    Ok(vec![
        edge(
            "unknown_fact",
            lower_local_npc_dialogue_response(
                input,
                prompt,
                r#"{"dialogue":"Dusk at Marrow Bridge.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.unknown"],"ward_tags":["settlement","trade"]}"#,
                telemetry,
            ),
            "model_response.grounded_fact_ids",
            "not present",
            &before,
            input,
        )?,
        edge(
            "blocked_term",
            lower_local_npc_dialogue_response(
                input,
                prompt,
                r#"{"dialogue":"The spaceship leaves Marrow Bridge at dusk.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.market_hours"],"ward_tags":["settlement","trade"]}"#,
                telemetry,
            ),
            "model_response.dialogue",
            "blocked",
            &before,
            input,
        )?,
        edge(
            "ward_escape",
            lower_local_npc_dialogue_response(
                input,
                prompt,
                r#"{"dialogue":"Dusk at Marrow Bridge.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.market_hours"],"ward_tags":["settlement","nuclear"]}"#,
                telemetry,
            ),
            "model_response.ward_tags",
            "outside",
            &before,
            input,
        )?,
        edge(
            "tampered_readback",
            validate_local_npc_dialogue_readback(&readback),
            "readback_fingerprint",
            "mismatch",
            &before,
            input,
        )?,
    ])
}

fn edge<T>(
    label: &str,
    result: Result<T, EngineError>,
    expected_field: &str,
    expected_reason: &str,
    before_hash: &str,
    input: &LocalNpcDialogueInput,
) -> Result<Value, Box<dyn std::error::Error>> {
    let after_hash = input_hash(input)?;
    let state_unchanged = before_hash == after_hash;
    let value = match result {
        Ok(_) => json!({"label": label, "matched": false, "error": "unexpected success"}),
        Err(EngineError::AiInvalidConfig { field, reason }) => json!({
            "label": label,
            "matched": field.ends_with(expected_field)
                && reason.contains(expected_reason)
                && state_unchanged,
            "field": field,
            "reason": reason,
            "state_unchanged": state_unchanged
        }),
        Err(EngineError::AiReadbackMismatch {
            field,
            expected,
            actual,
        }) => json!({
            "label": label,
            "matched": field.ends_with(expected_field) && state_unchanged,
            "field": field,
            "expected": expected,
            "actual": actual,
            "state_unchanged": state_unchanged
        }),
        Err(error) => json!({"label": label, "matched": false, "error": error.to_string()}),
    };
    Ok(value)
}
