//! Full State Verification for #1858 — scheduled multi-wave AI assaults.
//!
//! Source of truth: AI timeline (WAVE_SENT / WAVE_ADVANCE / STRIKE) + profile
//! fingerprint + MatchState after synthetic multi-wave schedule.

use std::fs;

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_runtime::{
        AiBuildStep, AiWaveStep, AuthoredUnit, EconomyRulesetAuthor, LoweredResourceKind,
        LoweredResourceNode, LoweredStructureType, LoweredUnitEconomy, LoweredUpkeepTier,
        MatchBlueprint, MatchRuntimeSession, OrderTargetKind, PlayerController, PlayerSlot,
        SessionOrder, UnitCapability, VictoryCondition, lower_ai_profile_with_waves,
        lower_runtime_constants, validate_runtime_datasets,
    },
};
use serde_json::json;

const TICK: u32 = 4;
const GRID: u32 = 16;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1858_multi_wave_ai")?;

    let waves = vec![
        AiWaveStep {
            at_tick: 0,
            size: 1,
            regroup_ticks: 2,
        },
        AiWaveStep {
            at_tick: 0,
            size: 1,
            regroup_ticks: 2,
        },
        AiWaveStep {
            at_tick: 0,
            size: 1,
            regroup_ticks: 2,
        },
    ];
    let profile = lower_ai_profile_with_waves(
        1,
        "ThreeRaid",
        1,
        "gold_mine",
        1,
        vec![AiBuildStep {
            struct_id: "town_hall".into(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        1,
        waves.clone(),
        0,
        3,
        1000,
    )?;
    assert!(profile.has_wave_schedule());
    assert_eq!(profile.waves.len(), 3);
    let profile_fp = profile.profile_fingerprint.clone();

    let mut session = session_with(profile)?;
    let mut timeline_snap = Vec::new();
    for _ in 0..100 {
        session.apply(SessionOrder::Tick)?;
        timeline_snap = session
            .ai()
            .timeline()
            .iter()
            .map(|e| {
                json!({
                    "tick": e.tick,
                    "kind": e.kind,
                    "detail": e.detail,
                })
            })
            .collect();
        let sents = session
            .ai()
            .timeline()
            .iter()
            .filter(|e| e.kind == "WAVE_SENT")
            .count();
        if sents >= 3 {
            break;
        }
    }

    let wave_sents: Vec<_> = session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "WAVE_SENT")
        .collect();
    let advances: Vec<_> = session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "WAVE_ADVANCE")
        .collect();
    assert!(
        wave_sents.len() >= 3,
        "expected >=3 WAVE_SENT, got {}",
        wave_sents.len()
    );
    assert!(
        advances.len() >= 2,
        "expected >=2 WAVE_ADVANCE, got {}",
        advances.len()
    );
    for (i, w) in wave_sents.iter().take(3).enumerate() {
        assert!(
            w.detail.contains(&format!("wave {i}")),
            "WAVE_SENT[{i}] missing wave index: {}",
            w.detail
        );
    }
    // Strikes still multi-hit engagement (#1860), not DestroyUnit.
    let strikes: Vec<_> = session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "STRIKE")
        .collect();
    assert!(!strikes.is_empty(), "waves must still strike");
    for s in &strikes {
        assert!(
            s.detail.contains("damages") || s.detail.contains("milli"),
            "strike must be ApplyDamage path: {}",
            s.detail
        );
    }
    let hash = session.combined_state_hash();

    // --- Edge 1: empty schedule = legacy single wave (no WAVE_ADVANCE). ---
    let legacy = lower_ai_profile_with_waves(
        1,
        "Legacy",
        1,
        "gold_mine",
        1,
        vec![AiBuildStep {
            struct_id: "town_hall".into(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        2,
        Vec::new(),
        0,
        3,
        1000,
    )?;
    assert!(!legacy.has_wave_schedule());
    let mut legacy_session = session_with(legacy)?;
    for _ in 0..40 {
        legacy_session.apply(SessionOrder::Tick)?;
    }
    let legacy_advances = legacy_session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "WAVE_ADVANCE")
        .count();
    assert_eq!(legacy_advances, 0, "legacy schedule must not WAVE_ADVANCE");
    let legacy_sents = legacy_session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "WAVE_SENT")
        .count();
    assert!(legacy_sents >= 1, "legacy still sends one wave");

    // --- Edge 2: decreasing at_tick fails loud at lower time. ---
    let e2 = lower_ai_profile_with_waves(
        1,
        "BadOrder",
        1,
        "gold_mine",
        1,
        vec![AiBuildStep {
            struct_id: "town_hall".into(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        1,
        vec![
            AiWaveStep {
                at_tick: 20,
                size: 1,
                regroup_ticks: 0,
            },
            AiWaveStep {
                at_tick: 5,
                size: 1,
                regroup_ticks: 0,
            },
        ],
        0,
        3,
        1000,
    );
    assert!(e2.is_err());
    let e2_msg = format!("{}", e2.unwrap_err());
    assert!(e2_msg.contains("non-decreasing") || e2_msg.contains("at_tick"));

    // --- Edge 3: zero wave size fails loud. ---
    let e3 = lower_ai_profile_with_waves(
        1,
        "BadSize",
        1,
        "gold_mine",
        1,
        vec![AiBuildStep {
            struct_id: "town_hall".into(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        1,
        vec![AiWaveStep {
            at_tick: 0,
            size: 0,
            regroup_ticks: 0,
        }],
        0,
        3,
        1000,
    );
    assert!(e3.is_err());
    let e3_msg = format!("{}", e3.unwrap_err());
    assert!(e3_msg.contains("size"));

    let report = json!({
        "schema": "litd.issue1858.multi_wave_ai_fsv.v1",
        "issue": 1858,
        "source_of_truth": {
            "ai_timeline_kinds": ["WAVE_SENT", "WAVE_ADVANCE", "STRIKE"],
            "profile_fingerprint": profile_fp,
            "report_path": run.report_path().display().to_string(),
            "manual_gate": "Read WAVE_SENT/WAVE_ADVANCE counts and details from this on-disk report."
        },
        "happy": {
            "schedule_len": 3,
            "wave_sent_count": wave_sents.len(),
            "wave_advance_count": advances.len(),
            "strike_count": strikes.len(),
            "wave_sent_details": wave_sents.iter().map(|e| e.detail.clone()).collect::<Vec<_>>(),
            "wave_advance_details": advances.iter().map(|e| e.detail.clone()).collect::<Vec<_>>(),
            "combined_state_hash": format!("{hash:016x}"),
            "timeline": timeline_snap,
        },
        "edges": {
            "legacy_empty_schedule": {
                "wave_advance_count": legacy_advances,
                "wave_sent_count": legacy_sents,
            },
            "decreasing_at_tick": { "error": e2_msg },
            "zero_wave_size": { "error": e3_msg },
        },
        "verdict": "PASS",
    });

    let report_path = run.report_path();
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    let on_disk: serde_json::Value = serde_json::from_str(&fs::read_to_string(&report_path)?)?;
    assert_eq!(on_disk["verdict"], "PASS");
    assert!(on_disk["happy"]["wave_sent_count"].as_u64().unwrap() >= 3);
    println!(
        "FSV #1858 PASS report={} wave_sents={} advances={} hash={hash:016x}",
        report_path.display(),
        wave_sents.len(),
        advances.len()
    );
    Ok(())
}

fn session_with(
    profile: litd_engine::game_runtime::LoweredAiProfile,
) -> Result<MatchRuntimeSession, Box<dyn std::error::Error>> {
    let economy = EconomyRulesetAuthor::new(TICK, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".into(),
            display_name: "Gold".into(),
            starting_balance: 10_000,
        })
        .resource(LoweredResourceKind {
            key: "lumber".into(),
            display_name: "Lumber".into(),
            starting_balance: 10_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".into(),
            food_min: 0,
            food_max: 80,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "soldier".into(),
            gold_cost: 100,
            lumber_cost: 10,
            food_cost: 2,
            build_time_secs: 1,
            food_provided: 0,
        })
        .structure(LoweredStructureType {
            struct_id: "town_hall".into(),
            gold_cost: 200,
            lumber_cost: 0,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["soldier".into()],
        })
        .node(LoweredResourceNode {
            node_id: "gold_mine".into(),
            resource_key: "gold".into(),
            cell_x: 5,
            cell_z: 3,
            amount_total: 5_000,
            yield_per_trip: 10,
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
    let move_cap = vec![UnitCapability {
        ability: 1,
        target_kind: OrderTargetKind::Point,
    }];
    let mut bp = MatchBlueprint::new("multi_wave_melee", slots, 2);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.author_unit(AuthoredUnit {
        unit: 1,
        owner: 0,
        is_structure: true,
        capabilities: vec![],
    });
    bp.author_unit(AuthoredUnit {
        unit: 10,
        owner: 0,
        is_structure: false,
        capabilities: move_cap.clone(),
    });
    // Two AI structures so match doesn't end mid-schedule on first kill.
    bp.author_unit(AuthoredUnit {
        unit: 2,
        owner: 1,
        is_structure: true,
        capabilities: vec![],
    });
    bp.author_unit(AuthoredUnit {
        unit: 3,
        owner: 1,
        is_structure: true,
        capabilities: vec![],
    });
    bp.author_unit(AuthoredUnit {
        unit: 11,
        owner: 1,
        is_structure: false,
        capabilities: move_cap,
    });
    let lowered_match = bp.lower()?;
    let constants = lower_runtime_constants(TICK, 200, 6_000, 500, 1_000, GRID, GRID, 0x1858)?;
    let manifest = litd_engine::game_runtime::RuntimeDatasetsManifest::assemble_with_ai(
        economy,
        lowered_match,
        constants,
        vec![profile],
    )?;
    validate_runtime_datasets(&manifest)?;
    Ok(MatchRuntimeSession::from_manifest(&manifest)?)
}
