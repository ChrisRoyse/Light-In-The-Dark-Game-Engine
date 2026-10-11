//! #1786 FSV: author a mini melee map AND a computer-opponent AI profile via the REAL
//! editor APIs, package a real `.litdgame` through the production `build_runtime_litdgame`
//! path, load it through the SAME path `litd-game` uses, and let the AUTHORED profile play a
//! full melee match to a terminal victory WITHOUT human input on its side (the opposing side
//! is scripted minimally: one strike on an AI structure to exercise the retreat rule).
//!
//! Proves the runtime half of the computer opponent: workers harvested, structures built per
//! build order, army trained (supply-gated), an attack wave formed + sent + a fog-gated
//! strike, a retreat/regroup reaction to losing a structure, and a terminal victory/defeat.
//! Two IN-PROCESS runs replay to one hash (cross-process invariance = the report sha256 across
//! the harness's two runs). Edge cases: (1) an unsatisfiable build order (missing tech) fails
//! LOUD at packaging; (2) the supply cap blocks training visibly, not spinning; (3) retreat.
//! Negative control: a corrupted profile dataset is rejected typed at package load.

use std::{fs, path::Path, process::Command};

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::AssetVaultCatalog;
use litd_editor::history::{EditorDocument, EditorObject};
use litd_editor::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, QuickSharePackage, write_quick_share_package,
};
use litd_editor::runtime_datasets_lowering::{build_runtime_litdgame, lower_runtime_datasets};
use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    game_package::load_runtime_locked_package,
    game_runtime::{MatchRuntimeSession, PlayerResult, RuntimeDatasetsManifest, SessionOrder},
    math::{Transform, Vec3},
};
use serde_json::json;

const SHARE_ID: &str = "issue1786ai";
const MAX_PACKAGE_BYTES: usize = 4 * 1024 * 1024;
const UNIT_IDS: [&str; 8] = [
    "type_worker",
    "type_soldier",
    "type_town_hall",
    "placed_hall_p0",
    "placed_hall_p1a",
    "placed_hall_p1b",
    "placed_soldier_p0",
    "placed_soldier_p1",
];
const MATCH_TICKS: usize = 60;
const SCRIPTED_STRIKE_INDEX: usize = 3;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1786_computer_opponent")?;

    let document = author_melee_with_ai("type_town_hall")?;
    let request = QuickShareBuildRequest::new(
        SHARE_ID,
        "agent.issue1786",
        QuickShareEndpoint::loopback(SHARE_ID, 8496),
        MAX_PACKAGE_BYTES,
    );
    let package = build_runtime_litdgame(&document, &request)?;
    let runtime = package
        .runtime
        .as_ref()
        .ok_or("packaged .litdgame did not carry runtime datasets")?;
    let datasets_fingerprint = runtime.datasets_fingerprint.clone();
    let ai_profile_count = runtime.ai_profiles.len();
    let ai_display = runtime
        .ai_profiles
        .first()
        .map(|p| p.display_name.clone())
        .unwrap_or_default();

    let package_path = run.run_dir().join("issue1786.litdgame.json");
    let writeback = write_quick_share_package(&package_path, &package, MAX_PACKAGE_BYTES)?;
    let package_sha256 = writeback.sha256.clone();

    let readback = load_runtime_locked_package(&package_path, "litd-game-cli")?;
    let loaded = readback
        .runtime
        .clone()
        .ok_or("loaded package did not carry runtime datasets")?;
    let load_fingerprint_matches = loaded.datasets_fingerprint == datasets_fingerprint;

    // The AI player is dense slot 1 (player_two); pick one of its structures for the scripted
    // enemy strike that triggers the retreat rule.
    let blueprint = loaded.lowered_match.verify()?;
    let ai_structure = blueprint
        .units
        .iter()
        .find(|u| u.is_structure && u.owner == 1)
        .map(|u| u.unit)
        .ok_or("no AI structure in blueprint")?;

    let stream = scripted_stream(ai_structure);

    // --- Observed full match: the AI plays; only Ticks + one scripted strike are applied. ---
    let mut session = MatchRuntimeSession::from_manifest(&loaded)?;
    let mut victory_tick: Option<u64> = None;
    for order in &stream {
        session.apply(order.clone())?;
        if victory_tick.is_none() && session.match_state().result(1)? == PlayerResult::Victory {
            victory_tick = Some(session.applied_tick());
        }
    }
    let harvested_gold = session.economy().accounts().balance("gold");
    let trained_bodies = session.economy().unit_body_count();
    let result_ai = format!("{:?}", session.match_state().result(1)?);
    let result_enemy = format!("{:?}", session.match_state().result(0)?);
    let timeline = collect_timeline(&session);
    let kinds: Vec<&str> = session.ai().timeline().iter().map(|e| e.kind).collect();
    let has = |k: &str| kinds.contains(&k);

    // --- Two in-process replays to one hash + save/load acid. ---
    let run_a = run_match(&loaded, &stream)?;
    let run_b = run_match(&loaded, &stream)?;
    let in_process_replay_match = run_a == run_b;
    let (continued, reloaded) = save_load_acid(&loaded, &stream)?;
    let acid_match = continued == reloaded;

    // --- Edge (2): the supply cap blocks training visibly (wave_size 50 vs 20 food). ---
    let supply_doc = author_melee_with_ai("type_town_hall")?;
    let supply_manifest = lower_supply_variant(&supply_doc)?;
    let supply_blocked = run_supply_probe(&supply_manifest)?;

    // --- Edge (1): an unsatisfiable build order (missing tech) fails LOUD at packaging with a
    // STABLE TYPED diagnostics code (not a brittle message substring). ---
    let bad_doc = author_melee_with_ai("type_missing_barracks")?;
    let unsatisfiable_code = match build_runtime_litdgame(&bad_doc, &request) {
        Ok(_) => "ACCEPTED_SILENTLY".to_string(),
        Err(error) => error.code().to_string(),
    };
    let unsatisfiable_rejected = unsatisfiable_code == "LITD-GAME-PACKAGE-INVALID-CONFIG";

    // --- Negative control: a corrupted profile dataset rejected typed at package load. ---
    let corrupt_path = run.run_dir().join("corrupt_profile.litdgame.json");
    let corrupt_code = corrupt_profile_rejected(&corrupt_path, &package)?;

    let victory_reached =
        victory_tick.is_some() && result_ai == "Victory" && result_enemy == "Defeat";
    let macro_complete = has("WORKER")
        && has("BUILD")
        && has("TRAIN")
        && has("WAVE_SENT")
        && has("STRIKE")
        && has("RETREAT")
        && has("RESUME");
    let verdict = load_fingerprint_matches
        && ai_profile_count == 1
        && victory_reached
        && macro_complete
        && harvested_gold > 0
        && trained_bodies >= 1
        && in_process_replay_match
        && acid_match
        && supply_blocked
        && unsatisfiable_rejected
        && corrupt_code == "LITD-REFLECT-FINGERPRINT-MISMATCH";

    let report = json!({
        "schema": "litd.editor.issue1786.computer_opponent_fsv.v1",
        "issue": 1786, "parents": [1784, 1843], "feeds": [805, 838],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "One 2-player melee authored via real EditorObject APIs with one editor-authored AiProfile, packaged through build_runtime_litdgame (the production runtime-datasets path) and loaded through load_runtime_locked_package (the production litd-game path). The AUTHORED profile -- executed by the generic runtime as a deterministic order-source folded into the session hash -- plays a full match unaided: workers harvest, structures build per build order, army trains supply-gated, an attack wave forms + a fog-gated strike lands, it retreats+regroups on losing a structure, and it reaches a terminal victory. Two in-process replays give one hash; three edge cases (unsatisfiable build order fails loud, supply cap blocks, retreat) and one typed negative control (corrupted profile rejected at load) cover determinism and rejection.",
        "package_sha256": package_sha256,
        "datasets_fingerprint": datasets_fingerprint,
        "ai_profile_count": ai_profile_count,
        "ai_display_name": ai_display,
        "load_fingerprint_matches": load_fingerprint_matches,
        "match_outcome": {
            "ai_player_result": result_ai,
            "enemy_player_result": result_enemy,
            "victory_tick": victory_tick,
            "harvested_gold": harvested_gold,
            "trained_bodies": trained_bodies,
        },
        "ai_decision_timeline": timeline,
        "final_state_hash": format!("{:016x}", session.combined_state_hash()),
        "in_process_replay_state_hash": format!("{run_a:016x}"),
        "in_process_replay_match": in_process_replay_match,
        "save_load_acid_match": acid_match,
        "save_load_acid_state_hash": format!("{continued:016x}"),
        "edge_supply_cap_blocked": supply_blocked,
        "edge_unsatisfiable_build_code": unsatisfiable_code,
        "edge_unsatisfiable_rejected": unsatisfiable_rejected,
        "neg_corrupt_profile_code": corrupt_code,
    });
    let readback_report = run.write_report(&report)?;
    let report_sha256_certutil = certutil_sha256(&readback_report.path);
    println!(
        "ISSUE1786_COMPUTER_OPPONENT_FSV verdict={} report={} report_sha256={} report_sha256_certutil={} datasets_fingerprint={} victory_tick={:?} ai_result={} enemy_result={} replay_hash={:016x} replay_match={} acid_match={}",
        if verdict { "GREEN" } else { "RED" },
        readback_report.path.display(),
        readback_report.sha256,
        report_sha256_certutil.as_deref().unwrap_or("NA"),
        datasets_fingerprint,
        victory_tick,
        result_ai,
        result_enemy,
        run_a,
        in_process_replay_match,
        acid_match,
    );
    for line in &timeline {
        println!("  {line}");
    }
    if !verdict {
        return Err("ISSUE1786 FSV verdict RED: an acceptance predicate failed".into());
    }
    Ok(())
}

/// Author a 2-player melee + one AI profile through `EditorObject`/`EditorDocument`. The AI
/// (player_two) gets TWO structures so losing one triggers retreat, not defeat. `army_struct`
/// names the structure the AI build order places (a bogus id exercises the missing-tech edge).
fn author_melee_with_ai(build_struct: &str) -> Result<EditorDocument, EngineError> {
    fn obj(id: &str, kind: &str, transform: Transform, props: &[(&str, &str)]) -> EditorObject {
        let mut object = EditorObject::new(id, kind, Some("world".to_string()), transform);
        for (key, value) in props {
            object = object.with_property(*key, *value);
        }
        object
    }

    let mut document = EditorDocument::new("calyx://world/issue1786-ai-melee", "rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;

    document.upsert_object(obj(
        "runtime_config",
        "GameplayConstant",
        Transform::IDENTITY,
        &[
            ("runtime.tick_rate", "4"),
            ("runtime.worker_cells_per_sec", "8"),
            ("runtime.deposit_ticks", "1"),
            ("runtime.start_gold", "10000"),
            ("runtime.start_lumber", "10000"),
            ("runtime.upkeep_food_cap", "80"),
            ("runtime.day_length_ticks", "200"),
            ("runtime.base_sight_radius_mm", "6000"),
            ("runtime.night_sight_penalty_permille", "500"),
            ("runtime.fog_cell_size_mm", "1000"),
            ("runtime.fog_grid_width", "16"),
            ("runtime.fog_grid_height", "16"),
            ("runtime.match_seed", "1786"),
        ],
    ))?;

    document.upsert_object(obj(
        "type_worker",
        "Unit",
        Transform::IDENTITY,
        &[
            ("runtime.role", "worker"),
            ("economy.gold_cost", "50"),
            ("economy.build_time_secs", "1"),
        ],
    ))?;
    document.upsert_object(obj(
        "type_soldier",
        "Unit",
        Transform::IDENTITY,
        &[
            ("runtime.role", "soldier"),
            ("economy.gold_cost", "100"),
            ("economy.lumber_cost", "10"),
            ("economy.food_cost", "2"),
            ("economy.build_time_secs", "1"),
        ],
    ))?;
    document.upsert_object(obj(
        "type_town_hall",
        "Unit",
        Transform::IDENTITY,
        &[
            ("runtime.role", "structure"),
            ("economy.gold_cost", "200"),
            ("economy.build_time_secs", "1"),
            ("economy.food_provided", "20"),
            ("runtime.footprint_w", "2"),
            ("runtime.footprint_h", "2"),
            ("runtime.trains", "type_soldier,type_worker"),
        ],
    ))?;

    document.upsert_object(obj(
        "gold_mine",
        "ResourceNode",
        Transform::from_translation(Vec3::new(5.0, 0.0, 3.0)),
        &[
            ("resource.key", "gold"),
            ("resource.amount_total", "5000"),
            ("resource.yield_per_trip", "10"),
        ],
    ))?;

    for (id, owner, role) in [
        ("placed_hall_p0", "player_one", "structure"),
        ("placed_hall_p1a", "player_two", "structure"),
        ("placed_hall_p1b", "player_two", "structure"),
        ("placed_soldier_p0", "player_one", "soldier"),
        ("placed_soldier_p1", "player_two", "soldier"),
    ] {
        document.upsert_object(obj(
            id,
            "Unit",
            Transform::IDENTITY,
            &[("runtime.role", role), ("template.owner_player_id", owner)],
        ))?;
    }

    // The authored computer-opponent profile (the #1786 runtime input).
    document.upsert_object(obj(
        "ai_aggressor",
        "AiProfile",
        Transform::IDENTITY,
        &[
            ("ai.controlled_player", "player_two"),
            ("ai.display_name", "Aggressor-1786"),
            ("ai.decision_interval_ticks", "1"),
            ("ai.worker_node_id", "gold_mine"),
            ("ai.worker_target", "3"),
            ("ai.army_unit_id", "type_soldier"),
            ("ai.wave_size", "2"),
            ("ai.retreat_when_structures_below", "2"),
            ("ai.retreat_cooldown_ticks", "6"),
            ("ai.handicap_permille", "1000"),
            ("ai.build_order", &format!("{build_struct}@0:10,10")),
        ],
    ))?;

    // Prefer any real vault model present during progressive #1878 rematerialization.
    // medievaldungeon/Rock1.fbx is not always on disk after partial intake; fail-loud if
    // the catalog has zero usable model paths rather than hardcoding a missing pack.
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/George.gltf"
                || record.relative_path
                    == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
                || (record.relative_path.ends_with(".glb")
                    && record.relative_path.contains("kaykit"))
                || (record.relative_path.ends_with(".gltf")
                    && record.relative_path.contains("polyhaven/models"))
        })
        .cloned()
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: "issue1786.fsv.vault_model".to_string(),
            reason: "missing real vault model record (need rematerialized Stan.gltf or other vault mesh)".to_string(),
        })?;
    for unit_id in UNIT_IDS {
        assign_vault_model_to_unit(&mut document, unit_id, &record)?;
    }
    Ok(document)
}

/// The match order stream: Ticks (the AI plays on each) plus ONE scripted enemy strike on an
/// AI structure to exercise the retreat rule. Fixed-length so two runs replay identically.
fn scripted_stream(ai_structure: u32) -> Vec<SessionOrder> {
    let mut orders = Vec::with_capacity(MATCH_TICKS + 1);
    for i in 0..MATCH_TICKS {
        if i == SCRIPTED_STRIKE_INDEX {
            orders.push(SessionOrder::DestroyUnit { unit: ai_structure });
        }
        orders.push(SessionOrder::Tick);
    }
    orders
}

fn collect_timeline(session: &MatchRuntimeSession) -> Vec<String> {
    session
        .ai()
        .timeline()
        .iter()
        .map(|e| format!("t{:>3} p{} {:<14} {}", e.tick, e.player, e.kind, e.detail))
        .collect()
}

fn run_match(
    manifest: &RuntimeDatasetsManifest,
    orders: &[SessionOrder],
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    for order in orders {
        session.apply(order.clone())?;
    }
    Ok(session.combined_state_hash())
}

fn save_load_acid(
    manifest: &RuntimeDatasetsManifest,
    orders: &[SessionOrder],
) -> Result<(u64, u64), Box<dyn std::error::Error>> {
    let midpoint = orders.len() / 2;
    let mut original = MatchRuntimeSession::from_manifest(manifest)?;
    for order in &orders[..midpoint] {
        original.apply(order.clone())?;
    }
    let save = original.capture_save()?;
    for order in &orders[midpoint..] {
        original.apply(order.clone())?;
    }
    let continued = original.combined_state_hash();
    let mut reloaded = MatchRuntimeSession::restore_from_save(manifest, &save)?;
    for order in &orders[midpoint..] {
        reloaded.apply(order.clone())?;
    }
    Ok((continued, reloaded.combined_state_hash()))
}

/// Re-lower the document but bump the profile's `wave_size` past what the food cap allows, so
/// the AI must hit the supply block. Returns whether a SUPPLY_BLOCKED decision was recorded.
fn lower_supply_variant(
    document: &EditorDocument,
) -> Result<RuntimeDatasetsManifest, Box<dyn std::error::Error>> {
    let mut doc = document.clone();
    if let Some(profile) = doc.objects.get_mut("ai_aggressor") {
        profile
            .properties
            .insert("ai.wave_size".to_string(), "50".to_string());
    }
    Ok(lower_runtime_datasets(&doc)?)
}

fn run_supply_probe(
    manifest: &RuntimeDatasetsManifest,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    for _ in 0..30 {
        session.apply(SessionOrder::Tick)?;
    }
    Ok(session
        .ai()
        .timeline()
        .iter()
        .any(|e| e.kind == "SUPPLY_BLOCKED"))
}

/// Tamper a lowered profile field WITHOUT re-deriving the fingerprint; the production load
/// path must reject it typed (fingerprint mismatch), never boot a tampered opponent.
fn corrupt_profile_rejected(
    path: &Path,
    package: &QuickSharePackage,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut tampered = package.clone();
    if let Some(runtime) = tampered.runtime.as_mut()
        && let Some(profile) = runtime.ai_profiles.first_mut()
    {
        profile.wave_size = 999; // fingerprint no longer matches contents
    }
    fs::write(path, serde_json::to_vec_pretty(&tampered)?)?;
    match load_runtime_locked_package(path, "litd-game-cli") {
        Ok(_) => Ok("ACCEPTED_SILENTLY".to_string()),
        Err(error) => Ok(error.code().to_string()),
    }
}

fn certutil_sha256(path: &Path) -> Option<String> {
    let output = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let candidate: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}
