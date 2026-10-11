//! Headless FSV for the session-orders / trigger-actions cluster:
//! #1855 effects, #1857 fog reveal/conceal, #1854 cinematic SimGating,
//! #1859 spawn-unit-at-cell, #1861 camera-follow + weather.
//!
//! Source of truth: a live `MatchRuntimeSession` built from a real lowered
//! runtime manifest. Applies a deterministic order stream, reads state back
//! (quest progress, fog visibility, spawned field unit, sim gate, weather,
//! camera follow binding), proves two-execution replay, and fails-loud edges.
//! Writes path-free report.json under target/fsv/ and prints its sha256.
//!
//! Run:
//!   cargo run -p litd-engine --features fsv --example issue1855_session_orders_cluster_fsv

use std::{env, fs, path::PathBuf};

use litd_engine::game_runtime::blueprint::{AuthoredUnit, MatchBlueprint};
use litd_engine::game_runtime::datasets::{RuntimeDatasetsManifest, lower_runtime_constants};
use litd_engine::game_runtime::groups::{OrderTargetKind, UnitCapability};
use litd_engine::game_runtime::hud::QuestProgress;
use litd_engine::game_runtime::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredUnitEconomy, LoweredUpkeepTier,
};
use litd_engine::game_runtime::players::{PlayerController, PlayerSlot};
use litd_engine::game_runtime::victory::VictoryCondition;
use litd_engine::game_runtime::{MatchRuntimeSession, SessionOrder};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const TICK_RATE: u32 = 4;
const GRID: u32 = 16;
const SPAWN_UNIT: &str = "grunt";

fn manifest() -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: SPAWN_UNIT.to_string(),
            gold_cost: 100,
            lumber_cost: 0,
            food_cost: 2,
            build_time_secs: 5,
            food_provided: 0,
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
    let mut blueprint = MatchBlueprint::new("issue1855_cluster", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    for (unit, owner, is_structure) in [(1_u32, 0_u8, true), (2, 1, true)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure,
            capabilities: Vec::new(),
        });
    }
    for (unit, owner) in [(10_u32, 0_u8), (11, 1)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure: false,
            capabilities: vec![UnitCapability {
                ability: 1,
                target_kind: OrderTargetKind::Point,
            }],
        });
    }
    let lowered_match = blueprint.lower().expect("lower match");
    let constants = lower_runtime_constants(TICK_RATE, 200, 6_000, 500, 1_000, GRID, GRID, 0x1855)
        .expect("lower constants");
    RuntimeDatasetsManifest::assemble(economy, lowered_match, constants).expect("assemble")
}

fn order_stream() -> Vec<SessionOrder> {
    vec![
        SessionOrder::QuestCreate {
            quest: 1,
            title: "Hold the line".to_string(),
            description: "Defend Ashford Crossing".to_string(),
            icon: "banner".to_string(),
            required: true,
            required_flags: vec![7],
        },
        SessionOrder::QuestDiscover { quest: 1 },
        SessionOrder::RevealRegion {
            player: 0,
            min_x: (GRID as i32) - 2,
            min_y: (GRID as i32) - 2,
            max_x: (GRID as i32) - 1,
            max_y: (GRID as i32) - 1,
        },
        SessionOrder::SpawnFieldUnit {
            unit_key: SPAWN_UNIT.to_string(),
            owner: 0,
            cell_x: 8,
            cell_z: 8,
        },
        SessionOrder::SetWeather {
            ordinal: 3,
            intensity_permille: 600,
        },
        SessionOrder::SetTimeOfDay { tick_in_day: 40 },
        SessionOrder::PlayCinematic {
            timeline_id: "m1_intro".to_string(),
            pauses_sim: true,
        },
        SessionOrder::CameraFollowUnit { unit: 10 },
        SessionOrder::QuestSatisfyFlag { flag: 7 },
        SessionOrder::QuestComplete { quest: 1 },
        SessionOrder::SkipCinematic,
        SessionOrder::Tick,
    ]
}

fn run_stream(manifest: &RuntimeDatasetsManifest) -> Result<MatchRuntimeSession, String> {
    let mut session =
        MatchRuntimeSession::from_manifest(manifest).map_err(|e| format!("session: {e}"))?;
    for order in order_stream() {
        session
            .apply(order)
            .map_err(|e| format!("apply {}: {}", e.code(), e))?;
    }
    Ok(session)
}

fn edge_cases(manifest: &RuntimeDatasetsManifest) -> Result<Value, String> {
    let mut s =
        MatchRuntimeSession::from_manifest(manifest).map_err(|e| format!("session: {e}"))?;
    let empty_quest = s
        .apply(SessionOrder::QuestCreate {
            quest: 1,
            title: String::new(),
            description: "x".to_string(),
            icon: String::new(),
            required: false,
            required_flags: Vec::new(),
        })
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "UNEXPECTED_OK".to_string());
    let dup_quest = {
        s.apply(SessionOrder::QuestCreate {
            quest: 2,
            title: "A".to_string(),
            description: "B".to_string(),
            icon: String::new(),
            required: false,
            required_flags: Vec::new(),
        })
        .map_err(|e| format!("create: {e}"))?;
        s.apply(SessionOrder::QuestCreate {
            quest: 2,
            title: "A".to_string(),
            description: "B".to_string(),
            icon: String::new(),
            required: false,
            required_flags: Vec::new(),
        })
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "UNEXPECTED_OK".to_string())
    };
    let bad_timer = s
        .apply(SessionOrder::StartTimer {
            one_shot: true,
            period_ticks: 0,
        })
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "UNEXPECTED_OK".to_string());
    let bad_reveal = s
        .apply(SessionOrder::RevealRegion {
            player: 0,
            min_x: 5,
            min_y: 0,
            max_x: 1,
            max_y: 1,
        })
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "UNEXPECTED_OK".to_string());
    let empty_cinematic = s
        .apply(SessionOrder::PlayCinematic {
            timeline_id: String::new(),
            pauses_sim: true,
        })
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "UNEXPECTED_OK".to_string());
    let bad_spawn = s
        .apply(SessionOrder::SpawnFieldUnit {
            unit_key: "missing".to_string(),
            owner: 0,
            cell_x: 1,
            cell_z: 1,
        })
        .err()
        .map(|e| e.code().to_string())
        .unwrap_or_else(|| "UNEXPECTED_OK".to_string());
    Ok(json!({
        "empty_quest_title_code": empty_quest,
        "duplicate_quest_code": dup_quest,
        "zero_period_timer_code": bad_timer,
        "inverted_reveal_code": bad_reveal,
        "empty_cinematic_code": empty_cinematic,
        "unknown_spawn_unit_code": bad_spawn,
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let out_dir = env::var("LITD_1855_FSV_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/fsv/issue1855-session-orders-cluster"));
    fs::create_dir_all(&out_dir)?;

    let manifest = manifest();
    let session_a = run_stream(&manifest)?;
    let session_b = run_stream(&manifest)?;
    let hash_a = session_a.combined_state_hash();
    let hash_b = session_b.combined_state_hash();
    let replay_match = hash_a == hash_b;

    let quest = session_a.hud().quests.quest(1).expect("quest 1");
    let far = (GRID as i32) - 1;
    let fog_visible = session_a.fog().is_visible(0, far, far);
    let spawned = session_a.field_unit(12);
    let weather = (
        session_a.time_of_day().weather_ordinal,
        session_a.time_of_day().weather_intensity_permille,
    );
    // Stream sets tick_in_day=40 then applies one Tick, which advances ToD by 1.
    let tod = session_a.time_of_day().tick_in_day;
    // After SkipCinematic at end of stream, sim advances again.
    let sim_advances = session_a.should_advance_sim();
    let follow = session_a.camera_follow_unit();

    // Prove mid-stream SimGating without mutating the final session.
    let mut gated = MatchRuntimeSession::from_manifest(&manifest)?;
    gated.apply(SessionOrder::PlayCinematic {
        timeline_id: "gate_check".to_string(),
        pauses_sim: true,
    })?;
    let gated_blocks = !gated.should_advance_sim();

    let edges = edge_cases(&manifest)?;
    let edges_ok = edges
        .as_object()
        .map(|o| {
            o.values().all(|v| {
                v.as_str()
                    .is_some_and(|c| c.starts_with("LITD_GAME_RUNTIME_"))
            })
        })
        .unwrap_or(false);

    let verdict = replay_match
        && quest.progress == QuestProgress::Completed
        && quest.discovered
        && fog_visible
        && spawned.is_some()
        && weather == (3, 600)
        && tod == 41
        && sim_advances
        && gated_blocks
        && follow == Some(10)
        && edges_ok;

    let report = json!({
        "issue": [1855, 1857, 1854, 1859, 1861],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "datasets_fingerprint": manifest.datasets_fingerprint,
        "combined_state_hash_a": format!("{hash_a:016x}"),
        "combined_state_hash_b": format!("{hash_b:016x}"),
        "replay_match": replay_match,
        "quest": {
            "id": 1,
            "discovered": quest.discovered,
            "progress": format!("{:?}", quest.progress),
        },
        "fog": {
            "far_corner_visible_player0": fog_visible,
            "modifier_count": session_a.fog_modifiers().len(),
        },
        "spawn": {
            "unit_id": spawned.map(|u| u.id),
            "cell": spawned.map(|u| [u.cell_x, u.cell_z]),
            "owner": spawned.map(|u| u.owner),
        },
        "cinematic": {
            "final_should_advance_sim": sim_advances,
            "midstream_pauses_sim": gated_blocks,
        },
        "weather": {
            "ordinal": weather.0,
            "intensity_permille": weather.1,
        },
        "time_of_day_tick_in_day": tod,
        "camera_follow_unit": follow,
        "edge_cases": edges,
        "order_stream_len": order_stream().len(),
    });

    let report_path = out_dir.join("report.json");
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    let report_sha = sha256_hex(&report_bytes);
    // Source-of-truth readback from disk.
    let readback = fs::read(&report_path)?;
    let readback_sha = sha256_hex(&readback);
    let readback_ok = readback_sha == report_sha;

    println!(
        "ISSUE1855_SESSION_ORDERS_CLUSTER_FSV verdict={}",
        report["verdict"]
    );
    println!("combined_state_hash={hash_a:016x} replay_match={replay_match}");
    println!("report={}", report_path.display());
    println!("report_sha256={report_sha}");
    println!("readback_sha256={readback_sha} match={readback_ok}");
    if !verdict || !readback_ok {
        std::process::exit(1);
    }
    Ok(())
}
