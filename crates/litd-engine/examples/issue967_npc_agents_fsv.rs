use litd_engine::{
    EngineError,
    ai::{
        NpcAgentActivationGrade, NpcAgentActivationInput, NpcAgentActivationReadback,
        NpcAgentDefinition, NpcAgentIntent, NpcAgentRules, NpcAgentRuntimeState,
        NpcAgentSignalKind, NpcWorldSignal, evaluate_npc_agent_activations,
        verify_npc_agent_activation_integrity,
    },
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    sim::SimBodyId,
    sim_order::SimOrderSourceKind,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue967_npc_agents")?;
    let input = issue967_input();
    let readback = evaluate_npc_agent_activations(&input)?;
    let edges = edge_readbacks(&readback);

    let guard = decision(&readback, "signal.guard.enemy")?;
    let elder = decision(&readback, "signal.elder.dialogue")?;
    let barkeep = decision(&readback, "signal.tavern.schedule")?;
    let scout = decision(&readback, "signal.scout.help")?;
    let cooldown = decision(&readback, "signal.guard.cooldown")?;
    let all_orders_lowered_agent = readback
        .orders
        .iter()
        .all(|order| order.source.kind == SimOrderSourceKind::LoweredAgent);
    let all_order_fingerprints_match = readback.decisions.iter().all(|decision| {
        decision
            .lowered_order
            .map(|order| Some(order.fingerprint()) == decision.order_fingerprint)
            .unwrap_or(decision.order_fingerprint.is_none())
    });
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool() == Some(true));
    let acceptance = json!({
        "npc_count": readback.npc_count,
        "default_agent_count": readback.default_agent_count,
        "signal_count": readback.signal_count,
        "accepted_count": readback.accepted_count,
        "skipped_count": readback.skipped_count,
        "prompt_count": readback.prompt_count,
        "order_count": readback.order_count,
        "spawn_request_count": readback.spawn_request_count,
        "hot_path_policy": readback.hot_path_policy,
        "calyx_live_query_allowed_in_hot_path": readback.calyx_live_query_allowed_in_hot_path,
        "all_orders_lowered_agent": all_orders_lowered_agent,
        "all_order_fingerprints_match": all_order_fingerprints_match,
        "guard_agent_id": guard.default_agent_id,
        "guard_execute_tick": guard.execute_tick,
        "guard_source_kind": guard.lowered_order.map(|order| format!("{:?}", order.source.kind)),
        "elder_prompt_profile": elder.prompt_profile_id,
        "untargeted_signal_selected_npc": barkeep.npc_id,
        "scout_spawn_requested": scout.spawn_requested,
        "cooldown_grade": cooldown.grade,
        "runtime_fnv1a64": readback.fnv1a64,
        "all_edges_matched": all_edges_matched,
        "invalid_config_code": "LITD-AI-INVALID-CONFIG"
    });
    ensure(readback.npc_count == 4, "expected four NPCs")?;
    ensure(
        readback.default_agent_count == 4,
        "all NPCs need default agents",
    )?;
    ensure(
        readback.accepted_count == 4,
        "expected four accepted activations",
    )?;
    ensure(readback.skipped_count == 1, "expected one cooldown skip")?;
    ensure(readback.order_count == 4, "expected four lowered orders")?;
    ensure(scout.spawn_requested, "scout should request spawn")?;
    ensure(
        cooldown.grade == NpcAgentActivationGrade::CooldownActive,
        "cooldown decision did not skip explicitly",
    )?;
    ensure(
        all_orders_lowered_agent,
        "orders were not all lowered-agent sourced",
    )?;
    ensure(
        all_order_fingerprints_match,
        "order fingerprints mismatched",
    )?;
    ensure(all_edges_matched, "edge readbacks did not all match")?;

    let report = json!({
        "schema": "litd.engine.issue967.npc_agents_fsv.v1",
        "schema_version": 1,
        "issue": 967,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "manual_gate": "supporting generated report; manual FSV must inspect this JSON, issue state, prompt/readback rows, lowered orders, source kinds, fingerprints, and edge cases"
        },
        "readme_references_reviewed": [
            "docs/readme/06-living-agents.png",
            "docs/readme/S17-agent-binding.png",
            "docs/readme/S20-living-world.png",
            "docs/readme/GP10-dialogue.png",
            "docs/readme/GP17-tavern.png"
        ],
        "research_sources": [
            {
                "label": "Epic Unreal Behavior Tree and Blackboard docs",
                "url": "https://dev.epicgames.com/documentation/unreal-engine/behavior-tree-in-unreal-engine---user-guide?lang=en-US",
                "used_for": "blackboard-style sensed state drives behavior decisions"
            },
            {
                "label": "Epic Unreal StateTree docs",
                "url": "https://dev.epicgames.com/documentation/unreal-engine/state-tree-in-unreal-engine?lang=en-US",
                "used_for": "signal-driven hierarchical AI state"
            },
            {
                "label": "Epic Unreal MassEntity/MassGameplay docs",
                "url": "https://dev.epicgames.com/documentation/unreal-engine/overview-of-mass-gameplay-in-unreal-engine",
                "used_for": "large-NPC activation, LOD, spawning, and signals"
            },
            {
                "label": "Gaffer deterministic lockstep",
                "url": "https://gafferongames.com/post/deterministic_lockstep/",
                "used_for": "only deterministic inputs enter the replayable sim"
            }
        ],
        "input_summary": {
            "runtime_id": input.runtime_id,
            "catalog_id": input.catalog_id,
            "rules": input.rules,
            "npc_ids": input.npcs.iter().map(|npc| npc.npc_id.clone()).collect::<Vec<_>>(),
            "signal_ids": input.signals.iter().map(|signal| signal.signal_id.clone()).collect::<Vec<_>>()
        },
        "happy_path": {
            "readback": readback
        },
        "acceptance": acceptance,
        "edges": edges
    });
    let written = run.write_report(&report)?;
    println!(
        "ISSUE967_NPC_AGENTS_FSV report={} sha256={} manifest={} run_id={} npcs={} signals={} accepted={} skipped={} orders={} spawn_requests={} fnv1a64={} edges={}",
        written.path.display(),
        written.sha256,
        written.latest_manifest_path.display(),
        run.run_id(),
        report["acceptance"]["npc_count"],
        report["acceptance"]["signal_count"],
        report["acceptance"]["accepted_count"],
        report["acceptance"]["skipped_count"],
        report["acceptance"]["order_count"],
        report["acceptance"]["spawn_request_count"],
        report["acceptance"]["runtime_fnv1a64"],
        report["edges"].as_array().map(Vec::len).unwrap_or(0)
    );
    Ok(())
}

fn edge_readbacks(readback: &NpcAgentActivationReadback) -> Vec<Value> {
    let mut duplicate = issue967_input();
    duplicate.npcs[1].npc_id = duplicate.npcs[0].npc_id.clone();
    let mut zero_agent = issue967_input();
    zero_agent.npcs[0].default_agent_id = 0;
    let mut missing_state = issue967_input();
    missing_state.states.pop();
    let mut unknown_target = issue967_input();
    unknown_target.signals[0].target_npc_id = Some("npc.missing".to_string());
    let mut tampered = readback.clone();
    tampered.fnv1a64 = "00000000".to_string();

    vec![
        expect_ai_invalid(
            "duplicate_npc_rejected",
            evaluate_npc_agent_activations(&duplicate),
            "npc_agents.npc_id",
            "duplicate",
        ),
        expect_ai_invalid(
            "zero_default_agent_rejected",
            evaluate_npc_agent_activations(&zero_agent),
            "npc_agents.default_agent_id",
            "non-zero",
        ),
        expect_ai_invalid(
            "missing_runtime_state_rejected",
            evaluate_npc_agent_activations(&missing_state),
            "npc_agents.states",
            "one state",
        ),
        expect_ai_invalid(
            "unknown_signal_target_rejected",
            evaluate_npc_agent_activations(&unknown_target),
            "npc_agents.signal.target_npc_id",
            "must exist",
        ),
        expect_ai_readback_mismatch(
            "tampered_runtime_hash_rejected",
            verify_npc_agent_activation_integrity(&tampered),
            "npc_agents.fnv1a64",
        ),
    ]
}

fn issue967_input() -> NpcAgentActivationInput {
    NpcAgentActivationInput {
        runtime_id: "issue967-runtime".to_string(),
        catalog_id: "ashen-reach-npcs".to_string(),
        rules: NpcAgentRules::living_world_defaults(),
        npcs: vec![
            npc(
                "npc.guard",
                "North Gate Guard",
                10,
                9671,
                "profile.guard",
                &["npc", "combat", "gate", "threat", "guard"],
                &["combat", "gate", "threat"],
                &[
                    NpcAgentIntent::RunController,
                    NpcAgentIntent::PromptReaction,
                ],
                FixedVec3::from_i32s(1, 0, 0),
            ),
            npc(
                "npc.elder",
                "Elder Cairn",
                11,
                9672,
                "profile.elder",
                &["npc", "dialogue", "lore", "ruins"],
                &["dialogue", "lore"],
                &[NpcAgentIntent::PromptDialogue, NpcAgentIntent::PromptEvent],
                FixedVec3::ZERO,
            ),
            npc(
                "npc.barkeep",
                "Golden Hearth Barkeep",
                12,
                9673,
                "profile.barkeep",
                &["npc", "social", "tavern", "schedule"],
                &["social", "tavern", "schedule"],
                &[
                    NpcAgentIntent::PromptDialogue,
                    NpcAgentIntent::PromptRoutine,
                    NpcAgentIntent::PromptEvent,
                ],
                FixedVec3::ZERO,
            ),
            npc(
                "npc.scout",
                "Thornwild Scout",
                13,
                9674,
                "profile.scout",
                &["npc", "wilderness", "help", "scout"],
                &["wilderness", "help", "scout"],
                &[
                    NpcAgentIntent::RunController,
                    NpcAgentIntent::PromptReaction,
                    NpcAgentIntent::PromptEvent,
                ],
                FixedVec3::from_i32s(0, 0, 1),
            ),
        ],
        states: vec![
            state("npc.guard", true),
            state("npc.elder", true),
            state("npc.barkeep", true),
            state("npc.scout", false),
        ],
        signals: vec![
            signal(
                "signal.guard.enemy",
                40,
                NpcAgentSignalKind::EnemyInRange,
                950,
                920,
                &["combat", "gate", "threat"],
                Some("npc.guard"),
                "raiders enter the north gate range",
            ),
            signal(
                "signal.elder.dialogue",
                41,
                NpcAgentSignalKind::DialogueOpportunity,
                850,
                875,
                &["dialogue", "lore"],
                Some("npc.elder"),
                "player asks about the ancient pact",
            ),
            signal(
                "signal.tavern.schedule",
                42,
                NpcAgentSignalKind::ScheduleTick,
                800,
                820,
                &["social", "tavern", "schedule"],
                None,
                "evening tavern service begins",
            ),
            signal(
                "signal.scout.help",
                43,
                NpcAgentSignalKind::HelpRequest,
                900,
                880,
                &["wilderness", "help", "scout"],
                None,
                "lost caravan requests a guide",
            ),
            signal(
                "signal.guard.cooldown",
                44,
                NpcAgentSignalKind::EnemyInRange,
                950,
                900,
                &["combat", "gate", "threat"],
                Some("npc.guard"),
                "second guard threat arrives inside cooldown",
            ),
        ],
    }
}

// Positional fixture constructor for NpcAgentDefinition; a struct-of-args would only
// re-spell the target struct's own fields. Scoped allow keeps the FSV fingerprint intact.
#[allow(clippy::too_many_arguments)]
fn npc(
    npc_id: &str,
    display_name: &str,
    body_id: u32,
    default_agent_id: u32,
    prompt_profile_id: &str,
    ward_tags: &[&str],
    activation_tags: &[&str],
    allowed_intents: &[NpcAgentIntent],
    order_velocity: FixedVec3,
) -> NpcAgentDefinition {
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

// Positional fixture constructor for NpcWorldSignal; struct-of-args would duplicate fields.
#[allow(clippy::too_many_arguments)]
fn signal(
    signal_id: &str,
    tick: u64,
    kind: NpcAgentSignalKind,
    strength_milli: u16,
    relevance_milli: u16,
    tags: &[&str],
    target_npc_id: Option<&str>,
    summary: &str,
) -> NpcWorldSignal {
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

fn decision<'a>(
    readback: &'a NpcAgentActivationReadback,
    signal_id: &str,
) -> Result<&'a litd_engine::ai::NpcAgentActivationDecision, Box<dyn std::error::Error>> {
    readback
        .decisions
        .iter()
        .find(|decision| decision.signal_id == signal_id)
        .ok_or_else(|| format!("missing decision {signal_id}").into())
}

fn expect_ai_invalid<T: std::fmt::Debug>(
    label: &str,
    result: Result<T, EngineError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineError::AiInvalidConfig { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "code": EngineError::AiInvalidConfig { field: field.clone(), reason: reason.clone() }.code(),
            "field": field,
            "reason": reason
        }),
        Err(error) => {
            json!({"label": label, "matched": false, "unexpected_error": error.to_string()})
        }
        Ok(value) => {
            json!({"label": label, "matched": false, "unexpected_success": format!("{value:?}")})
        }
    }
}

fn expect_ai_readback_mismatch(
    label: &str,
    result: Result<(), EngineError>,
    expected_field: &str,
) -> Value {
    match result {
        Err(EngineError::AiReadbackMismatch {
            field,
            expected,
            actual,
        }) => json!({
            "label": label,
            "matched": field == expected_field && expected != actual,
            "code": EngineError::AiReadbackMismatch { field: field.clone(), expected: expected.clone(), actual: actual.clone() }.code(),
            "field": field,
            "expected": expected,
            "actual": actual
        }),
        Err(error) => {
            json!({"label": label, "matched": false, "unexpected_error": error.to_string()})
        }
        Ok(()) => json!({"label": label, "matched": false, "unexpected": "operation succeeded"}),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn ensure(condition: bool, message: &str) -> std::io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message))
    }
}
