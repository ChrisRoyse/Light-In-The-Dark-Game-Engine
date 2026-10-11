//! #1788 full-roster end-user ship FSV: produce the M1 ship *set* through real packaging
//! paths, then independently re-read every artifact from disk.
//!
//! Ship set (SoT = on-disk package bytes + litd-game load):
//! 1. `litd-m1-skirmish.litdgame.json` — full dual-faction catalog, Ashford 1v1
//! 2. `litd-m1-ridgefork-2v2.litdgame.json` — full catalog, Ridgefork 2v2 forces
//! 3. `held-to-the-last.litdgame.json` — 3-mission campaign + Doran hero slot
//!
//! Plus a `m1-ship-manifest.json` listing sha256 of each package (readback after write).
//! Edges: (1) census==packaged for skirmish, (2) 2v2 force_count=2, (3) campaign 3 chapters
//! + corrupt package rejected.

use std::collections::BTreeMap;
use std::path::Path;

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::{AssetVaultCatalog, AssetVaultRecord};
use litd_editor::campaign::{
    Campaign, CampaignChapter, CampaignMetadata, RuntimeCampaignHeroSlot,
    RuntimeCampaignSharedDatum, RuntimeSharedObjectCategory, build_campaign_litdgame,
};
use litd_editor::history::{EditorDocument, EditorObject};
use litd_editor::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, write_quick_share_package,
};
use litd_editor::runtime_datasets_lowering::{build_runtime_litdgame, lower_runtime_datasets};
use litd_engine::game_package::load_runtime_locked_package;
use litd_engine::game_runtime::{AllianceState, MatchRuntimeSession};
use litd_engine::math::{Transform, Vec3};
use litd_hash::sha256_hex;
use serde_json::json;

#[path = "../issue1788_m1_skirmish_fsv/ashford.rs"]
mod ashford;
#[path = "../issue1788_m1_skirmish_fsv/faction_data.rs"]
mod faction_data;
#[path = "../issue1788_m1_skirmish_fsv/factions.rs"]
mod factions;
#[path = "../issue1788_m1_skirmish_fsv/packaged.rs"]
mod packaged;
#[path = "../issue1788_m1_2v2_fsv/ridgefork.rs"]
mod ridgefork;

use factions::{EMBERHOLD, STRUCTURES_PER_FACTION, UNITS_PER_FACTION, VEILSPUN};
use packaged::read_packaged_catalog;

const MAX_BYTES: usize = 24 * 1024 * 1024;
const MAX_CAMPAIGN_BYTES: usize = 512 * 1024;
const HERO_KEY: &str = "hero_doran_ashheart";
const CAMPAIGN_ID: &str = "camp.held-to-the-last";

fn is_mesh(r: &AssetVaultRecord) -> bool {
    let p = r.relative_path.to_ascii_lowercase();
    p.ends_with(".gltf") || p.ends_with(".glb") || p.ends_with(".mdl")
}

fn pick_mesh(catalog: &AssetVaultCatalog, prefer: &[&str]) -> Result<AssetVaultRecord, String> {
    for token in prefer {
        if let Some(r) = catalog
            .records()
            .iter()
            .find(|r| is_mesh(r) && r.relative_path.contains(token))
        {
            return Ok(r.clone());
        }
    }
    catalog
        .records()
        .iter()
        .find(|r| is_mesh(r))
        .cloned()
        .ok_or_else(|| "no mesh in vault".to_string())
}

fn skirmish_variant(base: &EditorDocument, remove: &str) -> EditorDocument {
    let mut d = base.clone();
    let _ = d.remove_object(remove);
    d
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1788_m1_full_roster_ship")?;

    let catalog = AssetVaultCatalog::load_default_cached()?;
    let unit_art = pick_mesh(&catalog, &["Stan.gltf", "Leela.gltf", "George.gltf"])?;
    let struct_art = pick_mesh(&catalog, &["gate.glb", "tower", "castle", "building"])?;

    // --- Full dual-faction content on Ashford 1v1 ---
    let mut ashford_doc = ashford::ashford_document()?;
    let mut rawcodes = factions::RawcodeGen::new();
    let ember = factions::author_faction(
        &mut ashford_doc,
        &EMBERHOLD,
        &mut rawcodes,
        &unit_art,
        &struct_art,
    )?;
    let veil = factions::author_faction(
        &mut ashford_doc,
        &VEILSPUN,
        &mut rawcodes,
        &unit_art,
        &struct_art,
    )?;
    ashford::author_runtime_and_map(&mut ashford_doc, &unit_art, &struct_art)?;
    ashford::author_ai_profile(
        &mut ashford_doc,
        "ai_emberhold",
        "Emberhold Marshal",
        EMBERHOLD.owner,
        "node_emberhold_main",
        &ember,
        (4, 4),
        1000,
    )?;
    ashford::author_ai_profile(
        &mut ashford_doc,
        "ai_veilspun",
        "Veilspun Matron",
        VEILSPUN.owner,
        "node_veilspun_main",
        &veil,
        (11, 11),
        1000,
    )?;
    ashford_doc.validate()?;

    let config_1v1 = skirmish_variant(&ashford_doc, "ai_emberhold");
    let req_1v1 = QuickShareBuildRequest::new(
        "issue1788m1ship1v1",
        "issue1788.ship.fsv",
        QuickShareEndpoint::loopback("issue1788m1ship1v1", 8801),
        MAX_BYTES,
    );
    let pkg_1v1 = build_runtime_litdgame(&config_1v1, &req_1v1)?;
    let path_1v1 = run.run_dir().join("litd-m1-skirmish.litdgame.json");
    let wb_1v1 = write_quick_share_package(&path_1v1, &pkg_1v1, MAX_BYTES)?;
    let loaded_1v1 = load_runtime_locked_package(&path_1v1, "litd-game-cli")?;
    let manifest_1v1 = loaded_1v1
        .runtime
        .as_ref()
        .ok_or("1v1 package missing runtime")?;
    let cat_1v1 = read_packaged_catalog(&path_1v1, manifest_1v1)?;
    let ship_1v1_ok = cat_1v1.matches_expected()
        && cat_1v1.census_matches(UNITS_PER_FACTION, STRUCTURES_PER_FACTION);

    // --- Full dual-faction content on Ridgefork 2v2 ---
    let mut ridge_doc = ridgefork::ridgefork_document()?;
    let mut raw2 = factions::RawcodeGen::new();
    let ember2 = factions::author_faction(
        &mut ridge_doc,
        &EMBERHOLD,
        &mut raw2,
        &unit_art,
        &struct_art,
    )?;
    let veil2 =
        factions::author_faction(&mut ridge_doc, &VEILSPUN, &mut raw2, &unit_art, &struct_art)?;
    ridgefork::author_runtime_and_map(&mut ridge_doc, &unit_art, &struct_art)?;
    ridgefork::author_ai_profile(
        &mut ridge_doc,
        "ai_ember_team",
        "Emberhold Marshal",
        "player_one",
        "node_p0",
        &ember2,
        (4, 4),
        1000,
    )?;
    ridgefork::author_ai_profile(
        &mut ridge_doc,
        "ai_veil_team",
        "Veilspun Matron",
        "player_three",
        "node_p2",
        &veil2,
        (4, 11),
        1000,
    )?;
    ridge_doc.validate()?;
    let config_2v2 = skirmish_variant(&ridge_doc, "ai_veil_team");
    let req_2v2 = QuickShareBuildRequest::new(
        "issue1788m1ship2v2",
        "issue1788.ship.fsv",
        QuickShareEndpoint::loopback("issue1788m1ship2v2", 8802),
        MAX_BYTES,
    );
    let pkg_2v2 = build_runtime_litdgame(&config_2v2, &req_2v2)?;
    let path_2v2 = run.run_dir().join("litd-m1-ridgefork-2v2.litdgame.json");
    let wb_2v2 = write_quick_share_package(&path_2v2, &pkg_2v2, MAX_BYTES)?;
    let loaded_2v2 = load_runtime_locked_package(&path_2v2, "litd-game-cli")?;
    let manifest_2v2 = loaded_2v2
        .runtime
        .as_ref()
        .ok_or("2v2 package missing runtime")?;
    let session_2v2 = MatchRuntimeSession::from_manifest(manifest_2v2)?;
    let roster = session_2v2.match_state().roster();
    let ship_2v2_ok = roster.player_count() == 4
        && roster.force_count() == 2
        && roster.alliance(0, 1)?.state == AllianceState::Ally
        && roster.are_enemies(0, 2)?;

    // --- Campaign ship ---
    let (campaign, chapter_maps) = author_campaign(&unit_art)?;
    campaign.validate()?;
    let req_c = QuickShareBuildRequest::new(
        CAMPAIGN_ID,
        "issue1788.ship.fsv",
        QuickShareEndpoint::loopback(CAMPAIGN_ID, 8803),
        MAX_CAMPAIGN_BYTES,
    );
    let built_c = build_campaign_litdgame(&campaign, &chapter_maps, &req_c)?;
    let path_c = run.run_dir().join("held-to-the-last.litdgame.json");
    let wb_c = write_quick_share_package(&path_c, &built_c.package, MAX_CAMPAIGN_BYTES)?;
    let loaded_c = load_runtime_locked_package(&path_c, "litd-game-cli")?;
    let camp = loaded_c
        .campaign
        .as_ref()
        .ok_or("campaign package missing campaign section")?;
    let ship_campaign_ok = camp.chapters.len() == 3
        && camp.entry_chapter_id == "m1"
        && camp.campaign_id == CAMPAIGN_ID
        && !camp.hero_carryover_slots.is_empty();

    // --- Ship manifest SoT: write then independent read ---
    let ship_manifest = json!({
        "schema": "litd.m1.ship_manifest.v1",
        "issue": 1788,
        "packages": [
            {
                "role": "skirmish_1v1",
                "path": path_1v1.file_name().and_then(|s| s.to_str()),
                "sha256": wb_1v1.sha256,
                "full_roster_economy_per_faction": 14,
            },
            {
                "role": "skirmish_2v2",
                "path": path_2v2.file_name().and_then(|s| s.to_str()),
                "sha256": wb_2v2.sha256,
                "players": 4,
                "forces": 2,
            },
            {
                "role": "campaign",
                "path": path_c.file_name().and_then(|s| s.to_str()),
                "sha256": wb_c.sha256,
                "chapters": 3,
                "hero": HERO_KEY,
            },
        ],
    });
    let manifest_path = run.run_dir().join("m1-ship-manifest.json");
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&ship_manifest)?)?;
    let manifest_bytes = std::fs::read(&manifest_path)?;
    let manifest_sha = sha256_hex(&manifest_bytes);
    let reloaded: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let packages = reloaded["packages"]
        .as_array()
        .ok_or("ship manifest packages missing")?;
    let manifest_ok = packages.len() == 3
        && packages[0]["sha256"] == wb_1v1.sha256
        && packages[1]["sha256"] == wb_2v2.sha256
        && packages[2]["sha256"] == wb_c.sha256;

    // Edge: corrupt 1v1 package rejected by litd-game
    let corrupt_path = run.run_dir().join("litd-m1-skirmish-corrupt.litdgame.json");
    let mut corrupt_val: serde_json::Value = serde_json::from_slice(&std::fs::read(&path_1v1)?)?;
    if let Some(fp) = corrupt_val.get_mut("manifest_fingerprint") {
        *fp = json!("deadbeef_corrupt_fingerprint");
    }
    std::fs::write(&corrupt_path, serde_json::to_vec_pretty(&corrupt_val)?)?;
    let corrupt_rejected = load_runtime_locked_package(&corrupt_path, "litd-game-cli").is_err();

    // Edge: 1v1 load fingerprint stable on re-read
    let bytes_a = std::fs::read(&path_1v1)?;
    let bytes_b = std::fs::read(&path_1v1)?;
    let reread_stable =
        sha256_hex(&bytes_a) == sha256_hex(&bytes_b) && sha256_hex(&bytes_a) == wb_1v1.sha256;

    // Edge: full lower of ashford has 2 AI profiles authored
    let full_ai = lower_runtime_datasets(&ashford_doc)?.ai_profiles.len();

    let verdict = ship_1v1_ok
        && ship_2v2_ok
        && ship_campaign_ok
        && manifest_ok
        && corrupt_rejected
        && reread_stable
        && full_ai == 2;

    let report = json!({
        "schema": "litd.editor.issue1788.m1_full_roster_ship_fsv.v1",
        "issue": 1788,
        "parents": [1784, 1783],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "End-user M1 ship set is three real .litdgame artifacts (1v1 full-roster skirmish, 2v2 forces, 3-mission campaign) plus an independent ship-manifest.json read back from disk. Each package is load-validated by litd-game. Packaged economy census equals authored 8+6 per faction. Corrupt package rejected. Minimal set: one full-roster 1v1 + multi-player map + campaign proves the ship roster without requiring full #1878 vault floors for combat anims.",
        "ship_manifest_path": manifest_path.display().to_string(),
        "ship_manifest_sha256": manifest_sha,
        "packages": {
            "skirmish_1v1": {
                "sha256": wb_1v1.sha256,
                "census_matches": ship_1v1_ok,
                "ember_econ_units": cat_1v1.emberhold.economy_unit_records,
                "veil_econ_units": cat_1v1.veilspun.economy_unit_records,
            },
            "skirmish_2v2": {
                "sha256": wb_2v2.sha256,
                "players": roster.player_count(),
                "forces": roster.force_count(),
                "ok": ship_2v2_ok,
            },
            "campaign": {
                "sha256": wb_c.sha256,
                "chapters": camp.chapters.len(),
                "ok": ship_campaign_ok,
            },
        },
        "edges": {
            "corrupt_rejected": corrupt_rejected,
            "reread_stable": reread_stable,
            "full_ai_profiles_authored": full_ai,
            "manifest_ok": manifest_ok,
        },
        "vault_unit_art": unit_art.relative_path,
        "vault_struct_art": struct_art.relative_path,
    });

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1788_M1_FULL_ROSTER_SHIP_FSV verdict={} report={} report_sha256={} ship_manifest_sha256={} 1v1={} 2v2={} campaign={} corrupt_rejected={} reread_stable={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        manifest_sha,
        ship_1v1_ok,
        ship_2v2_ok,
        ship_campaign_ok,
        corrupt_rejected,
        reread_stable,
    );
    if !verdict {
        return Err("ISSUE1788 full-roster ship FSV RED".into());
    }
    Ok(())
}

fn author_campaign(
    unit_art: &AssetVaultRecord,
) -> Result<(Campaign, BTreeMap<String, EditorDocument>), Box<dyn std::error::Error>> {
    let mut campaign = Campaign::new(
        CAMPAIGN_ID,
        CampaignMetadata {
            title: "Held to the Last".to_string(),
            description: "M1 hero campaign ship".to_string(),
            difficulty_tiers: vec!["normal".to_string()],
        },
    );
    campaign.entry_chapter_id = "m1".to_string();
    for (id, order, title, map_ref, unlock) in [
        (
            "m1",
            0u32,
            "The Watch That Held",
            "map_watch",
            &[] as &[&str],
        ),
        ("m2", 1, "Stone Answers Shadow", "map_stone", &["m1"]),
        (
            "m3",
            2,
            "The Last Light Miren Held",
            "map_last_light",
            &["m2"],
        ),
    ] {
        campaign.upsert_chapter(CampaignChapter {
            chapter_id: id.to_string(),
            order,
            title: title.to_string(),
            map_ref: map_ref.to_string(),
            map_document_path: format!("calyx://world/issue1788-{map_ref}"),
            unlock_after: unlock.iter().map(|s| (*s).to_string()).collect(),
            branch_on_victory: Vec::new(),
        });
    }
    campaign
        .shared_object_data
        .push(RuntimeCampaignSharedDatum {
            key: HERO_KEY.to_string(),
            category: RuntimeSharedObjectCategory::Hero,
            definition: json!({"display_name":"Doran Ashheart","role":"hero"}),
        });
    campaign.hero_carryover_slots.push(RuntimeCampaignHeroSlot {
        slot_id: "slot_doran".to_string(),
        hero_key: HERO_KEY.to_string(),
        carries_level: true,
        carries_items: true,
        active_through_chapter: None,
    });
    let mut maps = BTreeMap::new();
    for map_ref in ["map_watch", "map_stone", "map_last_light"] {
        let mut doc = EditorDocument::new(
            format!("calyx://world/issue1788-{map_ref}"),
            format!("issue1788-{map_ref}-rev-1"),
        )?;
        doc.upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))?;
        doc.upsert_object(
            EditorObject::new(
                "unit_doran",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(2.0, 0.0, 2.0)),
            )
            .with_property("faction", "Emberhold Compact"),
        )?;
        assign_vault_model_to_unit(&mut doc, "unit_doran", unit_art)?;
        maps.insert(map_ref.to_string(), doc);
    }
    Ok((campaign, maps))
}
