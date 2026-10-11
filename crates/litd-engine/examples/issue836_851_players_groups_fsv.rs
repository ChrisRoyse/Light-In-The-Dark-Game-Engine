//! FSV for #836 (players/forces/alliances + shared vision/control) and
//! #851 (unit groups / selection / order systems).
//!
//! Source of truth: a live [`MatchState`] materialized from a real lowered
//! blueprint. Happy path mutates alliance + groups + selection + orders;
//! three edges fail loud (locked alliance, wrong target shape, foreign unit
//! order). Dual-run replay proves deterministic final state hash. Report
//! written under target/fsv/ and re-hashed via certutil-equivalent sha256.
//!
//! Run: cargo run -p litd-engine --example issue836_851_players_groups_fsv --features fsv

use litd_engine::fsv_artifacts::begin_issue_run;
use litd_engine::game_runtime::{
    AllianceCell, AllianceState, AuthoredUnit, MatchBlueprint, MatchState, OrderTargetKind,
    PlayerController, PlayerSlot, UnitCapability, UnitOrder, VictoryCondition,
};
use litd_engine::math::FixedVec3;
use litd_hash::sha256_hex;
use serde_json::json;

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

fn melee_blueprint() -> MatchBlueprint {
    let slots = vec![
        slot(0, PlayerController::Human, 0),
        slot(1, PlayerController::Computer, 1),
        slot(2, PlayerController::NeutralPassive, 2),
    ];
    let mut bp = MatchBlueprint::new("issue836_851_arena", slots, 3);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.set_observers_after_defeat(true);
    bp.author_unit(unit(10, 0, true));
    bp.author_unit(unit(11, 0, false));
    bp.author_unit(unit(20, 1, true));
    bp.author_unit(unit(21, 1, false));
    bp.author_unit(unit(30, 2, false));
    bp
}

fn locked_alliance_blueprint() -> MatchBlueprint {
    let slots = vec![
        slot(0, PlayerController::Human, 0),
        slot(1, PlayerController::Computer, 1),
    ];
    let mut bp = MatchBlueprint::new("issue836_locked", slots, 2);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.author_unit(unit(1, 0, true));
    bp.author_unit(unit(2, 1, true));
    bp.author_alliance(
        0,
        1,
        AllianceCell {
            state: AllianceState::Enemy,
            shared_vision: false,
            shared_control: false,
            locked: true,
        },
    );
    bp
}

fn run_happy_path(state: &mut MatchState) -> Result<String, Box<dyn std::error::Error>> {
    // #836 SoT BEFORE
    let before_01 = format!("{:?}", state.roster().alliance(0, 1)?.state);
    let before_02 = format!("{:?}", state.roster().alliance(0, 2)?.state);
    if before_01 != "Enemy" || before_02 != "Neutral" {
        return Err(format!("unexpected defaults before={before_01}/{before_02}").into());
    }
    // Runtime ally + shared vision
    state.set_alliance(0, 1, AllianceState::Ally, true, false)?;
    let after_01 = state.roster().alliance(0, 1)?;
    if after_01.state != AllianceState::Ally || !after_01.shared_vision {
        return Err(format!("alliance after={after_01:?}").into());
    }
    if state.roster().are_enemies(0, 1)? {
        return Err("allies still reported as enemies".into());
    }

    // #851 groups + selection (order-independent)
    state.add_to_group(0, 10)?;
    state.add_to_group(0, 11)?;
    let members = state.player_groups().group(0)?.members().to_vec();
    if members != [10, 11] {
        return Err(format!("group members={members:?}").into());
    }
    state.select(0, &[11, 10])?;
    let selected = state.selection().selection(0)?.members().to_vec();
    if selected != [10, 11] {
        return Err(format!("selection not order-independent: {selected:?}").into());
    }

    // Orders: valid point move, shift-queue, then non-queue clears
    let a = UnitOrder::point(MOVE, FixedVec3::from_i32s(1, 0, 0), false);
    let b = UnitOrder::point(MOVE, FixedVec3::from_i32s(2, 0, 0), true);
    let c = UnitOrder::no_target(HOLD, false);
    state.issue_order(0, &[11], a)?;
    state.issue_order(0, &[11], b)?;
    if state.order_book(11)?.len() != 2 {
        return Err("shift-queue length != 2".into());
    }
    state.issue_order(0, &[11], c)?;
    if state.order_book(11)? != [c] {
        return Err(format!("hold did not clear queue: {:?}", state.order_book(11)?).into());
    }

    Ok(format!(
        "before_01={before_01};after_01={:?};shared_vision={};group={members:?};selection={selected:?}",
        after_01.state, after_01.shared_vision
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue836_851_players_groups")?;

    let lowered = melee_blueprint().lower()?;
    let mut state_a = MatchState::from_lowered(&lowered)?;
    let happy_a = run_happy_path(&mut state_a)?;
    let hash_a = format!("{:016x}", state_a.state_hash());

    let mut state_b = MatchState::from_lowered(&lowered)?;
    let happy_b = run_happy_path(&mut state_b)?;
    let hash_b = format!("{:016x}", state_b.state_hash());
    let replay_match = hash_a == hash_b && happy_a == happy_b;

    // Edge 1: locked alliance fails loud
    let locked = MatchState::from_lowered(&locked_alliance_blueprint().lower()?)?;
    let mut locked_mut = locked;
    let locked_err = locked_mut
        .set_alliance(0, 1, AllianceState::Ally, false, false)
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "SILENT_OK".into());
    let locked_fail_loud = locked_err == "LITD-GAME-PACKAGE-INVALID-CONFIG";

    // Edge 2: wrong target shape fails loud
    let mut edge_state = MatchState::from_lowered(&lowered)?;
    let wrong = UnitOrder::unit_target(MOVE, 20, false);
    let wrong_err = edge_state
        .issue_order(0, &[11], wrong)
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "SILENT_OK".into());
    let wrong_fail_loud = wrong_err == "LITD-SIM-INVALID-CONFIG";

    // Edge 3: order on foreign unit fails loud
    let ok = UnitOrder::point(MOVE, FixedVec3::from_i32s(5, 0, 0), false);
    let foreign_err = edge_state
        .issue_order(0, &[21], ok)
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "SILENT_OK".into());
    let foreign_fail_loud = foreign_err != "SILENT_OK";

    let verdict = replay_match && locked_fail_loud && wrong_fail_loud && foreign_fail_loud;
    let report = json!({
        "schema": "litd.fsv.issue836_851.players_groups.v1",
        "issues": [836, 851],
        "parents": [804],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "happy_path_trace": happy_a,
        "state_hash_a": hash_a,
        "state_hash_b": hash_b,
        "replay_match": replay_match,
        "edge_locked_alliance_code": locked_err,
        "edge_locked_alliance_fail_loud": locked_fail_loud,
        "edge_wrong_target_code": wrong_err,
        "edge_wrong_target_fail_loud": wrong_fail_loud,
        "edge_foreign_unit_code": foreign_err,
        "edge_foreign_unit_fail_loud": foreign_fail_loud,
        "sufficiency_argument": "Source of truth is MatchState after blueprint.lower(): alliance matrix, player_groups, selection, and order_book are read back after mutations. Dual runs must share state_hash. Locked alliance / wrong target / foreign unit fail with greppable codes (no silent success).",
    });
    let written = run.write_report(&report)?;
    let report_sha = sha256_hex(&std::fs::read(&written.path)?);
    println!(
        "ISSUE836_851_PLAYERS_GROUPS_FSV verdict={} report={} sha256={} replay_match={} locked={} wrong={} foreign={}",
        if verdict { "GREEN" } else { "RED" },
        written.path.display(),
        report_sha,
        replay_match,
        locked_fail_loud,
        wrong_fail_loud,
        foreign_fail_loud,
    );
    if !verdict {
        return Err("ISSUE836_851 FSV verdict RED".into());
    }
    Ok(())
}
