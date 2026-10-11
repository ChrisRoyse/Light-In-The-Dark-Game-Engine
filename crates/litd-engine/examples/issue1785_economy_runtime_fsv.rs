//! FSV harness for the base-building & economy runtime (#1785 / #837).
//!
//! Authors a mini base via the real runtime authoring API (resource nodes, a town-hall
//! structure, a worker + soldier unit type with real costs), lowers + packages it to a
//! fingerprinted RON artifact, then runs the deterministic runtime end to end:
//!   - workers harvest and deposit (account balances printed over ticks);
//!   - a town hall trains 5 soldiers to a rally point (5 sim bodies appear);
//!   - supply blocks training at the food cap (typed, observable);
//!   - an upkeep tier transition changes the income multiplier;
//!   - the SAME order stream replayed twice yields an identical state_hash;
//!   - a sabotaged cost field is rejected on load (negative control).
//!
//! Writes a path-free report.json (stable error codes only) plus the lowered artifact; the
//! artifact sha256 is recomputed independently with certutil by the runner.

use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    game_runtime::{
        EconomyOrder, EconomyRulesetAuthor, EconomyWorld, LoweredEconomyRuleset,
        LoweredResourceKind, LoweredResourceNode, LoweredStructureType, LoweredUnitEconomy,
        LoweredUpkeepTier, RallyPoint, verify_lowered_economy_ron,
    },
    nav::{
        BakedNavMesh, NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput,
        bake_navmesh,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const TICK_RATE: u32 = 16;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let out_dir = env::var("LITD_ECONOMY_FSV_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1785-economy-fsv"));
    fs::create_dir_all(&out_dir)?;

    // 1. Author + lower the mini base.
    let ruleset = author_mini_base()?;
    let canonical_ron = ruleset.to_canonical_ron()?;
    let artifact_path = out_dir.join("economy_ruleset.ron");
    fs::write(&artifact_path, canonical_ron.as_bytes())?;
    let artifact_bytes = fs::read(&artifact_path)?;
    let artifact_sha256 = sha256_hex(&artifact_bytes);

    // Round-trip verify (fail loud if the fingerprint does not reproduce).
    let loaded = verify_lowered_economy_ron(&canonical_ron, &ruleset.lowering_fingerprint)?;
    let roundtrip_ok = loaded.lowering_fingerprint == ruleset.lowering_fingerprint;

    // 2. Negative control: sabotage a lowered cost field, keep the original fingerprint.
    let negative_control_code = negative_control(&ruleset)?;

    // 3. Edge case A -- harvest and deposit.
    let harvest = run_harvest(ruleset.clone())?;

    // 4. Edge case B -- train 5 soldiers to a rally point.
    let training = run_training(ruleset.clone())?;

    // 5. Edge case C -- supply blocks training at cap + upkeep tier transition.
    let supply = run_supply_and_upkeep(ruleset.clone())?;

    // 6. Replay determinism -- same order stream twice.
    let stream = canonical_order_stream();
    let hash_a = run_order_stream(ruleset.clone(), &stream)?;
    let hash_b = run_order_stream(ruleset.clone(), &stream)?;
    let replay_hashes_match = hash_a == hash_b;

    let verdict = roundtrip_ok
        && negative_control_code == "LITD-REFLECT-FINGERPRINT-MISMATCH"
        && harvest["deposited_gold"].as_u64().unwrap_or(0) > 0
        && training["trained_bodies"].as_u64() == Some(5)
        && supply["blocked_code"] == "LITD-GAME-PACKAGE-INVALID-CONFIG"
        && supply["income_permille_before"].as_u64() > supply["income_permille_after"].as_u64()
        && replay_hashes_match;

    let report = json!({
        "issue": [1785, 837],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "lowering": {
            "fingerprint": ruleset.lowering_fingerprint,
            "roundtrip_ok": roundtrip_ok,
            "resource_count": ruleset.resources.len(),
            "unit_count": ruleset.units.len(),
            "structure_count": ruleset.structures.len(),
        },
        "negative_control": {
            "sabotaged_field": "units[soldier].gold_cost",
            "rejected_code": negative_control_code,
        },
        "edge_harvest": harvest,
        "edge_training": training,
        "edge_supply_upkeep": supply,
        "replay": {
            "state_hash_a": format!("{hash_a:016x}"),
            "state_hash_b": format!("{hash_b:016x}"),
            "hashes_match": replay_hashes_match,
        },
        "artifact": {
            "file": "economy_ruleset.ron",
            "bytes": artifact_bytes.len(),
            "sha256": artifact_sha256,
        },
    });

    let report_path = out_dir.join("report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;

    println!(
        "ISSUE1785_ECONOMY_RUNTIME_FSV verdict={}",
        report["verdict"]
    );
    println!("lowering_fingerprint={}", ruleset.lowering_fingerprint);
    println!("artifact_sha256={artifact_sha256}");
    println!("replay_state_hash={hash_a:016x} match={replay_hashes_match}");
    println!("report={}", report_path.display());
    if !verdict {
        return Err(Box::new(EngineError::GamePackageInvalidConfig {
            field: "economy.fsv.verdict".to_string(),
            reason: format!("FSV verdict RED: {report}"),
        }));
    }
    Ok(())
}

/// Author the mini base with real, editor-shaped costs (mirrors #818 UnitEconomy fields and
/// the #827 upkeep food caps). Thresholds are compact so a tier transition is exercisable.
fn author_mini_base() -> Result<LoweredEconomyRuleset, EngineError> {
    EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 5000,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: 1000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "no_upkeep".to_string(),
            food_min: 0,
            food_max: 4,
            income_numerator: 10,
            income_denominator: 10,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "low_upkeep".to_string(),
            food_min: 5,
            food_max: 9,
            income_numerator: 7,
            income_denominator: 10,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "high_upkeep".to_string(),
            food_min: 10,
            food_max: 100,
            income_numerator: 4,
            income_denominator: 10,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "peon".to_string(),
            gold_cost: 75,
            lumber_cost: 0,
            food_cost: 1,
            build_time_secs: 1,
            food_provided: 0,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "footman".to_string(),
            gold_cost: 135,
            lumber_cost: 0,
            food_cost: 2,
            build_time_secs: 1,
            food_provided: 0,
        })
        .node(LoweredResourceNode {
            node_id: "gold_mine".to_string(),
            resource_key: "gold".to_string(),
            cell_x: 5,
            cell_z: 0,
            amount_total: 5000,
            yield_per_trip: 10,
        })
        .structure(LoweredStructureType {
            struct_id: "town_hall".to_string(),
            gold_cost: 100,
            lumber_cost: 50,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 30,
            trains: vec!["peon".to_string(), "footman".to_string()],
        })
        .lower()
}

fn negative_control(ruleset: &LoweredEconomyRuleset) -> Result<String, EngineError> {
    let mut tampered = ruleset.clone();
    let idx = tampered
        .units
        .iter()
        .position(|u| u.unit_id == "footman")
        .expect("footman present");
    tampered.units[idx].gold_cost = 1; // free-ish soldier
    let tampered_ron = tampered.to_canonical_ron()?;
    match verify_lowered_economy_ron(&tampered_ron, &ruleset.lowering_fingerprint) {
        Ok(_) => Ok("ACCEPTED_SILENTLY".to_string()),
        Err(e) => Ok(e.code().to_string()),
    }
}

fn grid(width: i32, depth: i32) -> Result<BakedNavMesh, EngineError> {
    let mut cells = Vec::new();
    for z in 0..depth {
        for x in 0..width {
            cells.push(NavTerrainCellInput {
                id: format!("c_{x}_{z}"),
                x,
                z,
                height_fp: 0,
                flags: NavPathingFlags {
                    walkable: true,
                    buildable: true,
                    flyable: true,
                    blocked: false,
                },
            });
        }
    }
    bake_navmesh(&NavBakeInput::new(65536, cells, Vec::new()))
}

fn new_world(ruleset: LoweredEconomyRuleset) -> Result<EconomyWorld, EngineError> {
    EconomyWorld::new(
        ruleset,
        grid(10, 4)?,
        NavCellCoord::new(0, 0),
        NavCellCoord::new(0, 0),
        1785,
    )
}

fn run_harvest(ruleset: LoweredEconomyRuleset) -> Result<Value, EngineError> {
    let mut world = new_world(ruleset)?;
    world.add_worker(1, "gold_mine")?;
    let before = world.accounts().balance("gold");
    let mut samples = Vec::new();
    for _ in 0..8 {
        for _ in 0..25 {
            world.tick()?;
        }
        samples.push(json!({
            "tick": world.tick_count(),
            "gold": world.accounts().balance("gold"),
            "node_remaining": world.node_remaining("gold_mine"),
        }));
    }
    let after = world.accounts().balance("gold");
    Ok(json!({
        "before_gold": before,
        "after_gold": after,
        "deposited_gold": after - before,
        "node_drained": 5000 - world.node_remaining("gold_mine"),
        "samples": samples,
    }))
}

fn run_training(ruleset: LoweredEconomyRuleset) -> Result<Value, EngineError> {
    let mut world = new_world(ruleset)?;
    let id = world.place_structure("town_hall", 2, 1)?;
    for _ in 0..(TICK_RATE + 2) {
        world.tick()?;
    }
    let complete = world.structure(id).map(|s| s.complete).unwrap_or(false);
    world.set_rally(id, RallyPoint::Point { x: 8, z: 3 })?;
    let bodies_before = world.unit_body_count();
    for _ in 0..5 {
        world.train_unit(id, "footman")?;
    }
    // Each footman is build_time 1s = TICK_RATE ticks; drain the queue.
    for _ in 0..(TICK_RATE * 6) {
        world.tick()?;
    }
    let bodies_after = world.unit_body_count();
    Ok(json!({
        "structure_complete": complete,
        "rally": "point(8,3)",
        "bodies_before": bodies_before,
        "bodies_after": bodies_after,
        "trained_bodies": bodies_after - bodies_before,
    }))
}

fn run_supply_and_upkeep(ruleset: LoweredEconomyRuleset) -> Result<Value, EngineError> {
    let mut world = new_world(ruleset)?;
    let id = world.place_structure("town_hall", 2, 1)?;
    for _ in 0..(TICK_RATE + 2) {
        world.tick()?;
    }
    // town_hall provides 30 supply; footman food 2 each -> 15 fit, 16th blocks.
    let mut trained = 0_u32;
    let income_before = world.current_income_permille();
    loop {
        match world.train_unit(id, "footman") {
            Ok(()) => trained += 1,
            Err(e) => break gather_supply(world, id, trained, income_before, &e),
        }
        if trained > 40 {
            // safety: should have blocked well before this
            return Err(EngineError::GamePackageInvalidConfig {
                field: "economy.fsv.supply".to_string(),
                reason: "training never blocked at the supply cap".to_string(),
            });
        }
    }
}

fn gather_supply(
    world: EconomyWorld,
    _id: u32,
    trained: u32,
    income_before: u64,
    blocked: &EngineError,
) -> Result<Value, EngineError> {
    let income_after = world.current_income_permille();
    Ok(json!({
        "supply_cap": world.accounts().supply_cap(),
        "used_food": world.accounts().used_food(),
        "trained_before_block": trained,
        "blocked_code": blocked.code(),
        "blocked_message": blocked.to_string(),
        "income_permille_before": income_before,
        "income_permille_after": income_after,
        "tier_transition": income_before != income_after,
    }))
}

/// A fixed order stream used for the replay-determinism check.
fn canonical_order_stream() -> Vec<EconomyOrder> {
    let mut orders = vec![EconomyOrder::PlaceStructure {
        struct_id: "town_hall".to_string(),
        origin_x: 2,
        origin_z: 1,
    }];
    for _ in 0..(TICK_RATE + 2) {
        orders.push(EconomyOrder::Tick);
    }
    orders.push(EconomyOrder::SetRallyPoint {
        instance_id: 0,
        rally: RallyPoint::Point { x: 8, z: 3 },
    });
    orders.push(EconomyOrder::Grant {
        key: "gold".to_string(),
        amount: 250,
    });
    for _ in 0..5 {
        orders.push(EconomyOrder::TrainUnit {
            instance_id: 0,
            unit_id: "footman".to_string(),
        });
    }
    for _ in 0..(TICK_RATE * 6) {
        orders.push(EconomyOrder::Tick);
    }
    orders
}

fn run_order_stream(
    ruleset: LoweredEconomyRuleset,
    stream: &[EconomyOrder],
) -> Result<u64, EngineError> {
    let mut world = new_world(ruleset)?;
    world.add_worker(1, "gold_mine")?;
    for order in stream {
        world.apply_order(order);
    }
    Ok(world.state_hash())
}
