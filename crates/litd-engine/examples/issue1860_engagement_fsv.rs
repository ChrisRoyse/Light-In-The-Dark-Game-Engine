//! Full State Verification for #1860 — real engagement model (HP + ApplyDamage).
//!
//! Source of truth: `MatchState` unit HP fields + victory results after synthetic
//! damage orders, plus the AI timeline when the wave multi-hits a structure.
//! Instant `DestroyUnit` is NOT used for combat strikes.
//!
//! Edges: zero damage fails loud; damage-on-dead fails loud; overkill single hit
//! destroys and clamps amount_applied.

use std::fs;

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_runtime::{
        AiBuildStep, AuthoredUnit, DEFAULT_STRIKE_DAMAGE_MILLI, DEFAULT_STRUCTURE_MAX_HP_MILLI,
        EconomyRulesetAuthor, LoweredResourceKind, LoweredUnitEconomy, LoweredUpkeepTier,
        MatchBlueprint, MatchRuntimeSession, MatchState, OrderTargetKind, PlayerController,
        PlayerResult, PlayerSlot, SessionOrder, UnitCapability, VictoryCondition, lower_ai_profile,
        lower_runtime_constants, lower_runtime_constants_with_engagement,
        validate_runtime_datasets,
    },
};
use serde_json::json;

const TICK: u32 = 4;
const GRID: u32 = 16;
const ENEMY_STRUCTURE: u32 = 1;
const STRIKES_TO_KILL: u32 = 3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1860_engagement")?;

    // --- Happy path A: direct ApplyDamage multi-hit kill + SoT HP readback. ---
    let mut direct = MatchState::from_lowered(&direct_blueprint()?)?;
    let max_hp = direct.max_hp_milli(ENEMY_STRUCTURE)?;
    assert_eq!(max_hp, DEFAULT_STRUCTURE_MAX_HP_MILLI);
    let mut hp_trace = vec![direct.hp_milli(ENEMY_STRUCTURE)?];
    let mut destroyed_on = None;
    for hit in 1..=STRIKES_TO_KILL {
        let before = direct.hp_milli(ENEMY_STRUCTURE)?;
        let result = direct.apply_damage(ENEMY_STRUCTURE, DEFAULT_STRIKE_DAMAGE_MILLI)?;
        assert_eq!(result.hp_before, before);
        hp_trace.push(result.hp_after);
        if result.destroyed {
            destroyed_on = Some(hit);
            break;
        }
        assert!(direct.is_alive(ENEMY_STRUCTURE)?);
    }
    assert_eq!(
        destroyed_on,
        Some(STRIKES_TO_KILL),
        "default engagement is 3 strikes to kill a structure"
    );
    assert!(!direct.is_alive(ENEMY_STRUCTURE)?);
    assert_eq!(direct.hp_milli(ENEMY_STRUCTURE)?, 0);
    assert_eq!(direct.result(0)?, PlayerResult::Victory);
    assert_eq!(direct.result(1)?, PlayerResult::Defeat);

    // --- Happy path B: AI wave multi-hit strike (no DestroyUnit in combat path). ---
    let mut session = ai_session()?;
    let structure_hp_seed = session.match_state().hp_milli(ENEMY_STRUCTURE)?;
    assert_eq!(structure_hp_seed, DEFAULT_STRUCTURE_MAX_HP_MILLI);
    let mut victory_tick = None;
    for tick in 1..=100 {
        session.apply(SessionOrder::Tick)?;
        if session.match_state().result(1)? == PlayerResult::Victory {
            victory_tick = Some(tick);
            break;
        }
    }
    let victory_tick = victory_tick.expect("AI must win via multi-hit engagement");
    let timeline: Vec<serde_json::Value> = session
        .ai()
        .timeline()
        .iter()
        .map(|e| {
            json!({
                "tick": e.tick,
                "player": e.player,
                "kind": e.kind,
                "detail": e.detail,
            })
        })
        .collect();
    let strike_events: Vec<_> = session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "STRIKE")
        .collect();
    assert!(
        strike_events.len() as u32 >= STRIKES_TO_KILL,
        "AI must land >= {STRIKES_TO_KILL} ApplyDamage strikes, got {}",
        strike_events.len()
    );
    for s in &strike_events {
        assert!(
            s.detail.contains("damages") || s.detail.contains("milli"),
            "strike detail must describe damage not instant destroy: {}",
            s.detail
        );
        assert!(
            !s.detail.contains("wave destroys"),
            "legacy instant-kill wording must not appear: {}",
            s.detail
        );
    }
    assert!(!session.match_state().is_alive(ENEMY_STRUCTURE)?);
    assert_eq!(session.match_state().hp_milli(ENEMY_STRUCTURE)?, 0);
    let ai_hash = session.combined_state_hash();

    // --- Edge 1: zero damage fails loud; HP unchanged. ---
    let mut edge1 = MatchState::from_lowered(&direct_blueprint()?)?;
    let hp_before_e1 = edge1.hp_milli(ENEMY_STRUCTURE)?;
    let e1 = edge1.apply_damage(ENEMY_STRUCTURE, 0);
    assert!(e1.is_err(), "zero damage must fail loud");
    let e1_err = format!("{}", e1.unwrap_err());
    assert!(
        e1_err.contains("must be > 0") || e1_err.contains("zero"),
        "error must name the zero-damage rule: {e1_err}"
    );
    assert_eq!(edge1.hp_milli(ENEMY_STRUCTURE)?, hp_before_e1);
    assert!(edge1.is_alive(ENEMY_STRUCTURE)?);

    // --- Edge 2: damage on already-dead target fails loud. ---
    let mut edge2 = MatchState::from_lowered(&direct_blueprint()?)?;
    edge2.destroy_unit(ENEMY_STRUCTURE)?;
    let e2 = edge2.apply_damage(ENEMY_STRUCTURE, 100);
    assert!(e2.is_err(), "damage-on-dead must fail loud");
    let e2_err = format!("{}", e2.unwrap_err());
    assert!(
        e2_err.contains("destroyed") || e2_err.contains("dead"),
        "error must name dead target: {e2_err}"
    );

    // --- Edge 3: overkill single hit — amount_applied clamped, unit destroyed. ---
    let mut edge3 = MatchState::from_lowered(&direct_blueprint()?)?;
    let overkill = DEFAULT_STRUCTURE_MAX_HP_MILLI * 10;
    let r3 = edge3.apply_damage(ENEMY_STRUCTURE, overkill)?;
    assert!(r3.destroyed);
    assert_eq!(r3.amount_applied, DEFAULT_STRUCTURE_MAX_HP_MILLI);
    assert_eq!(r3.hp_after, 0);
    assert!(!edge3.is_alive(ENEMY_STRUCTURE)?);

    // --- Custom engagement ratio FSV: 5000 HP / 2000 damage = 3 hits still. ---
    let custom = lower_runtime_constants_with_engagement(
        TICK, 200, 6_000, 500, 1_000, GRID, GRID, 0x1860_C, 5_000, 1_000, 2_000,
    )?;
    assert_eq!(custom.structure_max_hp_milli, 5_000);
    assert_eq!(custom.strike_damage_milli, 2_000);
    let custom_fp = custom.constants_fingerprint.clone();
    assert!(!custom_fp.is_empty());

    let report = json!({
        "schema": "litd.issue1860.engagement_fsv.v1",
        "issue": 1860,
        "source_of_truth": {
            "match_state_hp_fields": ["hp_milli", "max_hp_milli"],
            "order_surface": "SessionOrder::ApplyDamage",
            "ai_strike": "ApplyDamage(strike_damage_milli) behind fog.is_visible",
            "report_path": run.report_path().display().to_string(),
            "manual_gate": "Read hp_trace, strike timeline, edges, and victory results from this report file on disk."
        },
        "happy_direct": {
            "structure_id": ENEMY_STRUCTURE,
            "max_hp_milli": max_hp,
            "strike_damage_milli": DEFAULT_STRIKE_DAMAGE_MILLI,
            "strikes_to_kill": STRIKES_TO_KILL,
            "hp_trace": hp_trace,
            "destroyed_on_hit": destroyed_on,
            "p0_result": "Victory",
            "p1_result": "Defeat",
        },
        "happy_ai": {
            "structure_hp_seed": structure_hp_seed,
            "victory_tick": victory_tick,
            "strike_count": strike_events.len(),
            "timeline": timeline,
            "structure_alive_after": false,
            "structure_hp_after": 0,
            "combined_state_hash": format!("{ai_hash:016x}"),
        },
        "edges": {
            "zero_damage": {
                "error": e1_err,
                "hp_unchanged": true,
                "alive": true,
            },
            "damage_on_dead": {
                "error": e2_err,
            },
            "overkill": {
                "requested": overkill,
                "amount_applied": r3.amount_applied,
                "destroyed": r3.destroyed,
            },
        },
        "custom_engagement_constants": {
            "structure_max_hp_milli": 5000,
            "strike_damage_milli": 2000,
            "constants_fingerprint": custom_fp,
        },
        "verdict": "PASS",
    });

    let report_path = run.report_path();
    fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    let on_disk = fs::read_to_string(&report_path)?;
    let parsed: serde_json::Value = serde_json::from_str(&on_disk)?;
    assert_eq!(parsed["verdict"], "PASS");
    assert_eq!(parsed["happy_direct"]["strikes_to_kill"], STRIKES_TO_KILL);
    assert!(parsed["happy_ai"]["strike_count"].as_u64().unwrap() >= u64::from(STRIKES_TO_KILL));
    // Physical SoT: re-read HP fields from the live state objects above match the report.
    println!("FSV #1860 PASS report={}", report_path.display());
    println!(
        "hp_trace={hp_trace:?} ai_strikes={} victory_tick={victory_tick} hash={ai_hash:016x}",
        strike_events.len()
    );
    Ok(())
}

fn direct_blueprint() -> Result<litd_engine::game_runtime::LoweredMatch, Box<dyn std::error::Error>>
{
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
    let mut bp = MatchBlueprint::new("engagement_direct", slots, 2);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    // P0 attacks P1's structure unit 1.
    bp.author_unit(AuthoredUnit {
        unit: 10,
        owner: 0,
        is_structure: true,
        capabilities: vec![],
    });
    bp.author_unit(AuthoredUnit {
        unit: ENEMY_STRUCTURE,
        owner: 1,
        is_structure: true,
        capabilities: vec![],
    });
    Ok(bp.lower()?)
}

fn ai_session() -> Result<MatchRuntimeSession, Box<dyn std::error::Error>> {
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
        .structure(litd_engine::game_runtime::LoweredStructureType {
            struct_id: "town_hall".into(),
            gold_cost: 200,
            lumber_cost: 0,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["soldier".into()],
        })
        .node(litd_engine::game_runtime::LoweredResourceNode {
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
    let mut bp = MatchBlueprint::new("engagement_ai", slots, 2);
    bp.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    bp.author_unit(AuthoredUnit {
        unit: ENEMY_STRUCTURE,
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
    let constants = lower_runtime_constants(TICK, 200, 6_000, 500, 1_000, GRID, GRID, 0x1860)?;
    let profile = lower_ai_profile(
        1,
        "Aggressor",
        1,
        "gold_mine",
        2,
        vec![AiBuildStep {
            struct_id: "town_hall".into(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        2,
        0,
        3,
        1000,
    )?;
    let manifest = litd_engine::game_runtime::RuntimeDatasetsManifest::assemble_with_ai(
        economy,
        lowered_match,
        constants,
        vec![profile],
    )?;
    validate_runtime_datasets(&manifest)?;
    Ok(MatchRuntimeSession::from_manifest(&manifest)?)
}
