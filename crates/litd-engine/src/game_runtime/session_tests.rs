//! #1843: unit coverage for the composed match-runtime session. The in-module authoring
//! builders are used HERE (tests only) to assemble a manifest; production lowering goes
//! through the editor valve. Proves: build-from-manifest, deterministic tick, the fog<->
//! alliance and day/night<->sight seams, the save/load replay acid test, two-execution
//! replay determinism, and the typed negative controls.

use super::super::blueprint::{AuthoredUnit, MatchBlueprint};
use super::super::datasets::{
    RuntimeDatasetsManifest, lower_runtime_constants, validate_runtime_datasets,
};
use super::super::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredStructureType,
    LoweredUnitEconomy, LoweredUpkeepTier,
};
use super::super::players::{PlayerController, PlayerSlot};
use super::super::victory::{PlayerResult, VictoryCondition};
use super::{MatchRuntimeSession, SessionOrder};

const TICK_RATE: u32 = 4;
const GRID: u32 = 16;

fn sample_manifest() -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 10_000,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: 10_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "no_upkeep".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "worker".to_string(),
            gold_cost: 50,
            lumber_cost: 0,
            food_cost: 1,
            build_time_secs: 1,
            food_provided: 0,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "soldier".to_string(),
            gold_cost: 100,
            lumber_cost: 10,
            food_cost: 2,
            build_time_secs: 1,
            food_provided: 0,
        })
        .node(LoweredResourceNode {
            node_id: "gold_mine".to_string(),
            resource_key: "gold".to_string(),
            cell_x: 5,
            cell_z: 3,
            amount_total: 5_000,
            yield_per_trip: 10,
        })
        .structure(LoweredStructureType {
            struct_id: "town_hall".to_string(),
            gold_cost: 200,
            lumber_cost: 0,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["soldier".to_string(), "worker".to_string()],
        })
        .lower()
        .expect("lower economy");

    let slots = vec![
        PlayerSlot {
            id: 0,
            controller: PlayerController::Human,
            force: 0,
            color: 0,
            handicap_pct: 100,
        },
        PlayerSlot {
            id: 1,
            controller: PlayerController::Computer,
            force: 1,
            color: 1,
            handicap_pct: 100,
        },
    ];
    let mut blueprint = MatchBlueprint::new("mini_melee", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    // Structures (victory-relevant): one town hall each.
    blueprint.author_unit(AuthoredUnit {
        unit: 1,
        owner: 0,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 2,
        owner: 1,
        is_structure: true,
        capabilities: Vec::new(),
    });
    // Field units (vision emitters): one soldier each.
    blueprint.author_unit(AuthoredUnit {
        unit: 10,
        owner: 0,
        is_structure: false,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 11,
        owner: 1,
        is_structure: false,
        capabilities: Vec::new(),
    });
    let lowered_match = blueprint.lower().expect("lower match");

    let constants = lower_runtime_constants(TICK_RATE, 200, 6_000, 500, 1_000, GRID, GRID, 0x1843)
        .expect("lower constants");

    RuntimeDatasetsManifest::assemble(economy, lowered_match, constants).expect("assemble")
}

/// The scripted mini match, as a fixed order stream. Harvest -> place hall -> train ->
/// move a unit (fog) -> alliance change.
fn scripted_stream() -> Vec<SessionOrder> {
    let mut orders = vec![
        SessionOrder::AddWorker {
            worker_id: 1,
            node_id: "gold_mine".to_string(),
        },
        SessionOrder::PlaceStructure {
            struct_id: "town_hall".to_string(),
            x: 2,
            z: 2,
        },
    ];
    // Build the hall (build_time 1s * tick_rate).
    for _ in 0..TICK_RATE + 1 {
        orders.push(SessionOrder::Tick);
    }
    orders.push(SessionOrder::TrainUnit {
        instance_id: 0,
        unit_id: "soldier".to_string(),
    });
    for _ in 0..TICK_RATE + 1 {
        orders.push(SessionOrder::Tick);
    }
    // Move a field unit -> fog changes for it.
    orders.push(SessionOrder::MoveFieldUnit {
        field_id: 10,
        cell_x: 8,
        cell_z: 8,
    });
    orders.push(SessionOrder::Tick);
    // Ally the two players with shared vision -> the fog seam.
    orders.push(SessionOrder::SetAlliance {
        from: 0,
        to: 1,
        ally: true,
        shared_vision: true,
        shared_control: false,
    });
    orders.push(SessionOrder::Tick);
    orders
}

fn run_stream(orders: &[SessionOrder]) -> MatchRuntimeSession {
    let manifest = sample_manifest();
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    for order in orders {
        session.apply(order.clone()).expect("apply order");
    }
    session
}

#[test]
fn issue1843_manifest_validates_and_builds() {
    let manifest = sample_manifest();
    validate_runtime_datasets(&manifest).expect("datasets valid");
    let session = MatchRuntimeSession::from_manifest(&manifest).expect("session builds");
    // Two players, two field units seeded, both structures alive.
    assert_eq!(session.match_state().roster().player_count(), 2);
    assert!(session.field_unit(10).is_some());
    assert!(session.field_unit(11).is_some());
}

#[test]
fn issue1843_scripted_match_harvest_train_and_victory() {
    let mut session = run_stream(&scripted_stream());
    // Harvest credited gold above the placement/training spend floor.
    assert!(session.economy().accounts().balance("gold") > 0);
    // A soldier body was trained into the economy sim.
    assert!(session.economy().unit_body_count() >= 1);
    // Destroy the enemy town hall -> victory flips.
    assert_eq!(
        session.match_state().result(0).unwrap(),
        PlayerResult::Playing
    );
    session
        .apply(SessionOrder::DestroyUnit { unit: 2 })
        .expect("destroy enemy hall");
    assert_eq!(
        session.match_state().result(0).unwrap(),
        PlayerResult::Victory
    );
    assert_eq!(
        session.match_state().result(1).unwrap(),
        PlayerResult::Defeat
    );
}

#[test]
fn issue1843_daynight_shrinks_sight_and_fog() {
    // The #848 seam is live when day/night drives the ACTUAL fog visible-cell count, not
    // merely the reported sight radius. Midnight must reveal STRICTLY fewer cells than
    // noon for the same unit positions. Under the reviewer's constant-radius mutation
    // (recompute_fog ignores `sight_radius_input_mm` and uses `base_sight_radius_mm`) the
    // two counts collapse to equal and this strict inequality fails.
    let manifest = sample_manifest();
    let visible_at = |tick_in_day: u64| -> usize {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        session
            .apply(SessionOrder::SetTimeOfDay { tick_in_day })
            .expect("set time of day");
        // A Tick is what recomputes the fog from the new sight radius.
        session.apply(SessionOrder::Tick).expect("tick");
        session.fog().counts(0).expect("player 0 fog").2
    };
    // tick_in_day 0 == midnight (darkest), 100 == noon (day_length 200 -> permille 500).
    let midnight_visible = visible_at(0);
    let noon_visible = visible_at(100);
    // Midnight still sees the unit's own cell disc (radius clamps to >= 1 cell).
    assert!(midnight_visible > 0, "midnight sees at least the unit disc");
    assert!(
        noon_visible > midnight_visible,
        "noon must reveal strictly more fog cells than midnight \
         (day/night drives the sight radius): noon={noon_visible} midnight={midnight_visible}"
    );
    // The radius input itself must also strictly shrink at night (the seam's source value).
    let midnight_mm = {
        let mut s = MatchRuntimeSession::from_manifest(&manifest).expect("s");
        s.apply(SessionOrder::SetTimeOfDay { tick_in_day: 0 })
            .expect("midnight");
        s.time_of_day().sight_radius_input_mm()
    };
    let noon_mm = {
        let mut s = MatchRuntimeSession::from_manifest(&manifest).expect("s");
        s.apply(SessionOrder::SetTimeOfDay { tick_in_day: 100 })
            .expect("noon");
        s.time_of_day().sight_radius_input_mm()
    };
    assert!(
        noon_mm > midnight_mm,
        "noon sight radius input must strictly exceed midnight: noon={noon_mm} midnight={midnight_mm}"
    );
}

#[test]
fn issue1843_shared_vision_reveals_ally_units() {
    // The #836 <-> #847 seam: an ally's field unit must GENUINELY reveal cells for the
    // other player once shared vision is set. Under the reviewer's mutation (recompute_fog
    // uses an always-empty SharedVisionTable) the enemy-side cell stays masked for player 0
    // after the alliance, and the post-alliance assertion fails.
    let manifest = sample_manifest();
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

    // Player 1's field unit sits on the far side of the map; player 1 sees its own cell,
    // player 0 cannot reach it from its own unit's sight disc.
    let enemy = session.field_unit(11).expect("enemy field unit seeded");
    assert_eq!(enemy.owner, 1);
    assert!(
        session.fog().is_visible(1, enemy.cell_x, enemy.cell_z),
        "player 1 sees its own unit's cell"
    );
    assert!(
        !session.fog().is_visible(0, enemy.cell_x, enemy.cell_z),
        "before any alliance, player 0 does NOT see the enemy-side cell"
    );

    // Ally the two players WITH shared vision, then Tick to recompute the fog.
    session
        .apply(SessionOrder::SetAlliance {
            from: 0,
            to: 1,
            ally: true,
            shared_vision: true,
            shared_control: false,
        })
        .expect("set alliance");
    session.apply(SessionOrder::Tick).expect("tick");

    assert!(
        session.fog().is_visible(0, enemy.cell_x, enemy.cell_z),
        "after SetAlliance{{shared_vision:true}} player 0 sees the ally unit's cell"
    );
}

#[test]
fn issue1843_save_load_acid_test() {
    let manifest = sample_manifest();
    let stream = scripted_stream();
    // Uninterrupted run to the end.
    let mut original = MatchRuntimeSession::from_manifest(&manifest).expect("orig");
    let midpoint = stream.len() / 2;
    for order in &stream[..midpoint] {
        original.apply(order.clone()).expect("apply");
    }
    let save = original.capture_save().expect("capture");
    for order in &stream[midpoint..] {
        original.apply(order.clone()).expect("apply");
    }
    let continued = original.combined_state_hash();
    // Reload from the mid-match save and finish.
    let mut reloaded = MatchRuntimeSession::restore_from_save(&manifest, &save).expect("restore");
    for order in &stream[midpoint..] {
        reloaded.apply(order.clone()).expect("apply");
    }
    assert_eq!(continued, reloaded.combined_state_hash());
}

#[test]
fn issue1843_two_execution_replay_determinism() {
    let stream = scripted_stream();
    let a = run_stream(&stream).combined_state_hash();
    let b = run_stream(&stream).combined_state_hash();
    assert_eq!(a, b);
}

#[test]
fn issue1843_negative_control_tampered_dataset_rejected() {
    let mut manifest = sample_manifest();
    // Sabotage a unit cost without re-lowering the fingerprint.
    manifest.economy.units[0].gold_cost = 1;
    let error = validate_runtime_datasets(&manifest).expect_err("must reject tampered dataset");
    // Fingerprint mismatch is the loud, typed rejection.
    assert_eq!(error.code(), "LITD-REFLECT-FINGERPRINT-MISMATCH");
    // And the session refuses to build from it.
    assert!(MatchRuntimeSession::from_manifest(&manifest).is_err());
}

#[test]
fn issue1843_negative_control_sabotaged_save_rejected() {
    let manifest = sample_manifest();
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session.apply(SessionOrder::Tick).expect("tick");
    let mut save = session.capture_save().expect("capture");
    // Tamper the applied tick without fixing the fingerprint.
    save.applied_tick += 99;
    let error = save.validate().expect_err("must reject tampered save");
    assert_eq!(error.code(), "LITD_GAME_RUNTIME_FINGERPRINT");
    // Restore also rejects it.
    assert!(MatchRuntimeSession::restore_from_save(&manifest, &save).is_err());
}
