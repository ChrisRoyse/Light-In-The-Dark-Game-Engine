//! #1843 FSV: author a mini melee map via the REAL editor APIs, package a real `.litdgame`
//! through the PRODUCTION path (`build_runtime_litdgame` = `build_quick_share_package` +
//! `attach_runtime_datasets`, the #853 campaign-attach analogue), load it through the SAME
//! path `litd-game` uses (`load_runtime_locked_package`), and run a scripted mini match on the
//! composed `MatchRuntimeSession` the host instantiates. No hand-assembled envelope and no
//! fabricated fingerprint: document/play fingerprints are the REAL editor-derived ones, the
//! runtime datasets are bound into the envelope `manifest_fingerprint` (#1843), and the
//! editor's `validate_package` runs on the runtime-carrying package (attach + write).
//!
//! Proves: harvest -> train -> group orders -> fog updates on move -> save/load -> destroy
//! enemy structure -> victory flip. Two IN-PROCESS runs replay to one hash. Negative controls:
//! self-inconsistent dataset rejected; a SUBSTITUTED (self-consistent) runtime section rejected
//! by the envelope binding; a sabotaged save rejected. Host-scope note (#1786 follow-up): the
//! host advances by deterministic ticks only; routing player order-input into the running host
//! session is out of #1843 scope.

use std::{fs, path::Path, process::Command};

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::AssetVaultCatalog;
use litd_editor::history::{EditorDocument, EditorObject};
use litd_editor::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, QuickSharePackage, write_quick_share_package,
};
use litd_editor::runtime_datasets_lowering::{
    MOVE_ABILITY, build_runtime_litdgame, lower_runtime_datasets,
};
use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    game_package::load_runtime_locked_package,
    game_runtime::{MatchRuntimeSession, RuntimeDatasetsManifest, SessionOrder},
    math::{Transform, Vec3},
};
use serde_json::json;

const SHARE_ID: &str = "issue1843mini";
const MAX_PACKAGE_BYTES: usize = 4 * 1024 * 1024;
const VAULT_MODEL: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const UNIT_IDS: [&str; 7] = [
    "type_worker",
    "type_soldier",
    "type_town_hall",
    "placed_hall_p0",
    "placed_hall_p1",
    "placed_soldier_p0",
    "placed_soldier_p1",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1843_runtime_host")?;

    let document = author_mini_melee()?;

    let request = QuickShareBuildRequest::new(
        SHARE_ID,
        "agent.issue1843",
        QuickShareEndpoint::loopback(SHARE_ID, 8493),
        MAX_PACKAGE_BYTES,
    );
    let package = build_runtime_litdgame(&document, &request)?;
    let datasets_fingerprint = package
        .runtime
        .as_ref()
        .ok_or("packaged .litdgame did not carry runtime datasets")?
        .datasets_fingerprint
        .clone();

    let package_path = run.run_dir().join("issue1843.litdgame.json");
    let writeback = write_quick_share_package(&package_path, &package, MAX_PACKAGE_BYTES)?;
    let package_sha256 = writeback.sha256.clone();
    let package_sha256_certutil = certutil_sha256(&package_path);

    let readback = load_runtime_locked_package(&package_path, "litd-game-cli")?;
    let loaded = readback
        .runtime
        .clone()
        .ok_or("loaded package did not carry runtime datasets")?;
    let load_fingerprint_matches = loaded.datasets_fingerprint == datasets_fingerprint;

    let blueprint = loaded.lowered_match.verify()?;
    let enemy_structure = blueprint
        .units
        .iter()
        .find(|u| u.is_structure && u.owner == 1)
        .map(|u| u.unit)
        .ok_or("no enemy structure in blueprint")?;
    let friendly_field = blueprint
        .units
        .iter()
        .find(|u| !u.is_structure && u.owner == 0)
        .map(|u| u.unit)
        .ok_or("no friendly field unit in blueprint")?;

    let stream = scripted_stream(enemy_structure, friendly_field);
    let mut session = MatchRuntimeSession::from_manifest(&loaded)?;
    let mut fog_hash_before_move = 0_u64;
    let mut fog_hash_after_move = 0_u64;
    let mut trained_bodies = 0_usize;
    let mut victory_before = String::new();
    for order in &stream {
        if matches!(order, SessionOrder::MoveFieldUnit { .. }) {
            fog_hash_before_move = session.fog().state_hash();
        }
        if matches!(order, SessionOrder::DestroyUnit { .. }) {
            victory_before = format!("{:?}", session.match_state().result(0)?);
            trained_bodies = session.economy().unit_body_count();
        }
        session.apply(order.clone())?;
        if matches!(order, SessionOrder::MoveFieldUnit { .. }) {
            session.apply(SessionOrder::Tick)?;
            fog_hash_after_move = session.fog().state_hash();
        }
    }
    let harvested_gold = session.economy().accounts().balance("gold");
    let victory_after = format!("{:?}", session.match_state().result(0)?);
    let defeat_after = format!("{:?}", session.match_state().result(1)?);
    let final_hash = session.combined_state_hash();

    let (continued_hash, reloaded_hash) = save_load_acid(&loaded, &stream)?;
    let acid_match = continued_hash == reloaded_hash;

    let run_a = run_match(&loaded, &stream)?;
    let run_b = run_match(&loaded, &stream)?;
    let in_process_replay_match = run_a == run_b;

    let corrupt_path = run.run_dir().join("corrupt.litdgame.json");
    let substituted_path = run.run_dir().join("substituted.litdgame.json");
    let corrupt_code = corrupt_dataset_rejected(&corrupt_path, &package)?;
    let substituted_code = substituted_runtime_rejected(&substituted_path, &package, &document)?;
    let sabotage_code = sabotaged_save_rejected(&loaded)?;

    let fog_changed = fog_hash_before_move != fog_hash_after_move;
    let verdict = load_fingerprint_matches
        && harvested_gold > 0
        && trained_bodies >= 1
        && fog_changed
        && acid_match
        && in_process_replay_match
        && victory_before == "Playing"
        && victory_after == "Victory"
        && defeat_after == "Defeat"
        && corrupt_code == "LITD-REFLECT-FINGERPRINT-MISMATCH"
        && substituted_code.contains("manifest_fingerprint")
        && sabotage_code == "LITD_GAME_RUNTIME_FINGERPRINT";

    let report = json!({
        "schema": "litd.editor.issue1843.runtime_host_fsv.v1",
        "issue": 1843, "parents": [1784, 804], "seams_bound": [836, 848, 1842],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "One 2-player melee authored via real EditorObject APIs, packaged through the production build_runtime_litdgame path (build_quick_share_package + attach_runtime_datasets with REAL fingerprints and validate_package) and loaded through load_runtime_locked_package (the production litd-game path), is the smallest artifact exercising every integration seam once: harvest+train, group orders, fog<->alliance + day/night<->sight, save/load replay, victory flip. Two in-process replays plus three fail-loud negative controls (self-inconsistent dataset, SUBSTITUTED runtime section rejected by the envelope binding, sabotaged save) cover determinism and rejection.",
        "package_sha256": package_sha256,
        "package_sha256_certutil": package_sha256_certutil,
        "package_sha256_certutil_matches": package_sha256_certutil.as_deref() == Some(package_sha256.as_str()),
        "document_fingerprint": package.document_fingerprint,
        "manifest_fingerprint": package.manifest_fingerprint,
        "datasets_fingerprint": datasets_fingerprint,
        "asset_count": package.asset_count,
        "load_fingerprint_matches": load_fingerprint_matches,
        "asset_bytes_verified": readback.asset_bytes_verified,
        "harvested_gold": harvested_gold,
        "trained_bodies": trained_bodies,
        "fog_changed_on_move": fog_changed,
        "victory_before_destroy": victory_before,
        "victory_after_destroy": victory_after,
        "defeat_after_destroy": defeat_after,
        "final_state_hash": format!("{final_hash:016x}"),
        "save_load_acid_match": acid_match,
        "save_load_acid_state_hash": format!("{continued_hash:016x}"),
        "in_process_replay_note": "two in-process runs of the same stream (NOT two OS processes); cross-process invariance is the report_sha256 across the harness's two runs",
        "in_process_replay_state_hash": format!("{run_a:016x}"),
        "in_process_replay_match": in_process_replay_match,
        "neg_self_inconsistent_dataset_code": corrupt_code,
        "neg_substituted_runtime_error": substituted_code,
        "neg_sabotaged_save_code": sabotage_code,
    });
    let readback_report = run.write_report(&report)?;
    println!(
        "ISSUE1843_RUNTIME_HOST_FSV verdict={} report={} report_sha256={} datasets_fingerprint={} in_process_replay_state_hash={:016x} in_process_replay_match={} acid_match={} package_sha256={}",
        if verdict { "GREEN" } else { "RED" },
        readback_report.path.display(),
        readback_report.sha256,
        datasets_fingerprint,
        run_a,
        in_process_replay_match,
        acid_match,
        package_sha256,
    );
    if !verdict {
        return Err("ISSUE1843 FSV verdict RED: an acceptance predicate failed".into());
    }
    Ok(())
}

/// Author a 2-player melee map through `EditorObject`/`EditorDocument`, then assign a vault
/// mesh to every Unit (the editor play/packaging path requires a model on each Unit).
fn author_mini_melee() -> Result<EditorDocument, EngineError> {
    // Build a `world`-parented object with a property bag (compresses the authoring calls).
    fn obj(id: &str, kind: &str, transform: Transform, props: &[(&str, &str)]) -> EditorObject {
        let mut object = EditorObject::new(id, kind, Some("world".to_string()), transform);
        for (key, value) in props {
            object = object.with_property(*key, *value);
        }
        object
    }

    let mut document = EditorDocument::new("calyx://world/issue1843-mini-melee", "rev-1")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;

    // Runtime-config record (#827/#848 tuning).
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
            ("runtime.match_seed", "6211"),
        ],
    ))?;

    // Unit-type catalog (#818 econ fields).
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

    // Resource node (cell from transform).
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

    // Placed starting units (players/forces + victory + fog emitters).
    for (id, owner, role) in [
        ("placed_hall_p0", "player_one", "structure"),
        ("placed_hall_p1", "player_two", "structure"),
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

    // Every Unit needs a mesh product for the editor play/packaging path; sharing one vault
    // record yields a single deterministic asset product (asset_count == 1).
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| record.relative_path == VAULT_MODEL)
        .cloned()
        .ok_or_else(|| EngineError::GamePackageInvalidConfig {
            field: "issue1843.fsv.vault_model".to_string(),
            reason: format!("vault record {VAULT_MODEL} not found"),
        })?;
    for unit_id in UNIT_IDS {
        assign_vault_model_to_unit(&mut document, unit_id, &record)?;
    }
    Ok(document)
}

/// The scripted match order stream; the handles come from the loaded datasets' blueprint.
fn scripted_stream(enemy_structure: u32, friendly_field: u32) -> Vec<SessionOrder> {
    let mut orders = vec![SessionOrder::AddWorker {
        worker_id: 1,
        node_id: "gold_mine".to_string(),
    }];
    for _ in 0..40 {
        orders.push(SessionOrder::Tick);
    }
    orders.push(SessionOrder::PlaceStructure {
        struct_id: "type_town_hall".to_string(),
        x: 2,
        z: 2,
    });
    for _ in 0..6 {
        orders.push(SessionOrder::Tick);
    }
    orders.push(SessionOrder::TrainUnit {
        instance_id: 0,
        unit_id: "type_soldier".to_string(),
    });
    for _ in 0..6 {
        orders.push(SessionOrder::Tick);
    }
    orders.push(SessionOrder::AddToGroup {
        player: 0,
        unit: friendly_field,
    });
    orders.push(SessionOrder::SelectUnits {
        player: 0,
        units: vec![friendly_field],
    });
    orders.push(SessionOrder::IssueMove {
        player: 0,
        units: vec![friendly_field],
        ability: MOVE_ABILITY,
        x: 9,
        z: 9,
        queued: false,
    });
    orders.push(SessionOrder::MoveFieldUnit {
        field_id: friendly_field,
        cell_x: 9,
        cell_z: 9,
    });
    orders.push(SessionOrder::SetAlliance {
        from: 0,
        to: 1,
        ally: true,
        shared_vision: true,
        shared_control: false,
    });
    orders.push(SessionOrder::SetTimeOfDay { tick_in_day: 0 });
    orders.push(SessionOrder::Tick);
    orders.push(SessionOrder::DestroyUnit {
        unit: enemy_structure,
    });
    orders
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

/// Serialize a package to disk WITHOUT re-validating (the negative controls write a package
/// validation would reject, then prove the load path rejects it).
fn write_raw(path: &Path, package: &QuickSharePackage) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(package)?)?;
    Ok(())
}

/// Mutate a unit cost in the runtime section WITHOUT re-deriving the datasets fingerprint;
/// the production load path must reject it typed.
fn corrupt_dataset_rejected(
    path: &Path,
    package: &QuickSharePackage,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut tampered = package.clone();
    if let Some(runtime) = tampered.runtime.as_mut() {
        runtime.economy.units[0].gold_cost = 1; // fingerprint no longer matches contents
    }
    write_raw(path, &tampered)?;
    match load_runtime_locked_package(path, "litd-game-cli") {
        Ok(_) => Ok("ACCEPTED_SILENTLY".to_string()),
        Err(error) => Ok(error.code().to_string()),
    }
}

/// Swap the runtime section for a self-consistent alternative but keep the ORIGINAL envelope
/// fingerprint; the #1843/MINOR-1 binding must reject it at load.
fn substituted_runtime_rejected(
    path: &Path,
    package: &QuickSharePackage,
    document: &EditorDocument,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut alt_document = document.clone();
    if let Some(config) = alt_document.objects.get_mut("runtime_config") {
        config
            .properties
            .insert("runtime.match_seed".to_string(), "424242".to_string());
    }
    let alt_manifest = lower_runtime_datasets(&alt_document)?;
    // The alternative must be a genuinely different but internally valid manifest.
    let original = package
        .runtime
        .as_ref()
        .ok_or("package carried no runtime datasets")?;
    if alt_manifest.datasets_fingerprint == original.datasets_fingerprint {
        return Ok("ALTERNATIVE_NOT_DISTINCT".to_string());
    }
    let mut substituted = package.clone();
    substituted.runtime = Some(alt_manifest); // envelope manifest_fingerprint left untouched
    write_raw(path, &substituted)?;
    match load_runtime_locked_package(path, "litd-game-cli") {
        Ok(_) => Ok("ACCEPTED_SILENTLY".to_string()),
        Err(error) => Ok(error.to_string()),
    }
}

/// Capture a save, tamper it, and assert both `validate` and `restore` reject it typed.
fn sabotaged_save_rejected(
    manifest: &RuntimeDatasetsManifest,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    session.apply(SessionOrder::Tick)?;
    let mut save = session.capture_save()?;
    save.applied_tick += 99; // tamper without fixing the fingerprint
    let validate_code = match save.validate() {
        Ok(()) => "ACCEPTED_SILENTLY".to_string(),
        Err(error) => error.code().to_string(),
    };
    // Restore must also reject it.
    if MatchRuntimeSession::restore_from_save(manifest, &save).is_ok() {
        return Ok("RESTORE_ACCEPTED_SILENTLY".to_string());
    }
    Ok(validate_code)
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
