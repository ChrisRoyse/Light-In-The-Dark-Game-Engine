use crate::{
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_world_activation::{
        ENGINE_CALYX_WORLD_ACTIVATION_HOT_PATH_POLICY, EngineWorldActivationAgentKind,
        EngineWorldActivationError, EngineWorldActivationGrade, EngineWorldActivationInput,
        EngineWorldActivationIntent, EngineWorldActivationPolicy, EngineWorldAgentProfile,
        EngineWorldAgentState, EngineWorldFact, EngineWorldFactKind,
        evaluate_calyx_world_activation, read_calyx_world_activation, store_calyx_world_activation,
        validate_world_activation_readback,
    },
    math::FixedVec3,
    sim::SimBodyId,
    sim_order::SimOrderSourceKind,
};

#[test]
fn issue968_ingests_world_facts_associates_agents_and_persists_ledger() {
    let (database, _vault_dir) = database("happy");
    let input = issue968_input();
    let readback = evaluate_calyx_world_activation(&input).unwrap();
    let stored = store_calyx_world_activation(&database, &readback).unwrap();
    let disk = read_calyx_world_activation(&database, "issue968-world-activation").unwrap();

    assert_eq!(readback.fact_count, 5);
    assert_eq!(readback.fact_kind_count, 4);
    assert_eq!(readback.agent_count, 5);
    assert_eq!(readback.association_count, 4);
    assert_eq!(readback.decision_count, 5);
    assert_eq!(readback.accepted_count, 3);
    assert_eq!(readback.skipped_count, 2);
    assert_eq!(readback.prompt_count, 3);
    assert_eq!(readback.order_count, 3);
    assert_eq!(readback.spawn_request_count, 1);
    assert_eq!(
        readback.hot_path_policy,
        ENGINE_CALYX_WORLD_ACTIVATION_HOT_PATH_POLICY
    );
    assert!(!readback.calyx_live_query_allowed_in_hot_path);
    assert!(readback.orders.iter().all(|order| {
        order.source.kind == SimOrderSourceKind::LoweredAgent && order.fingerprint() != 0
    }));
    assert_eq!(
        grade(&readback, "agent.rift"),
        EngineWorldActivationGrade::WardBlocked
    );
    assert_eq!(
        grade(&readback, "agent.elder"),
        EngineWorldActivationGrade::NoAssociation
    );
    assert_eq!(
        readback
            .decisions
            .iter()
            .find(|decision| decision.agent_id == "agent.scout")
            .map(|decision| decision.spawn_requested),
        Some(true)
    );
    assert_eq!(
        stored.activation.activation_fingerprint,
        readback.activation_fingerprint
    );
    assert_eq!(
        disk.activation.activation_fingerprint,
        readback.activation_fingerprint
    );
    validate_world_activation_readback(&readback).unwrap();
}

#[test]
fn issue968_skips_low_support_and_cooldown_without_effects() {
    let mut low_support = issue968_input();
    low_support.policy.min_support_milli = 950;
    let low = evaluate_calyx_world_activation(&low_support).unwrap();
    assert_eq!(
        grade(&low, "agent.guard"),
        EngineWorldActivationGrade::InsufficientSupport
    );

    let mut cooldown = issue968_input();
    cooldown.states[0].last_activation_tick = Some(39);
    let cooled = evaluate_calyx_world_activation(&cooldown).unwrap();
    let guard = cooled
        .decisions
        .iter()
        .find(|decision| decision.agent_id == "agent.guard")
        .unwrap();
    assert_eq!(guard.grade, EngineWorldActivationGrade::CooldownActive);
    assert!(!guard.accepted);
    assert!(guard.lowered_order.is_none());
    assert!(guard.prompt.is_none());
}

#[test]
fn issue1484_weak_measurement_reduces_support_below_authored_weight() {
    // A fact the measurement calls weak (measure 10) must not activate an agent
    // just because its authored weight (800) clears the 500 support floor. Before
    // #1484-4 support was floored at the authored weight (silent fail-open vs the
    // measurement); the measured strength is now authoritative.
    let mut weak = issue968_input();
    let supply = weak
        .facts
        .iter_mut()
        .find(|fact| fact.fact_id == "fact.measure.supply")
        .expect("barkeep supply fact present");
    supply.weight_milli = 800;
    supply.measure_milli = Some(10);
    let readback = evaluate_calyx_world_activation(&weak).unwrap();
    assert_eq!(
        grade(&readback, "agent.barkeep"),
        EngineWorldActivationGrade::InsufficientSupport
    );

    // Control: with no measurement the authored weight (800) still supports
    // activation, so the change only bites when a measurement is actually present.
    let mut authored = issue968_input();
    let supply = authored
        .facts
        .iter_mut()
        .find(|fact| fact.fact_id == "fact.measure.supply")
        .expect("barkeep supply fact present");
    supply.weight_milli = 800;
    supply.measure_milli = None;
    let readback = evaluate_calyx_world_activation(&authored).unwrap();
    assert_ne!(
        grade(&readback, "agent.barkeep"),
        EngineWorldActivationGrade::InsufficientSupport
    );
}

#[test]
fn issue968_invalid_inputs_fail_loud() {
    let mut duplicate = issue968_input();
    duplicate.facts[1].fact_id = duplicate.facts[0].fact_id.clone();
    expect_invalid(
        evaluate_calyx_world_activation(&duplicate),
        "world_activation.fact_id",
        "duplicate",
    );

    let mut zero_agent = issue968_input();
    zero_agent.agents[0].lowered_agent_id = 0;
    expect_invalid(
        evaluate_calyx_world_activation(&zero_agent),
        "world_activation.lowered_agent_id",
        "non-zero",
    );

    let mut missing_state = issue968_input();
    missing_state.states.pop();
    expect_invalid(
        evaluate_calyx_world_activation(&missing_state),
        "world_activation.states",
        "one state",
    );

    let mut unknown_target = issue968_input();
    unknown_target.facts[0].target_agent_id = Some("agent.missing".to_string());
    expect_invalid(
        evaluate_calyx_world_activation(&unknown_target),
        "world_activation.fact.target_agent_id",
        "must exist",
    );

    let mut tampered = evaluate_calyx_world_activation(&issue968_input()).unwrap();
    tampered.activation_fingerprint = "00000000".to_string();
    match validate_world_activation_readback(&tampered) {
        Err(EngineWorldActivationError::ReadbackMismatch { field, .. }) => {
            assert_eq!(field, "world_activation.activation_fingerprint");
        }
        other => panic!("expected readback mismatch, got {other:?}"),
    }
}

fn issue968_input() -> EngineWorldActivationInput {
    EngineWorldActivationInput {
        run_id: "issue968-world-activation".to_string(),
        world_id: "world.ashen-reach".to_string(),
        policy: EngineWorldActivationPolicy::living_world_defaults("issue968-policy"),
        agents: vec![
            agent(AgentArgs {
                agent_id: "agent.guard",
                display_name: "North Gate Guard",
                lowered_agent_id: 9681,
                body_id: 20,
                trigger_tags: &["combat", "gate", "threat"],
                ward_tags: &["combat", "gate", "threat"],
                intent: EngineWorldActivationIntent::RunController,
                order_velocity: FixedVec3::from_i32s(1, 0, 0),
            }),
            agent(AgentArgs {
                agent_id: "agent.barkeep",
                display_name: "Golden Hearth Barkeep",
                lowered_agent_id: 9682,
                body_id: 21,
                trigger_tags: &["economy", "supply", "tavern"],
                ward_tags: &["economy", "supply", "tavern"],
                intent: EngineWorldActivationIntent::PromptRoutine,
                order_velocity: FixedVec3::ZERO,
            }),
            agent(AgentArgs {
                agent_id: "agent.scout",
                display_name: "Thornwild Scout",
                lowered_agent_id: 9683,
                body_id: 22,
                trigger_tags: &["wilderness", "help", "scout"],
                ward_tags: &["wilderness", "help", "scout"],
                intent: EngineWorldActivationIntent::SpawnAgent,
                order_velocity: FixedVec3::from_i32s(0, 0, 1),
            }),
            agent(AgentArgs {
                agent_id: "agent.rift",
                display_name: "Rift Watcher",
                lowered_agent_id: 9684,
                body_id: 23,
                trigger_tags: &["rift", "teleport"],
                ward_tags: &["rift", "visual"],
                intent: EngineWorldActivationIntent::PromptEvent,
                order_velocity: FixedVec3::ZERO,
            }),
            agent(AgentArgs {
                agent_id: "agent.elder",
                display_name: "Elder Cairn",
                lowered_agent_id: 9685,
                body_id: 24,
                trigger_tags: &["lore", "dialogue"],
                ward_tags: &["lore", "dialogue"],
                intent: EngineWorldActivationIntent::PromptDialogue,
                order_velocity: FixedVec3::ZERO,
            }),
        ],
        states: vec![
            state("agent.guard", true),
            state("agent.barkeep", true),
            state("agent.scout", false),
            state("agent.rift", true),
            state("agent.elder", true),
        ],
        facts: vec![
            fact(FactArgs {
                fact_id: "fact.log.enemy",
                tick: 40,
                kind: EngineWorldFactKind::LogLine,
                weight_milli: 320,
                measure_milli: None,
                tags: &["combat", "gate", "threat"],
                target_agent_id: Some("agent.guard"),
                summary: "log: raiders shouted at the north gate",
            }),
            fact(FactArgs {
                fact_id: "fact.event.player",
                tick: 41,
                kind: EngineWorldFactKind::Event,
                weight_milli: 310,
                measure_milli: None,
                tags: &["combat", "gate", "threat"],
                target_agent_id: Some("agent.guard"),
                summary: "event: player entered the gate combat radius",
            }),
            fact(FactArgs {
                fact_id: "fact.measure.supply",
                tick: 42,
                kind: EngineWorldFactKind::Measure,
                weight_milli: 500,
                measure_milli: Some(880),
                tags: &["economy", "supply", "tavern"],
                target_agent_id: Some("agent.barkeep"),
                summary: "measure: tavern stockpile below evening threshold",
            }),
            fact(FactArgs {
                fact_id: "fact.bit.help",
                tick: 43,
                kind: EngineWorldFactKind::Bit,
                weight_milli: 900,
                measure_milli: None,
                tags: &["wilderness", "help", "scout"],
                target_agent_id: Some("agent.scout"),
                summary: "bit: caravan needs a wilderness guide",
            }),
            fact(FactArgs {
                fact_id: "fact.event.rift",
                tick: 44,
                kind: EngineWorldFactKind::Event,
                weight_milli: 900,
                measure_milli: None,
                tags: &["rift", "teleport", "visual"],
                target_agent_id: Some("agent.rift"),
                summary: "event: rift asks for illegal teleport visual",
            }),
        ],
    }
}

struct AgentArgs<'a> {
    agent_id: &'a str,
    display_name: &'a str,
    lowered_agent_id: u32,
    body_id: u32,
    trigger_tags: &'a [&'a str],
    ward_tags: &'a [&'a str],
    intent: EngineWorldActivationIntent,
    order_velocity: FixedVec3,
}

fn agent(args: AgentArgs<'_>) -> EngineWorldAgentProfile {
    let AgentArgs {
        agent_id,
        display_name,
        lowered_agent_id,
        body_id,
        trigger_tags,
        ward_tags,
        intent,
        order_velocity,
    } = args;
    EngineWorldAgentProfile {
        agent_id: agent_id.to_string(),
        display_name: display_name.to_string(),
        kind: EngineWorldActivationAgentKind::Npc,
        prompt_profile_id: format!("profile.{agent_id}"),
        lowered_agent_id,
        target_body: SimBodyId::from_raw(body_id),
        trigger_tags: strings(trigger_tags),
        ward_tags: strings(ward_tags),
        intent,
        order_velocity,
    }
}

fn state(agent_id: &str, spawned: bool) -> EngineWorldAgentState {
    EngineWorldAgentState {
        agent_id: agent_id.to_string(),
        spawned,
        last_activation_tick: None,
    }
}

struct FactArgs<'a> {
    fact_id: &'a str,
    tick: u64,
    kind: EngineWorldFactKind,
    weight_milli: u16,
    measure_milli: Option<u16>,
    tags: &'a [&'a str],
    target_agent_id: Option<&'a str>,
    summary: &'a str,
}

fn fact(args: FactArgs<'_>) -> EngineWorldFact {
    let FactArgs {
        fact_id,
        tick,
        kind,
        weight_milli,
        measure_milli,
        tags,
        target_agent_id,
        summary,
    } = args;
    EngineWorldFact {
        fact_id: fact_id.to_string(),
        world_id: "world.ashen-reach".to_string(),
        tick,
        kind,
        tags: strings(tags),
        summary: summary.to_string(),
        weight_milli,
        measure_milli,
        target_agent_id: target_agent_id.map(str::to_string),
        provenance_refs: vec![format!("world-fact://issue968/{fact_id}")],
    }
}

fn grade(
    readback: &crate::calyx_world_activation::EngineWorldActivationReadback,
    agent_id: &str,
) -> EngineWorldActivationGrade {
    readback
        .decisions
        .iter()
        .find(|decision| decision.agent_id == agent_id)
        .map(|decision| decision.grade)
        .unwrap()
}

fn expect_invalid<T: std::fmt::Debug>(
    result: Result<T, EngineWorldActivationError>,
    expected_field: &str,
    expected_reason: &str,
) {
    match result {
        Err(EngineWorldActivationError::InvalidInput { field, reason }) => {
            assert_eq!(field, expected_field);
            assert!(reason.contains(expected_reason));
        }
        other => panic!("expected invalid input, got {other:?}"),
    }
}

fn database(label: &str) -> (EngineCalyxDatabase, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    (
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap(),
        vault_dir,
    )
}

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue968-{label}"))
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}
