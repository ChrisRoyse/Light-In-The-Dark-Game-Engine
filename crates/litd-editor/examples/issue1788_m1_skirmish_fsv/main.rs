//! #1788 PHASE-1 FSV: the Milestone-1 RTS content -- two asymmetric factions (Emberhold Compact,
//! Veilspun Court), one 1v1 melee map (Ashford Crossing), one authored computer-opponent profile
//! per faction -- authored 100% through the REAL editor APIs (EditorObject/EditorDocument + the
//! object-data valves), persisted to the Calyx Aster store, lowered cleanly (all cross-refs
//! valid), packaged through the production `build_runtime_litdgame` path into ONE `.litdgame`,
//! and played headlessly to a terminal victory by the authored AI on both faction configurations.
//!
//! Proves: (a) full content lowers cleanly (unit/hero/ability/item/tech/upgrade census asserted,
//! hero->ability + tech-prereq cross-refs validated, art refs bound to real vault hashes); (b)
//! package + litd-game load validates; (c) TWO SP-vs-AI skirmishes reach terminal victory, one
//! per faction (the swapped computer-opponent knob); (d) two executions replay to one state hash;
//! (e) content census; (f) two typed negative controls (removed tech prereq, dangling art ref).

use std::path::Path;

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::{AssetVaultCatalog, AssetVaultRecord};
use litd_editor::history::{EditorDocument, EditorObject};
use litd_editor::mcp_object_tools::{COPY_BASE_OBJECT_TOOL, call_mcp_object_tool};
use litd_editor::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, write_quick_share_package,
};
use litd_editor::runtime_datasets_lowering::{build_runtime_litdgame, lower_runtime_datasets};
use litd_engine::game_package::load_runtime_locked_package;
use litd_engine::math::Transform;
use serde_json::json;

mod ashford;
mod census;
mod faction_data;
mod factions;
mod packaged;
mod skirmish;

use factions::{EMBERHOLD, VEILSPUN};
use packaged::read_packaged_catalog;
use skirmish::{certutil_sha256, replay_hash, run_skirmish, skirmish_variant};

const SHARE_ID: &str = "issue1788m1";
const MAX_PACKAGE_BYTES: usize = 24 * 1024 * 1024;

fn is_mesh(record: &AssetVaultRecord) -> bool {
    // Unit lower only accepts .mdl / .glb / .gltf (#1788 packaging). .fbx/.obj may
    // assign in the vault helper but fail at object-editor lower — reject early.
    let path = record.relative_path.to_ascii_lowercase();
    path.ends_with(".gltf") || path.ends_with(".glb") || path.ends_with(".mdl")
}

/// True if `record` binds cleanly as a unit mesh product (kind Mesh + geometry validates). Some
/// vault meshes carry degenerate geometry (#1822); this rejects them so no dangling/bad art ships.
fn mesh_binds(record: &AssetVaultRecord) -> bool {
    let Ok(mut doc) = EditorDocument::new("calyx://world/issue1788-mesh-probe", "probe-rev-1")
    else {
        return false;
    };
    if doc
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .is_err()
    {
        return false;
    }
    if doc
        .upsert_object(EditorObject::new(
            "probe",
            "Unit",
            Some("world".to_string()),
            Transform::IDENTITY,
        ))
        .is_err()
    {
        return false;
    }
    assign_vault_model_to_unit(&mut doc, "probe", record).is_ok()
}

/// Pick a real, BINDABLE vault mesh: prefer one whose path contains a preferred token, else the
/// first bindable mesh. Returns (record, matched_preferred) for the #1822 asset-gap table.
fn pick_mesh(
    catalog: &AssetVaultCatalog,
    prefer: &[&str],
) -> Result<(AssetVaultRecord, bool), String> {
    for token in prefer {
        if let Some(record) = catalog
            .records()
            .iter()
            .find(|r| is_mesh(r) && r.relative_path.contains(token) && mesh_binds(r))
        {
            return Ok((record.clone(), true));
        }
    }
    catalog
        .records()
        .iter()
        .find(|r| is_mesh(r) && mesh_binds(r))
        .cloned()
        .map(|r| (r, false))
        .ok_or_else(|| "no bindable mesh record in vault".to_string())
}

fn author_both_ai(
    document: &mut EditorDocument,
    emberhold: &factions::FactionIds,
    veilspun: &factions::FactionIds,
) -> Result<(), Box<dyn std::error::Error>> {
    ashford::author_ai_profile(
        document,
        "ai_emberhold",
        "Emberhold Marshal",
        EMBERHOLD.owner,
        "node_emberhold_main",
        emberhold,
        (4, 4),
        1000,
    )?;
    ashford::author_ai_profile(
        document,
        "ai_veilspun",
        "Veilspun Matron",
        VEILSPUN.owner,
        "node_veilspun_main",
        veilspun,
        (11, 11),
        1000,
    )?;
    Ok(())
}

/// The FULL content document: both factions' entire authored record set + map + both AI
/// profiles. Used for the census + cross-ref lowering + negative controls AND packaged whole
/// into the shipped .litdgame (#1849 lifted the preview-sim packaging cap, so the complete
/// 34-Unit-object catalog packages without truncation).
fn build_content_document(
    unit_art: &AssetVaultRecord,
    struct_art: &AssetVaultRecord,
) -> Result<(EditorDocument, factions::FactionIds, factions::FactionIds), Box<dyn std::error::Error>>
{
    let mut document = ashford::ashford_document()?;
    let mut rawcodes = factions::RawcodeGen::new();
    let emberhold = factions::author_faction(
        &mut document,
        &EMBERHOLD,
        &mut rawcodes,
        unit_art,
        struct_art,
    )?;
    let veilspun = factions::author_faction(
        &mut document,
        &VEILSPUN,
        &mut rawcodes,
        unit_art,
        struct_art,
    )?;
    ashford::author_runtime_and_map(&mut document, unit_art, struct_art)?;
    author_both_ai(&mut document, &emberhold, &veilspun)?;
    document.validate()?;
    Ok((document, emberhold, veilspun))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1788_m1_skirmish")?;

    // --- Real vault art: humanoid mesh for units/heroes, structure mesh for buildings. ---
    let catalog = AssetVaultCatalog::load_default_cached()?;
    let (unit_art, unit_pref) = pick_mesh(&catalog, &["Stan.gltf", "Leela.gltf", "animatedmech"])?;
    let (struct_art, struct_pref) = pick_mesh(
        &catalog,
        &[
            "gate.glb",
            "tower",
            "castle-kit",
            "dungeon-kit",
            "modular",
            "building",
        ],
    )?;

    // --- Author the full phase-1 content through the real editor APIs. ---
    let (content, emberhold, veilspun) = build_content_document(&unit_art, &struct_art)?;

    // --- Persist the object-data upgrade records to the Calyx Aster object store (real editor
    // persistence); this enables the p2 (upgrade) valve which re-reads the persisted document.
    // The object store carries object-data kinds, so the runtime/world/AI document persists
    // separately as the packaged .litdgame (below). ---
    let mut upgrade_doc = EditorDocument::new("calyx://world/issue1788-upgrades", "upg-rev-1")?;
    let mut upg_rawcodes = factions::RawcodeGen::new();
    factions::author_upgrade_records(&mut upgrade_doc, &EMBERHOLD, &mut upg_rawcodes)?;
    factions::author_upgrade_records(&mut upgrade_doc, &VEILSPUN, &mut upg_rawcodes)?;
    let aster_dir = run.artifact_dir("aster")?;
    let aster_path = aster_dir.join("ashford-upgrades-doc");
    let ledger_path = aster_dir.join("ashford-ledger");
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": aster_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue1788-ledger",
            "request_id": "issue1788-seed",
            "requested_by": "issue1788-fsv",
            "initial_document": serde_json::to_value(&upgrade_doc)?,
            "new_object_id": "probe_seed_upgrade",
            "rawcode": "9999",
            "base": {
                "base_object_id": "base.upgrade.probe",
                "kind": "Upgrade",
                "display_name": "Probe",
                "properties": {
                    "upgrade.effect_kind": "attack",
                    "upgrade.level_count": "1",
                    "upgrade.level_deltas": "1",
                    "upgrade.gold_costs": "100",
                    "upgrade.lumber_costs": "0",
                    "upgrade.research_times": "30",
                },
            },
            "overrides": {},
        }),
    )?;

    // --- Census: lower every authored record through its real valve; assert exact counts. ---
    let census_ember = census::lower_faction(&content, &aster_path, &emberhold)?;
    let census_veil = census::lower_faction(&content, &aster_path, &veilspun)?;
    census::lower_tech(&content)?;
    let constants_delta_ok = census::lower_constants_with_delta(&content)?;
    let (tile_palette, nav_cells, path_len) = census::lower_map(&content)?;
    let census_ok = census_ember.matches_expected() && census_veil.matches_expected();
    let map_ok = path_len >= 2 && nav_cells > 0 && tile_palette >= 1;

    // Both authored AI profiles lower from the full document (one computer opponent per faction).
    let full_manifest = lower_runtime_datasets(&content)?;
    let ai_profiles_authored = full_manifest.ai_profiles.len();

    // --- Negative controls (typed). ---
    let tech_neg =
        census::negative_tech_prereq(&content, &factions::tech_id(&EMBERHOLD, "stonecraft"));
    let art_neg =
        census::negative_dangling_art(&content, &factions::unit_id(&EMBERHOLD, "shieldsworn"));
    let negatives_ok = census::negative_controls_pass(&tech_neg, &art_neg);

    // --- Package configuration A (Veilspun AI active) into the ONE shipped .litdgame. The
    // packaged/playable document is the FULL authored content document (#1849 cap lifted). ---
    let config_a = skirmish_variant(&content, "ai_emberhold");
    let request = QuickShareBuildRequest::new(
        SHARE_ID,
        "issue1788.fsv",
        QuickShareEndpoint::loopback(SHARE_ID, 8798),
        MAX_PACKAGE_BYTES,
    );
    let package = build_runtime_litdgame(&config_a, &request)?;
    let shipped = package
        .runtime
        .as_ref()
        .ok_or("packaged .litdgame carried no runtime datasets")?;
    let datasets_fingerprint = shipped.datasets_fingerprint.clone();
    let package_path = run.run_dir().join("litd-m1-skirmish.litdgame.json");
    let writeback = write_quick_share_package(&package_path, &package, MAX_PACKAGE_BYTES)?;

    // --- litd-game load validates the packaged artifact. ---
    let loaded = load_runtime_locked_package(&package_path, "litd-game-cli")?;
    let manifest_a = loaded
        .runtime
        .clone()
        .ok_or("loaded package carried no runtime datasets")?;
    let load_fingerprint_matches = manifest_a.datasets_fingerprint == datasets_fingerprint;

    // --- Packaged catalog READBACK: read the shipped .litdgame bytes off disk and count the
    // per-faction records ACTUALLY inside it. With the #1849 cap lifted this is the FULL
    // authored catalog: census == packaged, proven from the shipped bytes. ---
    let packaged = read_packaged_catalog(&package_path, &manifest_a)?;
    let packaged_ok = packaged.matches_expected();
    let census_matches_packaged = census_ok
        && packaged.census_matches(
            factions::UNITS_PER_FACTION,
            factions::STRUCTURES_PER_FACTION,
        );

    // --- Skirmish A (packaged artifact): Veilspun AI (slot 1) vs passive Emberhold (slot 0). ---
    let outcome_a = run_skirmish(&manifest_a, 1, 0)?;
    // Two executions replay to one state hash (determinism).
    let replay_a1 = replay_hash(&manifest_a, 1)?;
    let replay_a2 = replay_hash(&manifest_a, 1)?;
    let replay_match = replay_a1 == replay_a2 && replay_a1 == outcome_a.final_hash;

    // --- Skirmish B (swap the computer-opponent knob): Emberhold AI (slot 0) vs passive Veilspun. ---
    let config_b = skirmish_variant(&content, "ai_veilspun");
    let manifest_b = lower_runtime_datasets(&config_b)?;
    let outcome_b = run_skirmish(&manifest_b, 0, 1)?;

    let a_terminal = outcome_a.victory_tick.is_some()
        && outcome_a.result_active == "Victory"
        && outcome_a.result_enemy == "Defeat";
    let b_terminal = outcome_b.victory_tick.is_some()
        && outcome_b.result_active == "Victory"
        && outcome_b.result_enemy == "Defeat";
    let both_factions_win = a_terminal && b_terminal;

    let verdict = census_ok
        && constants_delta_ok
        && map_ok
        && ai_profiles_authored == 2
        && load_fingerprint_matches
        && packaged_ok
        && census_matches_packaged
        && a_terminal
        && b_terminal
        && both_factions_win
        && replay_match
        && negatives_ok;

    let report = json!({
        "schema": "litd.editor.issue1788.m1_skirmish_phase1_fsv.v1",
        "issue": 1788, "parents": [1784, 1786, 1843],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "Two asymmetric factions + one 1v1 melee map + one authored AI profile per faction, all authored through the real EditorObject/object-data-valve APIs, persisted to the Calyx Aster store, and lowered cleanly through the census valves (unit/hero/ability/item/tech/upgrade census asserted; hero->ability and tech-prerequisite cross-refs validated; art refs bound to real vault content hashes). With the #1849 packaging cap lifted, the shipped litd-m1-skirmish.litdgame is built through build_runtime_litdgame from the FULL content document: `packaged_catalog` is read back from the shipped package bytes and equals `census_authored` for every unit/structure TYPE record (8 mobile units + 6 structures = 14 economy records per faction, census_matches_packaged=true) plus 3 placed starting units per faction -- nothing is truncated and litd-game load-validates the package. The authored computer opponent plays each configuration to a terminal victory headlessly against an inert opponent; swapping which faction is the active computer opponent shows each faction's dataset drives the pipeline to a terminal result -- this proves pipeline LIVENESS + DETERMINISM on both faction datasets, NOT balance. Two executions replay to one state hash. Two typed negative controls (removed tech prerequisite, dangling art ref) fail loud.",
        "package_sha256": writeback.sha256,
        "datasets_fingerprint": datasets_fingerprint,
        "load_fingerprint_matches": load_fingerprint_matches,
        "census_matches_packaged": census_matches_packaged,
        "vault_art": {
            "unit_model": unit_art.relative_path, "unit_model_sha256": unit_art.sha256, "unit_preferred": unit_pref,
            "structure_model": struct_art.relative_path, "structure_model_sha256": struct_art.sha256, "structure_preferred": struct_pref,
        },
        "census_authored": {
            "shipped": true,
            "note": "Full authored record set, each lowered through its real editor valve (cross-refs validated). The unit/structure TYPE records are ALL shipped -- packaged_catalog reads them back from the .litdgame bytes and census_matches_packaged asserts equality (#1849 cap lifted).",
            "emberhold": census_ember, "veilspun": census_veil,
            "ai_profiles_authored": ai_profiles_authored,
            "map": { "tile_palette": tile_palette, "nav_cells": nav_cells, "start_to_start_path_len": path_len },
            "counts_ok": census_ok, "constants_delta_ok": constants_delta_ok, "map_ok": map_ok,
        },
        "packaged_catalog": {
            "shipped": true,
            "source": "read back from the litd-m1-skirmish.litdgame package bytes (runtime.economy for type records; re-verified lowered_match canonical bytes for placed units)",
            "note": "The FULL authored catalog inside the shipped .litdgame (#1849 cap lifted). economy_unit_records = unit+structure TYPE records (8 mobile + 6 structure = 14 per faction); economy_structure_records = the structure subset (6); placed_starting_units = placed victory/vision units per owner slot (3).",
            "emberhold": packaged.emberhold, "veilspun": packaged.veilspun,
            "ai_profiles_packaged": packaged.ai_profiles,
            "counts_ok": packaged_ok,
            "census_matches_packaged": census_matches_packaged,
        },
        "skirmish_a_veilspun_ai": {
            "computer_opponent": outcome_a.active_display,
            "result_active": outcome_a.result_active, "result_enemy": outcome_a.result_enemy,
            "victory_tick": outcome_a.victory_tick, "final_state_hash": format!("{:016x}", outcome_a.final_hash),
        },
        "skirmish_b_emberhold_ai": {
            "computer_opponent": outcome_b.active_display,
            "result_active": outcome_b.result_active, "result_enemy": outcome_b.result_enemy,
            "victory_tick": outcome_b.victory_tick, "final_state_hash": format!("{:016x}", outcome_b.final_hash),
        },
        "both_factions_win": both_factions_win,
        "asymmetry_knob": "active computer-opponent profile (each faction wins as the AI-driven side)",
        "replay": { "hash": format!("{:016x}", replay_a1), "match": replay_match },
        "negative_controls": { "removed_tech_prereq_code": tech_neg, "dangling_art_ref_code": art_neg, "pass": negatives_ok },
    });

    let readback = run.write_report(&report)?;
    let report_sha_certutil = certutil_sha256(&readback.path);
    println!(
        "ISSUE1788_M1_SKIRMISH_FSV verdict={} report={} report_sha256={} report_sha256_certutil={} package_sha256={} census_authored_ok={} packaged_catalog_ok={} census_matches_packaged={} packaged_per_faction=[econ_units={},econ_structs={},placed={}] packaged_ai_profiles={} skirmishA={}/{} tickA={:?} skirmishB={}/{} tickB={:?} replay_match={} negatives={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        report_sha_certutil.as_deref().unwrap_or("NA"),
        writeback.sha256,
        census_ok,
        packaged_ok,
        census_matches_packaged,
        packaged.emberhold.economy_unit_records,
        packaged.emberhold.economy_structure_records,
        packaged.emberhold.placed_starting_units,
        packaged.ai_profiles,
        outcome_a.result_active,
        outcome_a.result_enemy,
        outcome_a.victory_tick,
        outcome_b.result_active,
        outcome_b.result_enemy,
        outcome_b.victory_tick,
        replay_match,
        negatives_ok,
    );
    println!("-- Skirmish A (Veilspun AI) decision timeline --");
    for line in outcome_a.highlights.iter().take(24) {
        println!("  {line}");
    }
    println!("-- Skirmish B (Emberhold AI) decision timeline --");
    for line in outcome_b.highlights.iter().take(24) {
        println!("  {line}");
    }
    if !verdict {
        return Err("ISSUE1788 phase-1 FSV verdict RED: an acceptance predicate failed".into());
    }
    Ok(())
}
