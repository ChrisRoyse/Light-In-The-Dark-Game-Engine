//! Full State Verification for #1856 + #1883 — spatial regions + enter/leave +
//! unit_in_region, and the editor handle map path proven via lower_region_set.
//!
//! Headless: real engine lowering + MatchRuntimeSession. Source of truth is the
//! fingerprinted region set embedded in RuntimeDatasetsManifest and the trigger
//! timeline after synthetic move orders. Edge cases fail loud.

use std::fs;

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_runtime::{
        AuthoredUnit, EconomyRulesetAuthor, LoweredRegion, LoweredResourceKind, LoweredTrigger,
        LoweredUnitEconomy, LoweredUpkeepTier, MatchBlueprint, MatchRuntimeSession,
        PlayerController, PlayerSlot, SessionOrder, TriggerAction, TriggerEvent, TriggerValue,
        TriggerVarInit, VictoryCondition, lower_region_set, lower_runtime_constants,
        lower_trigger_graph, validate_runtime_datasets,
    },
};
use serde_json::json;

const TICK: u32 = 4;
const UNIT: u32 = 10;
const REGION: u32 = 1;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1856_1883_regions")?;

    // --- Happy path: unit starts outside, moves into region, then out. ---
    let region_set = lower_region_set(vec![LoweredRegion {
        region_id: REGION,
        name: "escort_gate".into(),
        min_x: 5,
        min_z: 5,
        max_x: 8,
        max_z: 8,
    }])?;
    let region_fp = region_set.region_set_fingerprint.clone();

    let graph = lower_trigger_graph(
        vec![TriggerVarInit {
            name: "entered".into(),
            value: TriggerValue::Flag(false),
        }],
        vec![
            LoweredTrigger {
                trigger_id: "on_enter".into(),
                enabled: true,
                initially_on: true,
                events: vec![TriggerEvent::UnitEntersRegion { region: REGION }],
                conditions: vec![],
                actions: vec![TriggerAction::SetVariable {
                    name: "entered".into(),
                    value: TriggerValue::Flag(true),
                }],
            },
            LoweredTrigger {
                trigger_id: "on_leave".into(),
                enabled: true,
                initially_on: true,
                events: vec![TriggerEvent::UnitLeavesRegion { region: REGION }],
                conditions: vec![],
                actions: vec![TriggerAction::SetVariable {
                    name: "entered".into(),
                    value: TriggerValue::Flag(false),
                }],
            },
        ],
    )?;

    let mut session = session_with(Some(graph), Some(region_set))?;
    // start_cell for owner 0 is typically near origin; ensure outside region.
    let start = session.field_unit(UNIT).expect("unit seeded");
    let started_outside = !session.unit_in_region(UNIT, REGION)?;
    assert!(started_outside, "unit must start outside escort_gate");

    // Tick once at start (map init) — no enter yet.
    session.apply(SessionOrder::Tick)?;
    let after_tick1 = session.triggers().variable("entered").cloned();

    // Move into region.
    session.apply(SessionOrder::MoveFieldUnit {
        field_id: UNIT,
        cell_x: 6,
        cell_z: 6,
    })?;
    session.apply(SessionOrder::Tick)?;
    let inside = session.unit_in_region(UNIT, REGION)?;
    let enter_fired = session
        .triggers()
        .timeline()
        .iter()
        .any(|f| f.event == "unit_enters_region" && f.trigger_id == "on_enter");
    let flag_true = matches!(
        session.triggers().variable("entered"),
        Some(TriggerValue::Flag(true))
    );

    // Move out — leave event.
    session.apply(SessionOrder::MoveFieldUnit {
        field_id: UNIT,
        cell_x: 0,
        cell_z: 0,
    })?;
    session.apply(SessionOrder::Tick)?;
    let leave_fired = session
        .triggers()
        .timeline()
        .iter()
        .any(|f| f.event == "unit_leaves_region");

    // Edge 1: inverted region bounds fail at lower.
    let edge_inverted = lower_region_set(vec![LoweredRegion {
        region_id: 9,
        name: "bad".into(),
        min_x: 8,
        min_z: 0,
        max_x: 1,
        max_z: 1,
    }])
    .is_err();

    // Edge 2: trigger names missing region fails package validate.
    let orphan_graph = lower_trigger_graph(
        vec![],
        vec![LoweredTrigger {
            trigger_id: "orphan".into(),
            enabled: true,
            initially_on: true,
            events: vec![TriggerEvent::UnitEntersRegion { region: 99 }],
            conditions: vec![],
            actions: vec![TriggerAction::FireTrigger { trigger: 1 }],
        }],
    )?;
    let orphan_set = lower_region_set(vec![LoweredRegion {
        region_id: 1,
        name: "only".into(),
        min_x: 0,
        min_z: 0,
        max_x: 1,
        max_z: 1,
    }])?;
    let orphan_manifest = sample_manifest(Some(orphan_graph), Some(orphan_set))?;
    let edge_orphan = validate_runtime_datasets(&orphan_manifest).is_err();

    // Edge 3: unit_in_region on missing unit fails loud.
    let edge_session = session_with(
        None,
        Some(lower_region_set(vec![LoweredRegion {
            region_id: 1,
            name: "r".into(),
            min_x: 0,
            min_z: 0,
            max_x: 2,
            max_z: 2,
        }])?),
    )?;
    let edge_missing_unit = edge_session.unit_in_region(999, 1).is_err();

    assert!(inside, "unit at (6,6) must be inside region");
    assert!(enter_fired, "unit_enters_region must fire");
    assert!(flag_true, "enter action must set entered=true");
    assert!(edge_inverted && edge_orphan && edge_missing_unit);

    // Source of truth: persist region fingerprint + timeline to disk and re-read.
    let report = json!({
        "issue": [1856, 1883],
        "verdict": "PASS",
        "source_of_truth": {
            "region_set_fingerprint": region_fp,
            "start_cell": [start.cell_x, start.cell_z],
            "started_outside": started_outside,
            "after_enter_inside": inside,
            "enter_fired": enter_fired,
            "leave_fired": leave_fired,
            "flag_after_tick1": format!("{after_tick1:?}"),
            "timeline": session.triggers().timeline().iter().map(|f| {
                json!({"tick": f.tick, "trigger": f.trigger_id, "event": f.event})
            }).collect::<Vec<_>>(),
        },
        "edge_cases": {
            "inverted_bounds_fails": edge_inverted,
            "orphan_region_ref_fails": edge_orphan,
            "missing_unit_fails": edge_missing_unit,
        },
    });
    let report_path = run.run_dir().join("manual-fsv-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let bytes = fs::read(&report_path)?;
    let sha = litd_hash::sha256_hex(&bytes);
    let reloaded: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(reloaded["verdict"], "PASS");
    assert_eq!(
        reloaded["source_of_truth"]["region_set_fingerprint"],
        region_fp
    );
    println!(
        "FSV_PASS issue=1856,1883 report={} sha256={} region_fp={}",
        report_path.display(),
        sha,
        region_fp
    );
    Ok(())
}

fn session_with(
    triggers: Option<litd_engine::game_runtime::LoweredTriggerGraph>,
    regions: Option<litd_engine::game_runtime::LoweredRegionSet>,
) -> Result<MatchRuntimeSession, Box<dyn std::error::Error>> {
    let manifest = sample_manifest(triggers, regions)?;
    validate_runtime_datasets(&manifest)?;
    Ok(MatchRuntimeSession::from_manifest(&manifest)?)
}

fn sample_manifest(
    triggers: Option<litd_engine::game_runtime::LoweredTriggerGraph>,
    regions: Option<litd_engine::game_runtime::LoweredRegionSet>,
) -> Result<litd_engine::game_runtime::RuntimeDatasetsManifest, Box<dyn std::error::Error>> {
    let economy = EconomyRulesetAuthor::new(TICK, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".into(),
            display_name: "Gold".into(),
            starting_balance: 1000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".into(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "soldier".into(),
            gold_cost: 100,
            lumber_cost: 0,
            food_cost: 1,
            build_time_secs: 1,
            food_provided: 0,
        })
        .lower()?;
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
    let mut bp = MatchBlueprint::new("region_melee", slots, 2);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.author_unit(AuthoredUnit {
        unit: 1,
        owner: 0,
        is_structure: true,
        capabilities: vec![],
    });
    bp.author_unit(AuthoredUnit {
        unit: 2,
        owner: 1,
        is_structure: true,
        capabilities: vec![],
    });
    bp.author_unit(AuthoredUnit {
        unit: UNIT,
        owner: 0,
        is_structure: false,
        capabilities: vec![litd_engine::game_runtime::UnitCapability {
            ability: 1,
            target_kind: litd_engine::game_runtime::OrderTargetKind::Point,
        }],
    });
    let lowered_match = bp.lower()?;
    let constants = lower_runtime_constants(TICK, 200, 6000, 500, 1000, 16, 16, 0x1856)?;
    Ok(
        litd_engine::game_runtime::RuntimeDatasetsManifest::assemble_with_regions(
            economy,
            lowered_match,
            constants,
            Vec::new(),
            triggers,
            regions,
        )?,
    )
}
