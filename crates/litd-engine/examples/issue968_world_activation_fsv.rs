use litd_engine::{
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    calyx_world_activation::{
        EngineWorldActivationAgentKind, EngineWorldActivationError, EngineWorldActivationGrade,
        EngineWorldActivationInput, EngineWorldActivationIntent, EngineWorldActivationPolicy,
        EngineWorldAgentProfile, EngineWorldAgentState, EngineWorldFact, EngineWorldFactKind,
        evaluate_calyx_world_activation, read_calyx_world_activation, store_calyx_world_activation,
        validate_world_activation_readback,
    },
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    sim::SimBodyId,
    sim_order::SimOrderSourceKind,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue968_world_activation")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let input = issue968_input();
    let readback = evaluate_calyx_world_activation(&input)?;
    let stored = store_calyx_world_activation(&database, &readback)?;
    let disk = read_calyx_world_activation(&database, "issue968-world-activation")?;
    let edges = edge_readbacks(&database, &readback);

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
    let persisted_matches = stored.activation.activation_fingerprint
        == readback.activation_fingerprint
        && disk.activation.activation_fingerprint == readback.activation_fingerprint
        && stored.database.sha256 == disk.database.sha256;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool() == Some(true));
    let acceptance = json!({
        "fact_count": readback.fact_count,
        "fact_kind_count": readback.fact_kind_count,
        "agent_count": readback.agent_count,
        "association_count": readback.association_count,
        "decision_count": readback.decision_count,
        "accepted_count": readback.accepted_count,
        "skipped_count": readback.skipped_count,
        "prompt_count": readback.prompt_count,
        "order_count": readback.order_count,
        "spawn_request_count": readback.spawn_request_count,
        "hot_path_policy": readback.hot_path_policy,
        "calyx_live_query_allowed_in_hot_path": readback.calyx_live_query_allowed_in_hot_path,
        "all_orders_lowered_agent": all_orders_lowered_agent,
        "all_order_fingerprints_match": all_order_fingerprints_match,
        "persisted_matches": persisted_matches,
        "guard_grade": grade(&readback, "agent.guard"),
        "barkeep_grade": grade(&readback, "agent.barkeep"),
        "scout_spawn_requested": spawn_requested(&readback, "agent.scout"),
        "rift_grade": grade(&readback, "agent.rift"),
        "elder_grade": grade(&readback, "agent.elder"),
        "activation_fingerprint": readback.activation_fingerprint,
        "stored_database_sha256": stored.database.sha256,
        "all_edges_matched": all_edges_matched,
        "invalid_input_code": EngineWorldActivationError::InvalidInput { field: "x".to_string(), reason: "y".to_string() }.code()
    });
    ensure(readback.fact_count == 5, "expected five world facts")?;
    ensure(readback.fact_kind_count == 4, "expected four fact kinds")?;
    ensure(
        readback.accepted_count == 3,
        "expected three accepted activations",
    )?;
    ensure(
        readback.skipped_count == 2,
        "expected two skipped activations",
    )?;
    ensure(
        all_orders_lowered_agent,
        "orders must be lowered-agent sourced",
    )?;
    ensure(
        all_order_fingerprints_match,
        "order fingerprints must match",
    )?;
    ensure(persisted_matches, "persisted readback mismatch")?;
    ensure(all_edges_matched, "edge readbacks did not all match")?;

    let report = json!({
        "schema": "litd.engine.issue968.world_activation_fsv.v1",
        "schema_version": 1,
        "issue": 968,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual FSV must inspect this persisted JSON, manifest, Aster ledger row, world facts, associations, decisions, lowered orders, and hashes"
        },
        "sufficiency_argument": "The minimal synthetic tick has five facts covering all #968 input classes: log_line, event, measure, and bit. It drives three accepted activations, one Ward-blocked activation, and one no-association skip, which covers ingestion, association scoring, targeted agent selection, spawn request, deterministic lowering, Ward refusal, and no-effect skips. Edge readbacks cover duplicate facts, zero lowered id, missing state, unknown target, tampered hash, and missing persisted ledger.",
        "readme_references_reviewed": [
            "docs/readme/06-living-agents.png",
            "docs/readme/S17-agent-binding.png",
            "docs/readme/S20-living-world.png",
            "docs/readme/GP10-dialogue.png",
            "docs/readme/GP17-tavern.png"
        ],
        "research_sources": [
            {"label": "Epic MassGameplay overview", "url": "https://dev.epicgames.com/documentation/unreal-engine/overview-of-mass-gameplay-in-unreal-engine?lang=en-US", "used_for": "large entity spawning, LOD, and StateTree-style world orchestration"},
            {"label": "Epic Gameplay Tags", "url": "https://dev.epicgames.com/documentation/unreal-engine/using-gameplay-tags-in-unreal-engine?lang=en-US", "used_for": "tag containers and tag queries as deterministic gameplay selectors"},
            {"label": "Epic StateTree", "url": "https://dev.epicgames.com/documentation/unreal-engine/state-tree-in-unreal-engine?lang=en-US", "used_for": "hierarchical state/transition inspiration for flexible activation decisions"},
            {"label": "Gaffer deterministic lockstep", "url": "https://gafferongames.com/post/deterministic_lockstep/", "used_for": "only deterministic inputs cross the sim boundary"}
        ],
        "input_summary": {
            "run_id": input.run_id,
            "world_id": input.world_id,
            "policy": input.policy,
            "agent_ids": input.agents.iter().map(|agent| agent.agent_id.clone()).collect::<Vec<_>>(),
            "fact_ids": input.facts.iter().map(|fact| fact.fact_id.clone()).collect::<Vec<_>>()
        },
        "happy_path": {
            "readback": readback,
            "stored_database": stored.database,
            "disk_database": disk.database
        },
        "acceptance": acceptance,
        "edges": edges
    });
    let written = run.write_report(&report)?;
    println!(
        "ISSUE968_WORLD_ACTIVATION_FSV report={} sha256={} manifest={} run_id={} facts={} fact_kinds={} agents={} accepted={} skipped={} orders={} stored_sha={} edges={}",
        written.path.display(),
        written.sha256,
        written.latest_manifest_path.display(),
        run.run_id(),
        report["acceptance"]["fact_count"],
        report["acceptance"]["fact_kind_count"],
        report["acceptance"]["agent_count"],
        report["acceptance"]["accepted_count"],
        report["acceptance"]["skipped_count"],
        report["acceptance"]["order_count"],
        report["acceptance"]["stored_database_sha256"],
        report["edges"].as_array().map(Vec::len).unwrap_or(0)
    );
    Ok(())
}

fn edge_readbacks(
    database: &EngineCalyxDatabase,
    readback: &litd_engine::calyx_world_activation::EngineWorldActivationReadback,
) -> Vec<Value> {
    let mut duplicate = issue968_input();
    duplicate.facts[1].fact_id = duplicate.facts[0].fact_id.clone();
    let mut zero_agent = issue968_input();
    zero_agent.agents[0].lowered_agent_id = 0;
    let mut missing_state = issue968_input();
    missing_state.states.pop();
    let mut unknown_target = issue968_input();
    unknown_target.facts[0].target_agent_id = Some("agent.missing".to_string());
    let mut tampered = readback.clone();
    tampered.activation_fingerprint = "00000000".to_string();
    vec![
        expect_invalid(
            "duplicate_fact_rejected",
            evaluate_calyx_world_activation(&duplicate),
            "world_activation.fact_id",
            "duplicate",
        ),
        expect_invalid(
            "zero_lowered_agent_rejected",
            evaluate_calyx_world_activation(&zero_agent),
            "world_activation.lowered_agent_id",
            "non-zero",
        ),
        expect_invalid(
            "missing_state_rejected",
            evaluate_calyx_world_activation(&missing_state),
            "world_activation.states",
            "one state",
        ),
        expect_invalid(
            "unknown_target_rejected",
            evaluate_calyx_world_activation(&unknown_target),
            "world_activation.fact.target_agent_id",
            "must exist",
        ),
        expect_mismatch(
            "tampered_hash_rejected",
            validate_world_activation_readback(&tampered),
            "world_activation.activation_fingerprint",
        ),
        missing_ledger_edge(database),
    ]
}

fn issue968_input() -> EngineWorldActivationInput {
    EngineWorldActivationInput {
        run_id: "issue968-world-activation".to_string(),
        world_id: "world.ashen-reach".to_string(),
        policy: EngineWorldActivationPolicy::living_world_defaults("issue968-policy"),
        agents: vec![
            agent(
                "agent.guard",
                "North Gate Guard",
                9681,
                20,
                &["combat", "gate", "threat"],
                &["combat", "gate", "threat"],
                EngineWorldActivationIntent::RunController,
                FixedVec3::from_i32s(1, 0, 0),
            ),
            agent(
                "agent.barkeep",
                "Golden Hearth Barkeep",
                9682,
                21,
                &["economy", "supply", "tavern"],
                &["economy", "supply", "tavern"],
                EngineWorldActivationIntent::PromptRoutine,
                FixedVec3::ZERO,
            ),
            agent(
                "agent.scout",
                "Thornwild Scout",
                9683,
                22,
                &["wilderness", "help", "scout"],
                &["wilderness", "help", "scout"],
                EngineWorldActivationIntent::SpawnAgent,
                FixedVec3::from_i32s(0, 0, 1),
            ),
            agent(
                "agent.rift",
                "Rift Watcher",
                9684,
                23,
                &["rift", "teleport"],
                &["rift", "visual"],
                EngineWorldActivationIntent::PromptEvent,
                FixedVec3::ZERO,
            ),
            agent(
                "agent.elder",
                "Elder Cairn",
                9685,
                24,
                &["lore", "dialogue"],
                &["lore", "dialogue"],
                EngineWorldActivationIntent::PromptDialogue,
                FixedVec3::ZERO,
            ),
        ],
        states: vec![
            state("agent.guard", true),
            state("agent.barkeep", true),
            state("agent.scout", false),
            state("agent.rift", true),
            state("agent.elder", true),
        ],
        facts: vec![
            fact(
                "fact.log.enemy",
                40,
                EngineWorldFactKind::LogLine,
                320,
                None,
                &["combat", "gate", "threat"],
                Some("agent.guard"),
                "log: raiders shouted at the north gate",
            ),
            fact(
                "fact.event.player",
                41,
                EngineWorldFactKind::Event,
                310,
                None,
                &["combat", "gate", "threat"],
                Some("agent.guard"),
                "event: player entered the gate combat radius",
            ),
            fact(
                "fact.measure.supply",
                42,
                EngineWorldFactKind::Measure,
                500,
                Some(880),
                &["economy", "supply", "tavern"],
                Some("agent.barkeep"),
                "measure: tavern stockpile below evening threshold",
            ),
            fact(
                "fact.bit.help",
                43,
                EngineWorldFactKind::Bit,
                900,
                None,
                &["wilderness", "help", "scout"],
                Some("agent.scout"),
                "bit: caravan needs a wilderness guide",
            ),
            fact(
                "fact.event.rift",
                44,
                EngineWorldFactKind::Event,
                900,
                None,
                &["rift", "teleport", "visual"],
                Some("agent.rift"),
                "event: rift asks for illegal teleport visual",
            ),
        ],
    }
}

// Positional fixture constructor for EngineWorldAgentProfile; struct-of-args would duplicate fields.
#[allow(clippy::too_many_arguments)]
fn agent(
    agent_id: &str,
    display_name: &str,
    lowered_agent_id: u32,
    body_id: u32,
    trigger_tags: &[&str],
    ward_tags: &[&str],
    intent: EngineWorldActivationIntent,
    order_velocity: FixedVec3,
) -> EngineWorldAgentProfile {
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

// Positional fixture constructor for EngineWorldFact; struct-of-args would duplicate fields.
#[allow(clippy::too_many_arguments)]
fn fact(
    fact_id: &str,
    tick: u64,
    kind: EngineWorldFactKind,
    weight_milli: u16,
    measure_milli: Option<u16>,
    tags: &[&str],
    target_agent_id: Option<&str>,
    summary: &str,
) -> EngineWorldFact {
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
    readback: &litd_engine::calyx_world_activation::EngineWorldActivationReadback,
    agent_id: &str,
) -> EngineWorldActivationGrade {
    readback
        .decisions
        .iter()
        .find(|decision| decision.agent_id == agent_id)
        .map(|decision| decision.grade)
        .unwrap()
}

fn spawn_requested(
    readback: &litd_engine::calyx_world_activation::EngineWorldActivationReadback,
    agent_id: &str,
) -> bool {
    readback
        .decisions
        .iter()
        .find(|decision| decision.agent_id == agent_id)
        .map(|decision| decision.spawn_requested)
        .unwrap_or(false)
}

fn expect_invalid<T: std::fmt::Debug>(
    label: &str,
    result: Result<T, EngineWorldActivationError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineWorldActivationError::InvalidInput { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "code": EngineWorldActivationError::InvalidInput { field: field.clone(), reason: reason.clone() }.code(),
            "field": field,
            "reason": reason
        }),
        Err(error) => {
            json!({"label": label, "matched": false, "unexpected_error": error.to_string(), "code": error.code()})
        }
        Ok(value) => {
            json!({"label": label, "matched": false, "unexpected_success": format!("{value:?}")})
        }
    }
}

fn expect_mismatch(
    label: &str,
    result: Result<(), EngineWorldActivationError>,
    field: &str,
) -> Value {
    match result {
        Err(EngineWorldActivationError::ReadbackMismatch {
            field: actual,
            expected,
            actual: got,
        }) => json!({
            "label": label,
            "matched": actual == field && expected != got,
            "code": EngineWorldActivationError::ReadbackMismatch { field: actual.clone(), expected: expected.clone(), actual: got.clone() }.code(),
            "field": actual,
            "expected": expected,
            "actual": got
        }),
        Err(error) => {
            json!({"label": label, "matched": false, "unexpected_error": error.to_string(), "code": error.code()})
        }
        Ok(()) => json!({"label": label, "matched": false, "unexpected": "operation succeeded"}),
    }
}

fn missing_ledger_edge(database: &EngineCalyxDatabase) -> Value {
    match read_calyx_world_activation(database, "missing-world-activation") {
        Err(EngineWorldActivationError::Persistence(error)) => json!({
            "label": "missing_ledger_rejected",
            "matched": error.to_string().contains("missing-world-activation"),
            "code": EngineWorldActivationError::Persistence(error.clone()).code(),
            "error": error.to_string()
        }),
        Err(error) => {
            json!({"label": "missing_ledger_rejected", "matched": false, "unexpected_error": error.to_string(), "code": error.code()})
        }
        Ok(_) => {
            json!({"label": "missing_ledger_rejected", "matched": false, "unexpected": "read succeeded"})
        }
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
