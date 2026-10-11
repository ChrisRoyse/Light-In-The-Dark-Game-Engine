//! Unit tests for the base-building & economy runtime (#1785/#837).

use crate::nav::{
    BakedNavMesh, NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput, bake_navmesh,
};

use super::economy::{apply_upkeep_multiplier, upkeep_tier_for};
use super::lowered::{
    EconomyRulesetAuthor, LoweredEconomyRuleset, LoweredResourceKind, LoweredResourceNode,
    LoweredStructureType, LoweredUnitEconomy, LoweredUpkeepTier, verify_lowered_economy_ron,
};
use super::orders::EconomyOrder;
use super::structures::RallyPoint;
use super::world::EconomyWorld;

fn grid(width: i32, depth: i32) -> BakedNavMesh {
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
    bake_navmesh(&NavBakeInput::new(65536, cells, Vec::new())).expect("bake grid")
}

fn ruleset() -> LoweredEconomyRuleset {
    EconomyRulesetAuthor::new(16, 8, 1)
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
            name: "no".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 10,
            income_denominator: 10,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "high".to_string(),
            food_min: 51,
            food_max: 100,
            income_numerator: 4,
            income_denominator: 10,
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
            node_id: "mine".to_string(),
            resource_key: "gold".to_string(),
            cell_x: 4,
            cell_z: 0,
            amount_total: 1000,
            yield_per_trip: 10,
        })
        .structure(LoweredStructureType {
            struct_id: "town_hall".to_string(),
            gold_cost: 100,
            lumber_cost: 50,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["footman".to_string()],
        })
        .lower()
        .expect("lower ruleset")
}

fn world() -> EconomyWorld {
    EconomyWorld::new(
        ruleset(),
        grid(8, 4),
        NavCellCoord::new(0, 0),
        NavCellCoord::new(0, 0),
        1785,
    )
    .expect("world")
}

#[test]
fn fingerprint_roundtrips_and_rejects_tamper() {
    let rs = ruleset();
    let ron = rs.to_canonical_ron().expect("ron");
    verify_lowered_economy_ron(&ron, &rs.lowering_fingerprint).expect("verify clean");
    // Sabotage a cost field: re-encode with a lowered gold_cost but the ORIGINAL fingerprint.
    let mut tampered = rs.clone();
    tampered.units[0].gold_cost = 1;
    let tampered_ron = tampered.to_canonical_ron().expect("ron");
    assert_ne!(tampered_ron, ron);
    let err = verify_lowered_economy_ron(&tampered_ron, &rs.lowering_fingerprint);
    assert!(err.is_err(), "tampered cost must be rejected");
}

#[test]
fn duration_products_reject_during_authored_lowering() {
    let resource = || LoweredResourceKind {
        key: "gold".to_string(),
        display_name: "Gold".to_string(),
        starting_balance: 1_000,
    };
    let unit_error = EconomyRulesetAuthor::new(2, 1, 1)
        .resource(resource())
        .unit(LoweredUnitEconomy {
            unit_id: "overflow_unit".to_string(),
            gold_cost: 1,
            lumber_cost: 0,
            food_cost: 0,
            build_time_secs: u32::MAX,
            food_provided: 0,
        })
        .lower()
        .expect_err("unit duration product must reject");
    assert!(unit_error.to_string().contains("unit.build_time_secs"));

    let structure_error = EconomyRulesetAuthor::new(2, 1, 1)
        .resource(resource())
        .structure(LoweredStructureType {
            struct_id: "overflow_structure".to_string(),
            gold_cost: 1,
            lumber_cost: 0,
            build_time_secs: u32::MAX,
            footprint_w: 1,
            footprint_h: 1,
            food_provided: 0,
            trains: Vec::new(),
        })
        .lower()
        .expect_err("structure duration product must reject");
    assert!(
        structure_error
            .to_string()
            .contains("structure.build_time_secs")
    );
}

#[test]
fn upkeep_tier_scales_income() {
    let rs = ruleset();
    let no = upkeep_tier_for(&rs.upkeep_tiers, 10).expect("tier");
    assert_eq!(apply_upkeep_multiplier(100, no).expect("no upkeep"), 100);
    let high = upkeep_tier_for(&rs.upkeep_tiers, 80).expect("tier");
    assert_eq!(apply_upkeep_multiplier(100, high).expect("high upkeep"), 40);
}

#[test]
fn worker_harvests_and_deposits() {
    let mut w = world();
    w.add_worker(1, "mine").expect("worker");
    let start_gold = w.accounts().balance("gold");
    for _ in 0..200 {
        w.tick().expect("tick");
    }
    assert!(
        w.accounts().balance("gold") > start_gold,
        "worker deposited gold"
    );
    assert!(w.node_remaining("mine") < 1000, "node was drained");
}

#[test]
fn place_charges_and_cancel_refunds() {
    let mut w = world();
    let gold0 = w.accounts().balance("gold");
    let lumber0 = w.accounts().balance("lumber");
    let id = w.place_structure("town_hall", 2, 1).expect("place");
    assert_eq!(w.accounts().balance("gold"), gold0 - 100);
    assert_eq!(w.accounts().balance("lumber"), lumber0 - 50);
    w.cancel_structure(id).expect("cancel");
    assert_eq!(w.accounts().balance("gold"), gold0);
    assert_eq!(w.accounts().balance("lumber"), lumber0);
}

#[test]
fn unbuildable_footprint_is_rejected() {
    let mut w = world();
    // Origin far outside the 8x4 grid: no such cells.
    let err = w.place_structure("town_hall", 40, 40);
    assert!(err.is_err(), "off-grid placement must fail");
}

#[test]
fn training_blocks_at_supply_cap() {
    let mut w = world();
    let id = w.place_structure("town_hall", 2, 1).expect("place");
    // Complete the build so supply (food_provided=20) is applied.
    for _ in 0..40 {
        w.tick().expect("tick");
    }
    assert!(w.structure(id).expect("s").complete);
    // footman food_cost=2, cap=20 -> 10 succeed, 11th blocks.
    let mut trained = 0;
    for _ in 0..15 {
        if w.train_unit(id, "footman").is_ok() {
            trained += 1;
        }
    }
    assert_eq!(trained, 10, "supply cap of 20 food / 2 each = 10 units");
    assert!(w.train_unit(id, "footman").is_err(), "11th blocks at cap");
}

#[test]
fn replay_is_deterministic() {
    let stream = order_stream();
    let hash_a = run_stream(&stream);
    let hash_b = run_stream(&stream);
    assert_eq!(
        hash_a, hash_b,
        "identical order streams reproduce identical state_hash"
    );
}

fn order_stream() -> Vec<EconomyOrder> {
    let mut orders = vec![EconomyOrder::PlaceStructure {
        struct_id: "town_hall".to_string(),
        origin_x: 2,
        origin_z: 1,
    }];
    for _ in 0..40 {
        orders.push(EconomyOrder::Tick);
    }
    orders.push(EconomyOrder::SetRallyPoint {
        instance_id: 0,
        rally: RallyPoint::Point { x: 6, z: 3 },
    });
    for _ in 0..5 {
        orders.push(EconomyOrder::TrainUnit {
            instance_id: 0,
            unit_id: "footman".to_string(),
        });
    }
    for _ in 0..40 {
        orders.push(EconomyOrder::Tick);
    }
    orders
}

fn run_stream(stream: &[EconomyOrder]) -> u64 {
    let mut w = world();
    w.add_worker(1, "mine").expect("worker");
    for order in stream {
        w.apply_order(order);
    }
    w.state_hash()
}
