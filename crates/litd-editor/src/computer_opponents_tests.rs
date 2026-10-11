use litd_engine::{
    ai::load_ai_controller_run_json,
    fsv_artifacts::begin_issue_run,
    math::{Fixed, FixedVec3},
    nav::{NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput, bake_navmesh},
    sim::{FixedTickSim, SimConfig},
};

use super::*;

#[test]
fn computer_opponent_authors_and_lowers_chase_order() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let run = begin_issue_run("issue852_computer_opponents_unit").unwrap();
    let root = run.run_dir();
    let state_path = root.join("agent-state-aster");
    let opponents = vec![opponent("agent.unit", "unit.ai", 852)];
    let authoring = author_computer_opponents(&ComputerOpponentAuthoringArgs {
        state_path: state_path.clone(),
        ledger_dir: root.join("authoring-ledgers"),
        state_id: "issue852_unit_state".to_string(),
        requested_by: "issue852_unit".to_string(),
        opponents: opponents.clone(),
    })
    .unwrap();
    assert_eq!(authoring.binding_count, 1);
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(1).unwrap(), 852).unwrap();
    let ai = sim
        .spawn_body(v(0, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    let human = sim
        .spawn_body(v(2, 0, 0), FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    let controller_path = root.join("controller-aster");
    let ledger_path = root.join("opponent-ledger-aster");
    let ledger = lower_computer_opponent_orders(
        &ComputerOpponentRuntimeArgs {
            run_id: "issue852-unit".to_string(),
            state_path,
            controller_run_path: controller_path.clone(),
            ledger_path: ledger_path.clone(),
            opponents,
            objects: vec![ComputerOpponentObject {
                object_id: "unit.ai".to_string(),
                body_id: ai,
                nav_cell: NavCellCoord::new(0, 0),
                faction: "ai".to_string(),
                source_id: 852,
                alive: true,
            }],
            targets: vec![ComputerOpponentTarget {
                entity_id: "player.human".to_string(),
                body_id: human,
                nav_cell: NavCellCoord::new(2, 0),
                faction: "human".to_string(),
                alive: true,
            }],
        },
        &line_navmesh(3),
        &sim,
    )
    .unwrap();
    let controller = load_ai_controller_run_json(&controller_path).unwrap();
    assert_eq!(controller.report.order_count, 1);
    assert_eq!(ledger.ledger.order_count, 1);
    assert_eq!(read_computer_opponent_ledger(&ledger_path).unwrap(), ledger);
}

#[test]
fn duplicate_computer_player_fails_loud_before_write() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let run = begin_issue_run("issue852_computer_opponents_duplicate").unwrap();
    let root = run.run_dir();
    let mut opponents = vec![
        opponent("agent.a", "unit.a", 852),
        opponent("agent.b", "unit.b", 853),
    ];
    opponents[1].player_id = opponents[0].player_id;
    let state_path = root.join("duplicate-state-aster");
    let error = author_computer_opponents(&ComputerOpponentAuthoringArgs {
        state_path: state_path.clone(),
        ledger_dir: root.join("duplicate-ledgers"),
        state_id: "issue852_duplicate".to_string(),
        requested_by: "issue852_unit".to_string(),
        opponents,
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("duplicate player id"));
    assert!(!state_path.exists());
}

#[test]
fn issue1524_batched_ledger_amortizes_flushes_without_data_loss() {
    // FSV for issue #1524 item 4: the promotion-ready batched ledger must (a) reduce
    // durable write+verify cycles from one-per-row to one-per-cadence-batch, and
    // (b) lose no data — every appended row survives, in order, on read-back.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let run = begin_issue_run("issue1524_batched_ledger").unwrap();
    let root = run.run_dir();
    const N: usize = 24;
    const CADENCE: usize = 8;
    let rows: Vec<ComputerOpponentRunLedger> = (0..N).map(sample_ledger_row).collect();

    // BEFORE: per-row durable write + full verify (cadence 1) == current tool behavior
    // if it were promoted per-tick unchanged.
    let mut before =
        BatchedComputerOpponentLedger::new(root.join("before-aster"), "issue1524-before", 1)
            .unwrap();
    for row in &rows {
        before.append(row.clone()).unwrap();
    }
    let before_rb = before.finish().unwrap();

    // AFTER: bounded-cadence batching over the identical workload.
    let mut after =
        BatchedComputerOpponentLedger::new(root.join("after-aster"), "issue1524-after", CADENCE)
            .unwrap();
    for row in &rows {
        after.append(row.clone()).unwrap();
    }
    let after_rb = after.finish().unwrap();

    // (a) Flush cadence: N durable writes before, ceil(N/CADENCE) after.
    assert_eq!(
        before_rb.flush_count, N,
        "cadence-1 must flush once per row"
    );
    assert_eq!(
        after_rb.flush_count,
        N.div_ceil(CADENCE),
        "cadence batching must amortize flushes"
    );
    assert!(after_rb.flush_count < before_rb.flush_count);

    // (b) Durability: no data loss, order and content preserved through the store.
    assert_eq!(before_rb.appended, N);
    assert_eq!(after_rb.appended, N);
    assert_eq!(
        before_rb.rows, rows,
        "cadence-1 read-back lost/reordered rows"
    );
    assert_eq!(after_rb.rows, rows, "batched read-back lost/reordered rows");

    // Independent read-back of each stored batch confirms it is physically present.
    let mut recovered = Vec::new();
    for batch_index in 0..after_rb.flush_count {
        let batch = read_computer_opponent_ledger_batch(&root.join("after-aster"), batch_index)
            .expect("stored batch must read back");
        recovered.extend(batch.rows);
    }
    assert_eq!(recovered, rows, "physical batch read-back lost data");

    println!(
        "issue1524-item4 ledger-cadence N={N} cadence={CADENCE} \
         before_flush_count={} after_flush_count={} durable_rows_readback={} data_loss={}",
        before_rb.flush_count,
        after_rb.flush_count,
        after_rb.rows.len(),
        N - after_rb.rows.len(),
    );
}

fn sample_ledger_row(index: usize) -> ComputerOpponentRunLedger {
    ComputerOpponentRunLedger {
        schema: COMPUTER_OPPONENT_LEDGER_SCHEMA.to_string(),
        issue: 852,
        run_id: format!("issue1524-run-{index}"),
        source_of_truth: "issue1524 batched ledger fsv".to_string(),
        mcp_agent_state_path: "state".to_string(),
        mcp_agent_state_sha256: "0".repeat(64),
        mcp_agent_state_fingerprint: "f".repeat(16),
        controller_run_path: "controller".to_string(),
        controller_sha256: "0".repeat(64),
        controller_fnv1a64: "0".repeat(16),
        perception_fnv1a64: "0".repeat(16),
        navmesh_fnv1a64: "0".repeat(16),
        tick: index as u64,
        opponent_count: 1,
        target_count: 1,
        decision_count: 0,
        order_count: 0,
        decisions: Vec::new(),
        ledger_fingerprint: "0".repeat(16),
    }
}

fn opponent(agent_id: &str, object_id: &str, source_id: u32) -> ComputerOpponentDefinition {
    ComputerOpponentDefinition {
        player_id: (source_id % 10) as u8,
        player_name: agent_id.to_string(),
        agent_id: agent_id.to_string(),
        object_id: object_id.to_string(),
        display_name: agent_id.to_string(),
        faction: "ai".to_string(),
        source_id,
        region_id: "region.test".to_string(),
        enabled: true,
        script: ComputerOpponentScript {
            script_id: format!("script.{agent_id}"),
            behavior_preset: "melee_guardian".to_string(),
            prompt: "guard lane and attack enemies".to_string(),
            model_id: "model.local.tiny_agent".to_string(),
            personality: "melee".to_string(),
            allowed_intents: vec!["intent.attack".to_string(), "intent.chase".to_string()],
            forbidden_intents: vec!["intent.attack_civilian".to_string()],
            sight_range_cells: 8,
            attack_range_cells: 1,
            threat_range_cells: 8,
            path_max_expansions: 16,
            attack_damage_fp: 7,
        },
    }
}

fn line_navmesh(width: i32) -> litd_engine::nav::BakedNavMesh {
    let cells = (0..width)
        .map(|x| NavTerrainCellInput {
            id: format!("cell.{x:02}"),
            x,
            z: 0,
            height_fp: 0,
            flags: NavPathingFlags::default(),
        })
        .collect::<Vec<_>>();
    bake_navmesh(&NavBakeInput::new(Fixed::ONE.raw(), cells, Vec::new())).unwrap()
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
