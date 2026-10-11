//! #1851 FSV: author a runtime map AND a full ECA trigger graph via the REAL editor
//! APIs (MCP trigger tools over Calyx Aster), package a real `.litdgame` through the
//! production `build_runtime_litdgame_with_triggers` path, load it through the SAME
//! path `litd-game` uses, and let the AUTHORED triggers drive the match to a victory
//! outcome with no scripted kill: map_initialization arms a mission flag, a countdown
//! timer expiry kills the enemy soldier (gated on the flag + unit_alive), and that
//! unit's death fires the authored victory custom trigger (#838).
//!
//! Determinism: TWO independent executions replay to IDENTICAL combined_state_hash
//! STREAMS (#85), plus the save/load acid check. Independent readback re-parses the
//! package BYTES from disk and re-derives the trigger fingerprint. Edge cases fail
//! LOUD: (1) an out-of-scope authored construct (unit_enters_region, #1856) is
//! rejected at lowering; (2) a tampered trigger section is rejected typed at package
//! load; (3) a trigger referencing a nonexistent unit/variable is rejected at
//! lowering. Report is path-free and run-invariant; sha256 cross-checked via certutil.

mod support;

use std::{fs, path::PathBuf};

use litd_editor::mcp_trigger_tools::read_mcp_trigger_state;
use litd_editor::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, write_quick_share_package,
};
use litd_editor::runtime_datasets_lowering::build_runtime_litdgame_with_triggers;
use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_package::load_runtime_locked_package,
    game_runtime::{MatchRuntimeSession, PlayerResult, TriggerValue},
};
use serde_json::{Value, json};

use support::{
    MAX_PACKAGE_BYTES, SHARE_ID, author_map, author_trigger_state, certutil_sha256,
    enemy_soldier_handle, ghost_variable_edge, lowering_edge, order_stream, run_hash_stream,
    save_load_acid, tampered_triggers_rejected,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1851_runtime_eca_triggers")?;

    // ---- 1. Author the map + trigger graphs through the REAL editor valves. ----
    let aster_root = PathBuf::from("Calyx/target/fsv/issue1851_triggers");
    if aster_root.exists() {
        fs::remove_dir_all(&aster_root)?;
    }
    fs::create_dir_all(&aster_root)?;
    let state_path = aster_root.join("trigger-state-aster");
    let ledger_path = aster_root.join("trigger-ledger-aster");
    author_trigger_state(&state_path, &ledger_path)?;
    let trigger_state = read_mcp_trigger_state(&state_path)?.state;

    let document = author_map()?;
    let request = QuickShareBuildRequest::new(
        SHARE_ID,
        "agent.issue1851",
        QuickShareEndpoint::loopback(SHARE_ID, 8497),
        MAX_PACKAGE_BYTES,
    );
    let package = build_runtime_litdgame_with_triggers(&document, &request, Some(&trigger_state))?;
    let runtime = package
        .runtime
        .as_ref()
        .ok_or("packaged .litdgame did not carry runtime datasets")?;
    let datasets_fingerprint = runtime.datasets_fingerprint.clone();
    let trigger_fingerprint = runtime
        .triggers
        .as_ref()
        .map(|g| g.graph_fingerprint.clone())
        .ok_or("packaged .litdgame did not carry the trigger graph")?;
    let trigger_count = runtime.triggers.as_ref().map(|g| g.triggers.len());

    // ---- 2. Write + load through the production litd-game path. ----
    let package_path = run.run_dir().join("issue1851.litdgame.json");
    let writeback = write_quick_share_package(&package_path, &package, MAX_PACKAGE_BYTES)?;
    let package_sha256 = writeback.sha256.clone();
    let readback = load_runtime_locked_package(&package_path, "litd-game-cli")?;
    let loaded = readback
        .runtime
        .clone()
        .ok_or("loaded package did not carry runtime datasets")?;
    let load_fingerprint_matches = loaded.datasets_fingerprint == datasets_fingerprint;

    // ---- 3. Independent readback FROM PACKAGE BYTES: re-parse the raw JSON and
    // re-derive the trigger graph fingerprint from its canonical form. ----
    let raw: Value = serde_json::from_slice(&fs::read(&package_path)?)?;
    let raw_triggers = raw
        .pointer("/runtime/triggers")
        .cloned()
        .ok_or("raw package JSON has no /runtime/triggers section")?;
    let independent_graph: litd_engine::game_runtime::LoweredTriggerGraph =
        serde_json::from_value(raw_triggers)?;
    let independent_readback_ok = independent_graph.compute_fingerprint()? == trigger_fingerprint
        && independent_graph.graph_fingerprint == trigger_fingerprint;

    // ---- 4. The authored triggers play the match to victory (no scripted kill). ----
    let enemy_soldier = enemy_soldier_handle(&loaded)?;
    let stream = order_stream();
    let mut session = MatchRuntimeSession::from_manifest(&loaded)?;
    let mut victory_tick: Option<u64> = None;
    for order in &stream {
        session.apply(order.clone())?;
        if victory_tick.is_none() && session.match_state().result(0)? == PlayerResult::Victory {
            victory_tick = Some(session.applied_tick());
        }
    }
    let mission_started =
        session.triggers().variable("mission_started") == Some(&TriggerValue::Flag(true));
    let soldier_dead = !session.match_state().is_alive(enemy_soldier)?;
    let result_p0 = format!("{:?}", session.match_state().result(0)?);
    let result_p1 = format!("{:?}", session.match_state().result(1)?);
    let timeline: Vec<String> = session
        .triggers()
        .timeline()
        .iter()
        .map(|f| format!("t{:>3} {} via {}", f.tick, f.trigger_id, f.event))
        .collect();
    let fired_events: Vec<&str> = session
        .triggers()
        .timeline()
        .iter()
        .map(|f| f.event)
        .collect();
    let pipeline_complete = fired_events.contains(&"map_initialization")
        && fired_events.contains(&"countdown_timer_expires")
        && fired_events.contains(&"unit_dies");

    // ---- 5. TWO-EXECUTION replay determinism: identical hash STREAMS. ----
    let stream_a = run_hash_stream(&loaded, &stream)?;
    let stream_b = run_hash_stream(&loaded, &stream)?;
    let replay_streams_match = stream_a == stream_b;
    let (continued, reloaded) = save_load_acid(&loaded, &stream)?;
    let acid_match = continued == reloaded;

    // ---- Edge (1): out-of-scope construct rejected at lowering, naming #1856. ----
    let (edge_region_code, edge_region_names_1856) = lowering_edge(
        &trigger_state,
        "trg_region",
        "unit_enters_region",
        &[("region_id", "gate"), ("unit_id", "placed_soldier_p1")],
    )?;
    // ---- Edge (3a): nonexistent unit reference rejected at lowering. ----
    let (edge_ghost_unit_code, _) = lowering_edge(
        &trigger_state,
        "trg_ghost",
        "unit_dies",
        &[("unit_id", "ghost_unit")],
    )?;
    // ---- Edge (3b): nonexistent variable reference rejected at lowering. ----
    let edge_ghost_var_code = ghost_variable_edge(&trigger_state)?;

    // ---- Edge (2): tampered trigger section rejected typed at package load. ----
    let corrupt_path = run.run_dir().join("corrupt_triggers.litdgame.json");
    let tamper_code = tampered_triggers_rejected(&corrupt_path, &package)?;

    let victory_reached =
        victory_tick.is_some() && result_p0 == "Victory" && result_p1 == "Playing";
    let verdict = load_fingerprint_matches
        && trigger_count == Some(3)
        && independent_readback_ok
        && mission_started
        && soldier_dead
        && pipeline_complete
        && victory_reached
        && replay_streams_match
        && acid_match
        && edge_region_code == "LITD-GAME-PACKAGE-INVALID-CONFIG"
        && edge_region_names_1856
        && edge_ghost_unit_code == "LITD-GAME-PACKAGE-INVALID-CONFIG"
        && edge_ghost_var_code == "LITD-GAME-PACKAGE-INVALID-CONFIG"
        && tamper_code == "LITD-REFLECT-FINGERPRINT-MISMATCH";

    let report = json!({
        "schema": "litd.editor.issue1851.runtime_eca_triggers_fsv.v1",
        "issue": 1851, "parents": [1784, 1788], "doctrine": [1783, 1848], "feeds": [1856],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "One 2-player runtime map authored via real EditorObject APIs plus a three-trigger ECA graph authored through the REAL MCP trigger tools (Calyx Aster store), lowered through the new #1851 runtime trigger valve, packaged through build_runtime_litdgame_with_triggers, and loaded through load_runtime_locked_package (the production litd-game path). The AUTHORED triggers alone drive the mission: map_initialization arms a flag, a countdown timer expiry (routed from the previously-discarded TimerFired records) kills the enemy soldier under flag+unit_alive conditions, and unit_dies fires the authored CustomTrigger victory. Two independent executions produce identical combined_state_hash streams and the save/load acid holds. Independent readback re-derives the trigger fingerprint from raw package bytes. Three lowering edge cases and one load-time tamper control reject typed.",
        "package_sha256": package_sha256,
        "datasets_fingerprint": datasets_fingerprint,
        "trigger_graph_fingerprint": trigger_fingerprint,
        "trigger_count": trigger_count,
        "load_fingerprint_matches": load_fingerprint_matches,
        "independent_readback_ok": independent_readback_ok,
        "match_outcome": {
            "mission_started_flag": mission_started,
            "enemy_soldier_dead": soldier_dead,
            "player0_result": result_p0,
            "player1_result": result_p1,
            "victory_tick": victory_tick,
            "trigger_pipeline_complete": pipeline_complete,
        },
        "trigger_timeline": timeline,
        "final_state_hash": format!("{:016x}", session.combined_state_hash()),
        "replay_hash_stream_len": stream_a.len(),
        "replay_streams_match": replay_streams_match,
        "save_load_acid_match": acid_match,
        "edge_region_construct_code": edge_region_code,
        "edge_region_names_1856": edge_region_names_1856,
        "edge_ghost_unit_code": edge_ghost_unit_code,
        "edge_ghost_variable_code": edge_ghost_var_code,
        "neg_tampered_triggers_code": tamper_code,
    });
    let readback_report = run.write_report(&report)?;
    let report_sha256_certutil = certutil_sha256(&readback_report.path);
    println!(
        "ISSUE1851_RUNTIME_ECA_TRIGGERS_FSV verdict={} report={} report_sha256={} report_sha256_certutil={} datasets_fingerprint={} trigger_fingerprint={} victory_tick={:?} p0={} p1={} replay_match={} acid_match={} region_code={} ghost_unit_code={} ghost_var_code={} tamper_code={}",
        if verdict { "GREEN" } else { "RED" },
        readback_report.path.display(),
        readback_report.sha256,
        report_sha256_certutil.as_deref().unwrap_or("NA"),
        datasets_fingerprint,
        trigger_fingerprint,
        victory_tick,
        result_p0,
        result_p1,
        replay_streams_match,
        acid_match,
        edge_region_code,
        edge_ghost_unit_code,
        edge_ghost_var_code,
        tamper_code,
    );
    for line in &timeline {
        println!("  {line}");
    }
    if !verdict {
        return Err("ISSUE1851 FSV verdict RED: an acceptance predicate failed".into());
    }
    Ok(())
}
