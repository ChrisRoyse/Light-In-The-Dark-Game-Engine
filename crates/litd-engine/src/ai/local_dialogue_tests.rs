use crate::ai::{
    AgentModelRole, AgentModelSelectionRequest, CalyxDialogueFact, LocalNpcDialogueInput,
    LocalNpcDialogueIntent, LocalNpcDialogueTelemetry, build_local_npc_dialogue_prompt,
    default_agent_model_registry, lower_local_npc_dialogue_response, select_agent_model,
    validate_local_npc_dialogue_readback,
};

#[test]
fn issue970_lowers_grounded_local_dialogue() {
    let input = input_fixture();
    let prompt = build_local_npc_dialogue_prompt(&input).unwrap();
    assert!(prompt.contains("fact.market_hours"));

    let readback = lower_local_npc_dialogue_response(
        &input,
        &prompt,
        r#"{"dialogue":"Dusk, dearie. The Marrow Bridge market closes with the last light.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.market_hours"],"ward_tags":["settlement","trade"]}"#,
        &telemetry_fixture(),
    )
    .unwrap();

    assert!(readback.accepted);
    assert_eq!(readback.intent, LocalNpcDialogueIntent::ShareGroundedFact);
    assert_eq!(readback.grounded_fact_ids, ["fact.market_hours"]);
    assert_eq!(
        readback.provenance_refs,
        ["calyx://engine-content/lore/market-hours"]
    );
    assert_eq!(readback.lowered_order.source, "lowered_local_llm");
    assert!(readback.lowered_order.deterministic);
    validate_local_npc_dialogue_readback(&readback).unwrap();
}

#[test]
fn issue970_fails_closed_for_ood_or_tampered_dialogue() {
    let input = input_fixture();
    let prompt = build_local_npc_dialogue_prompt(&input).unwrap();

    expect_invalid(
        lower_local_npc_dialogue_response(
            &input,
            &prompt,
            r#"{"dialogue":"The spaceship docks by dusk at Marrow Bridge.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.market_hours"],"ward_tags":["settlement","trade"]}"#,
            &telemetry_fixture(),
        ),
        "model_response.dialogue",
        "blocked",
    );
    expect_invalid(
        lower_local_npc_dialogue_response(
            &input,
            &prompt,
            r#"{"dialogue":"Dusk at Marrow Bridge.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.unknown"],"ward_tags":["settlement","trade"]}"#,
            &telemetry_fixture(),
        ),
        "model_response.grounded_fact_ids",
        "not present",
    );
    expect_invalid(
        lower_local_npc_dialogue_response(
            &input,
            &prompt,
            r#"{"dialogue":"Dusk at Marrow Bridge.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.market_hours"],"ward_tags":["settlement","nuclear"]}"#,
            &telemetry_fixture(),
        ),
        "model_response.ward_tags",
        "outside",
    );

    let mut readback = lower_local_npc_dialogue_response(
        &input,
        &prompt,
        r#"{"dialogue":"Dusk, dearie. The Marrow Bridge market closes with the last light.","intent":"share_grounded_fact","voice":"in_character","grounded_fact_ids":["fact.market_hours"],"ward_tags":["settlement","trade"]}"#,
        &telemetry_fixture(),
    )
    .unwrap();
    readback.readback_fingerprint = "0000".to_string();
    match validate_local_npc_dialogue_readback(&readback) {
        Err(crate::EngineError::AiReadbackMismatch { field, .. }) => {
            assert_eq!(field, "local_dialogue.readback_fingerprint");
        }
        other => panic!("expected readback mismatch, got {other:?}"),
    }
}

fn input_fixture() -> LocalNpcDialogueInput {
    let registry = default_agent_model_registry();
    let selection = select_agent_model(
        &registry,
        &AgentModelSelectionRequest {
            run_id: "issue970-test".to_string(),
            agent_id: "agent.npc.barkeep".to_string(),
            role: AgentModelRole::NpcDialogue,
            required_context_tokens: 2_048,
            concurrent_requests: 1,
            existing_warm_pool_mib: 0,
            prefer_warm_pool: true,
        },
    )
    .unwrap();
    LocalNpcDialogueInput {
        run_id: "issue970-test".to_string(),
        prompt_request_id: "prompt.issue970.barkeep.market".to_string(),
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
            provenance_ref: "calyx://engine-content/lore/market-hours".to_string(),
            required_dialogue_terms: vec![
                "Marrow Bridge".to_string(),
                "market closes".to_string(),
                "dusk".to_string(),
            ],
        }],
        blocked_terms: vec!["spaceship".to_string(), "nuclear".to_string()],
        model_selection: selection,
        serving_model: "gemma3:4b-it-q4_K_M".to_string(),
        serving_endpoint: "http://127.0.0.1:11434/api/generate".to_string(),
        max_dialogue_chars: 160,
    }
}

fn telemetry_fixture() -> LocalNpcDialogueTelemetry {
    LocalNpcDialogueTelemetry {
        model_name: "gemma3:4b-it-q4_K_M".to_string(),
        model_digest: "a2af6cc3eb7fa8be8504abaf9b04e88f17a119ec3f04a3addf55f92841195f5a"
            .to_string(),
        done_reason: "stop".to_string(),
        total_duration_ms: 700,
        load_duration_ms: 300,
        prompt_eval_count: 160,
        eval_count: 68,
        warm_pool_loaded: true,
    }
}

fn expect_invalid<T: std::fmt::Debug>(
    result: Result<T, crate::EngineError>,
    field: &str,
    reason: &str,
) {
    match result {
        Err(crate::EngineError::AiInvalidConfig {
            field: actual_field,
            reason: actual_reason,
        }) => {
            assert_eq!(actual_field, format!("local_dialogue.{field}"));
            assert!(
                actual_reason.contains(reason),
                "expected {actual_reason} to contain {reason}"
            );
        }
        other => panic!("expected AiInvalidConfig, got {other:?}"),
    }
}
