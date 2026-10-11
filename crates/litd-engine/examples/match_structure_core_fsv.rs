//! FSV for the match-structure core (#836 players/alliances, #851 groups/
//! selection/orders, #838 victory/defeat) over the real runtime APIs.
//!
//! End-to-end: author a 2-player + neutral map blueprint via the real builder,
//! lower it (canonical RON + sha256 fingerprint), verify the artifact, then run
//! the deterministic runtime. Three edge cases print before/after state: the
//! alliance matrix queried before/after a runtime alliance-change order; unit
//! groups filtered and orders issued/validated deterministically; and destroying
//! the last enemy structure flipping victory/defeat states.
//!
//! A replay-determinism check runs an identical order stream twice and asserts
//! identical `state_hash`. A negative control proves sabotaged condition data
//! fails loud. A `report.json` is written path-free/run-invariant and its sha256
//! is recomputed independently (certutil) by the caller and printed here too.

use std::{env, error::Error, fs, path::PathBuf};

use litd_engine::{
    game_runtime::{
        AllianceState, AuthoredUnit, MatchBlueprint, MatchOrder, MatchOrderKind, MatchOrderQueue,
        MatchState, OrderTargetKind, PlayerController, PlayerResult, PlayerSlot, UnitCapability,
        UnitOrder, VictoryCondition,
    },
    math::FixedVec3,
    sim::{FixedTickSim, SimConfig},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const MOVE: u16 = 1;
const ATTACK: u16 = 2;
const HOLD: u16 = 3;

fn slot(id: u8, controller: PlayerController, force: u8) -> PlayerSlot {
    PlayerSlot {
        id,
        controller,
        force,
        color: id,
        handicap_pct: 100,
    }
}

fn unit(id: u32, owner: u8, is_structure: bool) -> AuthoredUnit {
    AuthoredUnit {
        unit: id,
        owner,
        is_structure,
        capabilities: vec![
            UnitCapability {
                ability: MOVE,
                target_kind: OrderTargetKind::Point,
            },
            UnitCapability {
                ability: ATTACK,
                target_kind: OrderTargetKind::Unit,
            },
            UnitCapability {
                ability: HOLD,
                target_kind: OrderTargetKind::None,
            },
        ],
    }
}

/// P0 human (force 0), P1 computer (force 1), P2 neutral-passive (force 2).
fn blueprint() -> MatchBlueprint {
    let slots = vec![
        slot(0, PlayerController::Human, 0),
        slot(1, PlayerController::Computer, 1),
        slot(2, PlayerController::NeutralPassive, 2),
    ];
    let mut bp = MatchBlueprint::new("fsv_arena", slots, 3);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.set_observers_after_defeat(true);
    bp.author_unit(unit(10, 0, true)); // P0 keep
    bp.author_unit(unit(11, 0, false)); // P0 soldier
    bp.author_unit(unit(20, 1, true)); // P1 keep (the last enemy structure)
    bp.author_unit(unit(30, 2, false)); // neutral critter
    bp
}

fn result_name(result: PlayerResult) -> &'static str {
    match result {
        PlayerResult::Playing => "playing",
        PlayerResult::Victory => "victory",
        PlayerResult::Defeat => "defeat",
    }
}

fn alliance_name(state: AllianceState) -> &'static str {
    match state {
        AllianceState::Ally => "ally",
        AllianceState::Neutral => "neutral",
        AllianceState::Enemy => "enemy",
    }
}

/// Drive the full order stream once and return the final match state.
fn run_stream() -> Result<MatchState, Box<dyn Error>> {
    let lowered = blueprint().lower()?;
    let mut state = MatchState::from_lowered(&lowered)?;
    let mut queue = MatchOrderQueue::new();
    queue.enqueue(MatchOrder::new(
        1,
        0,
        0,
        MatchOrderKind::SetAlliance {
            from: 0,
            to: 1,
            state: AllianceState::Ally,
            shared_vision: true,
            shared_control: false,
        },
    ))?;
    queue.enqueue(MatchOrder::new(
        1,
        0,
        1,
        MatchOrderKind::Select {
            units: vec![11, 10],
        },
    ))?;
    queue.enqueue(MatchOrder::new(
        2,
        0,
        0,
        MatchOrderKind::IssueOrder {
            units: vec![11],
            order: UnitOrder::point(MOVE, FixedVec3::from_i32s(3, 0, 0), false),
        },
    ))?;
    queue.enqueue(MatchOrder::new(
        3,
        0,
        0,
        MatchOrderKind::DestroyUnit { unit: 20 },
    ))?;
    queue.run_ticks(&mut state, 4)?;
    Ok(state)
}

fn negative_control_code() -> String {
    // A custom condition referencing a non-existent player must fail loud.
    let mut bp = MatchBlueprint::new(
        "sabotaged",
        vec![
            slot(0, PlayerController::Human, 0),
            slot(1, PlayerController::Computer, 1),
        ],
        2,
    );
    bp.author_condition(VictoryCondition::CustomTrigger {
        player: 9,
        trigger: 1,
    });
    bp.author_unit(unit(1, 0, true));
    let lowered = bp.lower().expect("lower sabotaged blueprint");
    match MatchState::from_lowered(&lowered) {
        Ok(_) => panic!("sabotaged condition data was accepted"),
        Err(err) => err.code().to_string(),
    }
}

fn tampered_artifact_code() -> String {
    let mut lowered = blueprint().lower().expect("lower");
    lowered.canonical_ron.push(' ');
    match lowered.verify() {
        Ok(_) => panic!("tampered artifact verified"),
        Err(err) => err.code().to_string(),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();

    // Author + lower + verify. The fingerprint is run-invariant.
    let lowered = blueprint().lower()?;
    let verified = lowered.verify().is_ok();

    // A tiny FixedTickSim proves the match layer is sim-adjacent: one body per
    // unit, and the last enemy structure is fatally attacked in the sim before
    // the match records its destruction. Both worlds are deterministic.
    let config = SimConfig::new(64)?;
    let mut sim = FixedTickSim::<8>::new(config, 0x836_838_851)?;
    let keep = sim.spawn_body(FixedVec3::ZERO, FixedVec3::ZERO, FixedVec3::ZERO)?;
    let attacker = sim.spawn_body(
        FixedVec3::from_i32s(1, 0, 0),
        FixedVec3::ZERO,
        FixedVec3::ZERO,
    )?;
    sim.record_attack_at(1, attacker, keep, 4096)?;
    let sim_hash = sim.run_ticks(1)?.state_hash;

    // Edge 1: alliance matrix before/after the runtime alliance-change order.
    let mut state = MatchState::from_lowered(&lowered)?;
    let alliance_before = state.roster().alliance(0, 1)?.state;
    let neutral_before = state.roster().alliance(0, 2)?.state;
    state.set_alliance(0, 1, AllianceState::Ally, true, false)?;
    let alliance_after = state.roster().alliance(0, 1)?.state;
    println!(
        "[edge1 alliance] P0->P1 before={} after={} ; P0->P2(neutral)={}",
        alliance_name(alliance_before),
        alliance_name(alliance_after),
        alliance_name(neutral_before),
    );

    // Edge 2: groups filter + deterministic order issue/validation.
    let mut edge2 = MatchState::from_lowered(&lowered)?;
    edge2.add_to_group(0, 10)?;
    edge2.add_to_group(0, 11)?;
    let group_members = edge2.player_groups().group(0)?.members().to_vec();
    let structures_only = edge2.player_groups().group(0)?.filter(|u| u == 10);
    edge2.select(0, &[11, 10])?;
    let selection = edge2.selection().selection(0)?.members().to_vec();
    let issued = edge2.issue_order(
        0,
        &[11],
        UnitOrder::point(MOVE, FixedVec3::from_i32s(3, 0, 0), false),
    )?;
    let bad_order_code = match edge2.issue_order(0, &[11], UnitOrder::unit_target(MOVE, 20, false))
    {
        Ok(_) => panic!("invalid target order accepted"),
        Err(err) => err.code().to_string(),
    };
    println!(
        "[edge2 groups] group={group_members:?} filter_structures={:?} selection={selection:?} issued={issued} bad_order_code={bad_order_code}",
        structures_only.members(),
    );

    // Edge 3: destroy the last enemy structure -> victory/defeat flip.
    let mut edge3 = MatchState::from_lowered(&lowered)?;
    let p0_before = edge3.result(0)?;
    let p1_before = edge3.result(1)?;
    let events = edge3.destroy_unit(20)?;
    let p0_after = edge3.result(0)?;
    let p1_after = edge3.result(1)?;
    let p1_observer = edge3.is_observer(1)?;
    println!(
        "[edge3 victory] P0 {}->{} ; P1 {}->{} ; P1_observer={} ; game_over_events={}",
        result_name(p0_before),
        result_name(p0_after),
        result_name(p1_before),
        result_name(p1_after),
        p1_observer,
        events.len(),
    );

    // Replay determinism: identical order streams -> identical state_hash.
    let hash_a = run_stream()?.state_hash();
    let hash_b = run_stream()?.state_hash();
    let replay_ok = hash_a == hash_b;
    println!("[replay] hash_a={hash_a:016X} hash_b={hash_b:016X} identical={replay_ok}");

    // Negative controls (both must fail loud with stable codes).
    let sabotage_code = negative_control_code();
    let tamper_code = tampered_artifact_code();
    println!(
        "[negative] sabotaged_condition_code={sabotage_code} tampered_artifact_code={tamper_code}"
    );

    let verdict = verified
        && replay_ok
        && matches!(p1_after, PlayerResult::Defeat)
        && matches!(p0_after, PlayerResult::Victory)
        && matches!(p1_before, PlayerResult::Playing)
        && bad_order_code == "LITD-SIM-INVALID-CONFIG"
        && sabotage_code == "LITD-GAME-PACKAGE-INVALID-CONFIG"
        && tamper_code == "LITD-GAME-PACKAGE-INVALID-CONFIG";

    let report: Value = json!({
        "schema": "litd.game.match_structure_core.fsv.v1",
        "issues": [836, 838, 851],
        "verdict": if verdict { "green" } else { "red" },
        "lowered_fingerprint": lowered.fingerprint,
        "artifact_verified": verified,
        "sim_state_hash": format!("{sim_hash:016X}"),
        "edge1_alliance": {
            "p0_p1_before": alliance_name(alliance_before),
            "p0_p1_after": alliance_name(alliance_after),
            "p0_p2_neutral": alliance_name(neutral_before),
        },
        "edge2_groups": {
            "group_members": group_members,
            "filter_structures": structures_only.members(),
            "selection_sorted": selection,
            "orders_issued": issued,
            "invalid_order_code": bad_order_code,
        },
        "edge3_victory": {
            "p0_before": result_name(p0_before),
            "p0_after": result_name(p0_after),
            "p1_before": result_name(p1_before),
            "p1_after": result_name(p1_after),
            "p1_observer_after_defeat": p1_observer,
            "game_over_events": events.len(),
        },
        "replay_determinism": {
            "state_hash": format!("{hash_a:016X}"),
            "identical": replay_ok,
        },
        "negative_controls": {
            "sabotaged_condition_code": sabotage_code,
            "tampered_artifact_code": tamper_code,
        },
    });
    let report_text = serde_json::to_string_pretty(&report)? + "\n";

    let path = env::var("LITD_MATCH_STRUCTURE_FSV_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/match-structure-core-fsv-report.json"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &report_text)?;
    let report_sha = sha256_hex(report_text.as_bytes());
    println!(
        "wrote report to {} verdict={}",
        path.display(),
        if verdict { "green" } else { "red" }
    );
    println!("report_sha256={report_sha}");
    println!("replayed_state_hash={hash_a:016X}");

    if !verdict {
        return Err("FSV verdict RED".into());
    }
    Ok(())
}
