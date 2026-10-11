//! #1788 phase-2 FSV: 2v2 melee map "Ridgefork Crossing" -- four corner starts, two forces
//! (Emberhold team + Veilspun team), authored 100% through real editor APIs, packaged through
//! `build_runtime_litdgame`, and played headlessly so each team can win a SP-vs-AI config.
//!
//! Proves: (a) four owners lower to dense slots with force_count=2 and same-force allies;
//! (b) package + litd-game load; (c) team A AI destroys all force-1 structures → force-0
//! Victory / force-1 Defeat; (d) swapped team B AI config; (e) dual-run replay hash match;
//! (f) AI never treats allies as strike targets (force-aware enemy set).

use litd_editor::asset_vault::AssetVaultCatalog;
use litd_editor::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, write_quick_share_package,
};
use litd_editor::runtime_datasets_lowering::{build_runtime_litdgame, lower_runtime_datasets};
use litd_editor::terrain_lowering::{
    HeightFieldLowerParams, lower_height_field, lower_nav_bake_input, lower_tile_splat_map,
};
use litd_engine::game_package::load_runtime_locked_package;
use litd_engine::game_runtime::{AllianceState, MatchRuntimeSession};
use litd_engine::nav::{NavCellCoord, bake_navmesh, find_path};
use serde_json::json;

#[path = "../issue1788_m1_skirmish_fsv/faction_data.rs"]
mod faction_data;
#[path = "../issue1788_m1_skirmish_fsv/factions.rs"]
mod factions;

mod ridgefork;
mod skirmish;

use factions::{EMBERHOLD, VEILSPUN};
use ridgefork::{FORCE_EMBER, FORCE_VEIL};
use skirmish::{certutil_sha256, pick_mesh, replay_hash, run_team_skirmish, skirmish_variant};

const SHARE_ID: &str = "issue1788m12v2";
const MAX_PACKAGE_BYTES: usize = 24 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1788_m1_2v2")?;

    let catalog = AssetVaultCatalog::load_default_cached()?;
    let (unit_art, unit_pref) = pick_mesh(&catalog, &["Stan.gltf", "Leela.gltf", "animatedmech"])?;
    let (struct_art, struct_pref) = pick_mesh(
        &catalog,
        &["gate.glb", "tower", "castle-kit", "dungeon-kit", "building"],
    )?;

    let mut document = ridgefork::ridgefork_document()?;
    let mut rawcodes = factions::RawcodeGen::new();
    let emberhold = factions::author_faction(
        &mut document,
        &EMBERHOLD,
        &mut rawcodes,
        &unit_art,
        &struct_art,
    )?;
    let veilspun = factions::author_faction(
        &mut document,
        &VEILSPUN,
        &mut rawcodes,
        &unit_art,
        &struct_art,
    )?;
    ridgefork::author_runtime_and_map(&mut document, &unit_art, &struct_art)?;
    ridgefork::author_ai_profile(
        &mut document,
        "ai_ember_team",
        "Emberhold Marshal",
        "player_one",
        "node_p0",
        &emberhold,
        (4, 4),
        1000,
    )?;
    ridgefork::author_ai_profile(
        &mut document,
        "ai_veil_team",
        "Veilspun Matron",
        "player_three",
        "node_p2",
        &veilspun,
        (4, 11),
        1000,
    )?;
    document.validate()?;

    let _height = lower_height_field(&document, &HeightFieldLowerParams::default())?;
    let _tiles = lower_tile_splat_map(&document)?;
    let nav_input = lower_nav_bake_input(&document, HeightFieldLowerParams::default().cliff_step)?;
    let nav = bake_navmesh(&nav_input.input)?;
    let path = find_path(&nav, NavCellCoord::new(2, 2), NavCellCoord::new(12, 13))?;
    let path_len = path.nodes.len();
    let map_ok = path_len >= 2;

    let full = lower_runtime_datasets(&document)?;
    let full_bp = full.lowered_match.verify()?;
    let full_session = MatchRuntimeSession::from_manifest(&full)?;
    let roster = full_session.match_state().roster();
    let player_count = roster.player_count();
    let force_count = roster.force_count();
    let slot_forces: Vec<u8> = roster.slots().iter().map(|s| s.force).collect();
    let forces_ok = player_count == 4
        && force_count == 2
        && slot_forces == [0, 0, 1, 1]
        && roster.alliance(0, 1)?.state == AllianceState::Ally
        && roster.alliance(2, 3)?.state == AllianceState::Ally
        && roster.are_enemies(0, 2)?
        && roster.are_enemies(1, 3)?;
    let placed_per_owner: Vec<usize> = (0..4)
        .map(|slot| full_bp.units.iter().filter(|u| u.owner == slot).count())
        .collect();
    let placed_ok = placed_per_owner.iter().all(|&n| n == 3);
    let ai_profiles_authored = full.ai_profiles.len();

    let config_a = skirmish_variant(&document, "ai_veil_team");
    let request = QuickShareBuildRequest::new(
        SHARE_ID,
        "issue1788.2v2.fsv",
        QuickShareEndpoint::loopback(SHARE_ID, 8799),
        MAX_PACKAGE_BYTES,
    );
    let package = build_runtime_litdgame(&config_a, &request)?;
    let shipped = package
        .runtime
        .as_ref()
        .ok_or("packaged .litdgame carried no runtime datasets")?;
    let datasets_fingerprint = shipped.datasets_fingerprint.clone();
    let package_path = run.run_dir().join("litd-m1-ridgefork-2v2.litdgame.json");
    let writeback = write_quick_share_package(&package_path, &package, MAX_PACKAGE_BYTES)?;

    let loaded = load_runtime_locked_package(&package_path, "litd-game-cli")?;
    let manifest_a = loaded
        .runtime
        .clone()
        .ok_or("loaded package carried no runtime datasets")?;
    let load_fingerprint_matches = manifest_a.datasets_fingerprint == datasets_fingerprint;
    let packaged_ai = manifest_a.ai_profiles.len();
    let packaged_slots = {
        let s = MatchRuntimeSession::from_manifest(&manifest_a)?;
        s.match_state().roster().player_count()
    };

    let outcome_a = run_team_skirmish(&manifest_a, &[0, 1], &[2, 3])?;
    let replay_a1 = replay_hash(&manifest_a, 0)?;
    let replay_a2 = replay_hash(&manifest_a, 0)?;
    let replay_match = replay_a1 == replay_a2 && replay_a1 == outcome_a.final_hash;

    let config_b = skirmish_variant(&document, "ai_ember_team");
    let manifest_b = lower_runtime_datasets(&config_b)?;
    let outcome_b = run_team_skirmish(&manifest_b, &[2, 3], &[0, 1])?;

    let a_terminal = outcome_a.victory_tick.is_some()
        && outcome_a.force0_results.iter().all(|r| r == "Victory")
        && outcome_a.force1_results.iter().all(|r| r == "Defeat");
    let b_terminal = outcome_b.victory_tick.is_some()
        && outcome_b.force0_results.iter().all(|r| r == "Victory")
        && outcome_b.force1_results.iter().all(|r| r == "Defeat");
    let both_teams_win = a_terminal && b_terminal;
    // Enemy force: 2 players × 2 starting structures each = 4 (soldiers are not structures).
    let enemy_set_ok = outcome_a.enemy_struct_count == 4
        && outcome_b.enemy_struct_count == 4
        && outcome_a.ally_not_in_enemy_set
        && outcome_b.ally_not_in_enemy_set;

    let verdict = map_ok
        && forces_ok
        && placed_ok
        && ai_profiles_authored == 2
        && load_fingerprint_matches
        && packaged_ai == 1
        && packaged_slots == 4
        && a_terminal
        && b_terminal
        && both_teams_win
        && replay_match
        && enemy_set_ok;

    let report = json!({
        "schema": "litd.editor.issue1788.m1_2v2_ridgefork_fsv.v1",
        "issue": 1788,
        "parents": [1784, 1783, 836, 838],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "One 2v2 melee map (Ridgefork Crossing) with four corner starts and two forces (Emberhold team slots 0+1, Veilspun team slots 2+3), authored via EditorDocument + template.force_id, lowered through the real runtime_datasets valve. Roster proves force_count=2, same-force Ally+shared vision, cross-force Enemy. Packaged .litdgame loads on litd-game with 4 slots and 1 active AI profile. Each team AI config plays to terminal Victory for the whole force and Defeat for the enemy force; AI enemy structure set has exactly 4 cross-force structures (2 players x 2 halls; never allies). Dual-run replay hash matches. Minimal set: one map + two team knobs prove the 2v2 map+force pipeline; full campaign/assets remain separate #1788 slices.",
        "package_sha256": writeback.sha256,
        "datasets_fingerprint": datasets_fingerprint,
        "load_fingerprint_matches": load_fingerprint_matches,
        "vault_art": {
            "unit_model": unit_art.relative_path,
            "unit_model_sha256": unit_art.sha256,
            "unit_preferred": unit_pref,
            "structure_model": struct_art.relative_path,
            "structure_model_sha256": struct_art.sha256,
            "structure_preferred": struct_pref,
        },
        "map": {
            "id": "ridgefork_crossing",
            "grid": ridgefork::GRID,
            "path_len_sw_to_ne": path_len,
            "map_ok": map_ok,
            "player_count": player_count,
            "force_count": force_count,
            "slot_forces": slot_forces,
            "forces_ok": forces_ok,
            "placed_per_owner": placed_per_owner,
            "placed_ok": placed_ok,
            "ai_profiles_authored": ai_profiles_authored,
            "force_ember": FORCE_EMBER,
            "force_veil": FORCE_VEIL,
        },
        "packaged": {
            "ai_profiles": packaged_ai,
            "player_slots": packaged_slots,
        },
        "skirmish_a_ember_team_ai": {
            "computer_opponent": outcome_a.active_display,
            "active_slots": [0, 1],
            "enemy_slots": [2, 3],
            "active_results": outcome_a.force0_results,
            "enemy_results": outcome_a.force1_results,
            "victory_tick": outcome_a.victory_tick,
            "final_state_hash": format!("{:016x}", outcome_a.final_hash),
            "enemy_struct_count": outcome_a.enemy_struct_count,
            "ally_not_in_enemy_set": outcome_a.ally_not_in_enemy_set,
        },
        "skirmish_b_veil_team_ai": {
            "computer_opponent": outcome_b.active_display,
            "active_slots": [2, 3],
            "enemy_slots": [0, 1],
            "active_results": outcome_b.force0_results,
            "enemy_results": outcome_b.force1_results,
            "victory_tick": outcome_b.victory_tick,
            "final_state_hash": format!("{:016x}", outcome_b.final_hash),
            "enemy_struct_count": outcome_b.enemy_struct_count,
            "ally_not_in_enemy_set": outcome_b.ally_not_in_enemy_set,
        },
        "both_teams_win": both_teams_win,
        "enemy_set_ok": enemy_set_ok,
        "replay": { "hash": format!("{:016x}", replay_a1), "match": replay_match },
    });

    let readback = run.write_report(&report)?;
    let report_sha_certutil = certutil_sha256(&readback.path);
    println!(
        "ISSUE1788_M1_2V2_FSV verdict={} report={} report_sha256={} report_sha256_certutil={} package_sha256={} forces_ok={} map_ok={} packaged_slots={} packaged_ai={} skirmishA_tick={:?} skirmishB_tick={:?} both_teams_win={} enemy_set_ok={} replay_match={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        report_sha_certutil.as_deref().unwrap_or("NA"),
        writeback.sha256,
        forces_ok,
        map_ok,
        packaged_slots,
        packaged_ai,
        outcome_a.victory_tick,
        outcome_b.victory_tick,
        both_teams_win,
        enemy_set_ok,
        replay_match,
    );
    println!("-- Team A (Ember AI) timeline --");
    for line in outcome_a.highlights.iter().take(20) {
        println!("  {line}");
    }
    println!("-- Team B (Veil AI) timeline --");
    for line in outcome_b.highlights.iter().take(20) {
        println!("  {line}");
    }
    if !verdict {
        return Err("ISSUE1788 2v2 FSV verdict RED: an acceptance predicate failed".into());
    }
    Ok(())
}
