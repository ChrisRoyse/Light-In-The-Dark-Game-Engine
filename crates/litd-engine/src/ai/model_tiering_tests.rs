use crate::{
    EngineError,
    ai::{
        AI_AGENT_MODEL_HOT_PATH_POLICY, AgentModelRole, AgentModelSelectionRequest,
        default_agent_model_registry, select_agent_model, validate_agent_model_registry,
        validate_agent_model_selection_readback,
    },
};

#[test]
fn issue969_selects_right_sized_models_by_agent_role() {
    let registry = default_agent_model_registry();
    validate_agent_model_registry(&registry).unwrap();

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
    )
    .unwrap();
    assert_eq!(object.selected_model_id, "model.local.tiny_agent");
    assert_eq!(object.reserved_total_mib, 276);
    assert_eq!(object.warm_pool_after_mib, 276);

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
    )
    .unwrap();
    assert_eq!(
        creature.selected_model_id,
        "model.local.small_creature_agent"
    );
    assert_eq!(creature.reserved_total_mib, 804);

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
    )
    .unwrap();
    assert_eq!(dialogue.selected_model_id, "model.local.larger_agent");
    assert_eq!(dialogue.reserved_total_mib, 7_168);
    assert_eq!(dialogue.hot_path_policy, AI_AGENT_MODEL_HOT_PATH_POLICY);
    assert!(!dialogue.calyx_live_query_allowed_in_hot_path);
    validate_agent_model_selection_readback(&dialogue).unwrap();
}

#[test]
fn issue969_edges_fail_closed_for_bad_registry_and_requests() {
    let registry = default_agent_model_registry();

    let mut duplicate = registry.clone();
    duplicate.models[1].model_id = duplicate.models[0].model_id.clone();
    expect_invalid(
        validate_agent_model_registry(&duplicate),
        "agent_model.registry.model_id",
        "duplicate",
    );

    let mut nondeterministic = registry.clone();
    nondeterministic.models[0].deterministic_lowering_only = false;
    expect_invalid(
        validate_agent_model_registry(&nondeterministic),
        "agent_model.model.deterministic_lowering_only",
        "deterministic",
    );

    expect_invalid(
        select_agent_model(
            &registry,
            &request(
                "issue969-context-edge",
                "agent.object.long-context",
                AgentModelRole::ObjectVisual,
                2_048,
                1,
                0,
            ),
        ),
        "agent_model.selection",
        "context",
    );

    expect_invalid(
        select_agent_model(
            &registry,
            &request(
                "issue969-warm-edge",
                "agent.npc.crowd",
                AgentModelRole::NpcDialogue,
                4_096,
                8,
                19_000,
            ),
        ),
        "agent_model.selection",
        "warm pool",
    );

    let mut tampered = select_agent_model(
        &registry,
        &request(
            "issue969-tamper-edge",
            "agent.npc.tampered",
            AgentModelRole::NpcDialogue,
            4_096,
            1,
            0,
        ),
    )
    .unwrap();
    tampered.selection_fingerprint = "0000".to_string();
    expect_invalid(
        validate_agent_model_selection_readback(&tampered),
        "agent_model.selection.selection_fingerprint",
        "fingerprint mismatch",
    );
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

fn expect_invalid<T: std::fmt::Debug>(result: Result<T, EngineError>, field: &str, reason: &str) {
    match result {
        Err(EngineError::AgentModelTieringInvalidConfig {
            field: actual_field,
            reason: actual_reason,
        }) => {
            assert_eq!(actual_field, field);
            assert!(
                actual_reason.contains(reason),
                "expected reason containing {reason}, got {actual_reason}"
            );
        }
        other => panic!("expected AgentModelTieringInvalidConfig, got {other:?}"),
    }
}
