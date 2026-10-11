use crate::{
    EngineError,
    ai::{
        AI_NPC_AGENT_HOT_PATH_POLICY, NpcAgentActivationGrade, NpcAgentActivationInput,
        NpcAgentDefinition, NpcAgentIntent, NpcAgentRules, NpcAgentRuntimeState,
        NpcAgentSignalKind, NpcWorldSignal, evaluate_npc_agent_activations,
        verify_npc_agent_activation_integrity,
    },
    math::FixedVec3,
    sim::SimBodyId,
    sim_order::SimOrderSourceKind,
};

#[test]
fn issue967_all_npcs_have_default_agents_and_lowered_orders() {
    let input = issue967_input();
    let readback = evaluate_npc_agent_activations(&input).unwrap();

    assert_eq!(readback.npc_count, 4);
    assert_eq!(readback.default_agent_count, 4);
    assert_eq!(readback.signal_count, 5);
    assert_eq!(readback.accepted_count, 4);
    assert_eq!(readback.skipped_count, 1);
    assert_eq!(readback.prompt_count, 4);
    assert_eq!(readback.order_count, 4);
    assert_eq!(readback.spawn_request_count, 1);
    assert_eq!(readback.hot_path_policy, AI_NPC_AGENT_HOT_PATH_POLICY);
    assert!(!readback.calyx_live_query_allowed_in_hot_path);
    assert!(readback.orders.iter().all(|order| {
        order.source.kind == SimOrderSourceKind::LoweredAgent
            && order.fingerprint() != 0
            && order.execute_tick > 40
    }));
    assert_eq!(
        readback
            .decisions
            .iter()
            .find(|decision| decision.signal_id == "signal.tavern.schedule")
            .and_then(|decision| decision.npc_id.as_deref()),
        Some("npc.barkeep")
    );
    assert_eq!(
        readback
            .decisions
            .iter()
            .find(|decision| decision.signal_id == "signal.scout.help")
            .map(|decision| decision.spawn_requested),
        Some(true)
    );
    assert_eq!(
        readback.decisions.last().map(|decision| decision.grade),
        Some(NpcAgentActivationGrade::CooldownActive)
    );
    verify_npc_agent_activation_integrity(&readback).unwrap();
}

#[test]
fn issue967_skips_weak_ward_and_missing_candidate_without_effects() {
    let mut input = issue967_input();
    input.signals = vec![
        signal(SignalArgs {
            signal_id: "signal.weak.enemy",
            tick: 1,
            kind: NpcAgentSignalKind::EnemyInRange,
            strength_milli: 100,
            relevance_milli: 900,
            tags: &["combat", "gate", "threat"],
            target_npc_id: Some("npc.guard"),
            summary: "weak enemy signal",
        }),
        signal(SignalArgs {
            signal_id: "signal.ward.block",
            tick: 2,
            kind: NpcAgentSignalKind::DialogueOpportunity,
            strength_milli: 900,
            relevance_milli: 900,
            tags: &["combat", "gate", "threat"],
            target_npc_id: Some("npc.elder"),
            summary: "elder receives combat signal outside Ward",
        }),
        signal(SignalArgs {
            signal_id: "signal.no.candidate",
            tick: 3,
            kind: NpcAgentSignalKind::WorldEvent,
            strength_milli: 900,
            relevance_milli: 900,
            tags: &["ocean", "trade"],
            target_npc_id: None,
            summary: "distant trade fleet arrives",
        }),
    ];

    let readback = evaluate_npc_agent_activations(&input).unwrap();
    assert_eq!(readback.accepted_count, 0);
    assert_eq!(readback.order_count, 0);
    assert_eq!(readback.prompt_count, 0);
    assert_eq!(
        readback
            .decisions
            .iter()
            .map(|decision| decision.grade)
            .collect::<Vec<_>>(),
        vec![
            NpcAgentActivationGrade::SignalTooWeak,
            NpcAgentActivationGrade::WardBlocked,
            NpcAgentActivationGrade::NoCandidate,
        ]
    );
    assert!(readback.decisions.iter().all(|decision| {
        !decision.accepted
            && decision.prompt_request_id.is_none()
            && decision.lowered_order.is_none()
    }));
}

#[test]
fn issue967_invalid_inputs_fail_loud() {
    let mut duplicate = issue967_input();
    duplicate.npcs[1].npc_id = duplicate.npcs[0].npc_id.clone();
    expect_ai_invalid(
        evaluate_npc_agent_activations(&duplicate),
        "npc_agents.npc_id",
        "duplicate",
    );

    let mut zero_agent = issue967_input();
    zero_agent.npcs[0].default_agent_id = 0;
    expect_ai_invalid(
        evaluate_npc_agent_activations(&zero_agent),
        "npc_agents.default_agent_id",
        "non-zero",
    );

    let mut missing_state = issue967_input();
    missing_state.states.pop();
    expect_ai_invalid(
        evaluate_npc_agent_activations(&missing_state),
        "npc_agents.states",
        "one state",
    );

    let mut unknown_target = issue967_input();
    unknown_target.signals[0].target_npc_id = Some("npc.missing".to_string());
    expect_ai_invalid(
        evaluate_npc_agent_activations(&unknown_target),
        "npc_agents.signal.target_npc_id",
        "must exist",
    );
}

pub(crate) fn issue967_input() -> NpcAgentActivationInput {
    NpcAgentActivationInput {
        runtime_id: "issue967-runtime".to_string(),
        catalog_id: "ashen-reach-npcs".to_string(),
        rules: NpcAgentRules::living_world_defaults(),
        npcs: vec![
            npc(NpcArgs {
                npc_id: "npc.guard",
                display_name: "North Gate Guard",
                body_id: 10,
                default_agent_id: 9671,
                prompt_profile_id: "profile.guard",
                ward_tags: &["npc", "combat", "gate", "threat", "guard"],
                activation_tags: &["combat", "gate", "threat"],
                allowed_intents: &[
                    NpcAgentIntent::RunController,
                    NpcAgentIntent::PromptReaction,
                ],
                order_velocity: FixedVec3::from_i32s(1, 0, 0),
            }),
            npc(NpcArgs {
                npc_id: "npc.elder",
                display_name: "Elder Cairn",
                body_id: 11,
                default_agent_id: 9672,
                prompt_profile_id: "profile.elder",
                ward_tags: &["npc", "dialogue", "lore", "ruins"],
                activation_tags: &["dialogue", "lore"],
                allowed_intents: &[NpcAgentIntent::PromptDialogue, NpcAgentIntent::PromptEvent],
                order_velocity: FixedVec3::ZERO,
            }),
            npc(NpcArgs {
                npc_id: "npc.barkeep",
                display_name: "Golden Hearth Barkeep",
                body_id: 12,
                default_agent_id: 9673,
                prompt_profile_id: "profile.barkeep",
                ward_tags: &["npc", "social", "tavern", "schedule"],
                activation_tags: &["social", "tavern", "schedule"],
                allowed_intents: &[
                    NpcAgentIntent::PromptDialogue,
                    NpcAgentIntent::PromptRoutine,
                    NpcAgentIntent::PromptEvent,
                ],
                order_velocity: FixedVec3::ZERO,
            }),
            npc(NpcArgs {
                npc_id: "npc.scout",
                display_name: "Thornwild Scout",
                body_id: 13,
                default_agent_id: 9674,
                prompt_profile_id: "profile.scout",
                ward_tags: &["npc", "wilderness", "help", "scout"],
                activation_tags: &["wilderness", "help", "scout"],
                allowed_intents: &[
                    NpcAgentIntent::RunController,
                    NpcAgentIntent::PromptReaction,
                    NpcAgentIntent::PromptEvent,
                ],
                order_velocity: FixedVec3::from_i32s(0, 0, 1),
            }),
        ],
        states: vec![
            state("npc.guard", true),
            state("npc.elder", true),
            state("npc.barkeep", true),
            state("npc.scout", false),
        ],
        signals: vec![
            signal(SignalArgs {
                signal_id: "signal.guard.enemy",
                tick: 40,
                kind: NpcAgentSignalKind::EnemyInRange,
                strength_milli: 950,
                relevance_milli: 920,
                tags: &["combat", "gate", "threat"],
                target_npc_id: Some("npc.guard"),
                summary: "raiders enter the north gate range",
            }),
            signal(SignalArgs {
                signal_id: "signal.elder.dialogue",
                tick: 41,
                kind: NpcAgentSignalKind::DialogueOpportunity,
                strength_milli: 850,
                relevance_milli: 875,
                tags: &["dialogue", "lore"],
                target_npc_id: Some("npc.elder"),
                summary: "player asks about the ancient pact",
            }),
            signal(SignalArgs {
                signal_id: "signal.tavern.schedule",
                tick: 42,
                kind: NpcAgentSignalKind::ScheduleTick,
                strength_milli: 800,
                relevance_milli: 820,
                tags: &["social", "tavern", "schedule"],
                target_npc_id: None,
                summary: "evening tavern service begins",
            }),
            signal(SignalArgs {
                signal_id: "signal.scout.help",
                tick: 43,
                kind: NpcAgentSignalKind::HelpRequest,
                strength_milli: 900,
                relevance_milli: 880,
                tags: &["wilderness", "help", "scout"],
                target_npc_id: None,
                summary: "lost caravan requests a guide",
            }),
            signal(SignalArgs {
                signal_id: "signal.guard.cooldown",
                tick: 44,
                kind: NpcAgentSignalKind::EnemyInRange,
                strength_milli: 950,
                relevance_milli: 900,
                tags: &["combat", "gate", "threat"],
                target_npc_id: Some("npc.guard"),
                summary: "second guard threat arrives inside cooldown",
            }),
        ],
    }
}

struct NpcArgs<'a> {
    npc_id: &'a str,
    display_name: &'a str,
    body_id: u32,
    default_agent_id: u32,
    prompt_profile_id: &'a str,
    ward_tags: &'a [&'a str],
    activation_tags: &'a [&'a str],
    allowed_intents: &'a [NpcAgentIntent],
    order_velocity: FixedVec3,
}

fn npc(args: NpcArgs<'_>) -> NpcAgentDefinition {
    let NpcArgs {
        npc_id,
        display_name,
        body_id,
        default_agent_id,
        prompt_profile_id,
        ward_tags,
        activation_tags,
        allowed_intents,
        order_velocity,
    } = args;
    NpcAgentDefinition {
        npc_id: npc_id.to_string(),
        display_name: display_name.to_string(),
        body_id: SimBodyId::from_raw(body_id),
        default_agent_id,
        prompt_profile_id: prompt_profile_id.to_string(),
        ward_tags: strings(ward_tags),
        activation_tags: strings(activation_tags),
        allowed_intents: allowed_intents.to_vec(),
        order_velocity,
    }
}

fn state(npc_id: &str, spawned: bool) -> NpcAgentRuntimeState {
    NpcAgentRuntimeState {
        npc_id: npc_id.to_string(),
        spawned,
        last_activation_tick: None,
    }
}

struct SignalArgs<'a> {
    signal_id: &'a str,
    tick: u64,
    kind: NpcAgentSignalKind,
    strength_milli: u16,
    relevance_milli: u16,
    tags: &'a [&'a str],
    target_npc_id: Option<&'a str>,
    summary: &'a str,
}

fn signal(args: SignalArgs<'_>) -> NpcWorldSignal {
    let SignalArgs {
        signal_id,
        tick,
        kind,
        strength_milli,
        relevance_milli,
        tags,
        target_npc_id,
        summary,
    } = args;
    NpcWorldSignal {
        signal_id: signal_id.to_string(),
        world_id: "world.ashen-reach".to_string(),
        tick,
        kind,
        strength_milli,
        relevance_milli,
        tags: strings(tags),
        summary: summary.to_string(),
        target_npc_id: target_npc_id.map(str::to_string),
        target_body: Some(SimBodyId::from_raw(100)),
        provenance_refs: vec![format!("signal://issue967/{signal_id}")],
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn expect_ai_invalid<T: std::fmt::Debug>(
    result: Result<T, EngineError>,
    expected_field: &str,
    expected: &str,
) {
    match result {
        Err(EngineError::AiInvalidConfig { field, reason }) => {
            assert_eq!(field, expected_field);
            assert!(
                reason.contains(expected),
                "expected '{reason}' to contain '{expected}'"
            );
        }
        other => panic!("expected AiInvalidConfig, got {other:?}"),
    }
}
