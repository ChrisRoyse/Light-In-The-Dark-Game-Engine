//! #1788 phase-2 FSV: M1 hero campaign content "Held to the Last" -- 3 missions authored
//! 100% through real editor campaign APIs, persisted to Calyx Aster, packaged through
//! `build_campaign_litdgame`, accepted by litd-game load, with hero carry-over slots for
//! Doran Ashheart. Negative: corrupted campaign manifest is rejected.
//!
//! Minimal-sufficient for the campaign CONTENT slice of #1788 (3+ missions + persistent
//! hero + one packaged .litdgame). Full 5-mission story bible + live cinematics remain
//! follow-on; runtime campaign flow runner is proven by closed #1852.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    campaign::{
        Campaign, CampaignChapter, CampaignMetadata, CampaignStoreRecord, RuntimeCampaignHeroSlot,
        RuntimeCampaignSharedDatum, RuntimeSharedObjectCategory, build_campaign_litdgame,
        read_campaign, read_campaign_ledger, write_campaign, write_campaign_ledger,
    },
    history::{EditorDocument, EditorObject},
    quick_share::{QuickShareBuildRequest, QuickShareEndpoint, write_quick_share_package},
};
use litd_engine::{
    EngineError,
    game_package::load_runtime_locked_package,
    math::{Transform, Vec3},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const HERO_KEY: &str = "hero_doran_ashheart";
const CAMPAIGN_ID: &str = "camp.held-to-the-last";
const MAX_BYTES: usize = 512 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1788_m1_campaign")?;

    let chapter_maps = build_chapter_worlds()?;
    let campaign = author_m1_campaign();
    campaign.validate()?;

    let campaign_vault = run.artifact_dir("campaign-aster")?;
    let ledger_vault = run.artifact_dir("campaign-ledger")?;
    let package_path = run.run_dir().join("held-to-the-last.litdgame.json");
    let corrupt_path = run.run_dir().join("held-to-the-last-corrupt.litdgame.json");

    let write_readback = write_campaign(&campaign_vault, &campaign)?;
    let ledger_readback = write_campaign_ledger(
        &ledger_vault,
        "issue1788-campaign-ledger",
        CampaignStoreRecord {
            request_id: "issue1788-m1-campaign".to_string(),
            requested_by: "issue1788.fsv".to_string(),
            action_summary: "authored Held to the Last 3-mission campaign".to_string(),
            campaign_id: campaign.campaign_id.clone(),
            campaign_sha256: write_readback.sha256.clone(),
            campaign_bytes: write_readback.bytes,
            campaign_fingerprint: write_readback.campaign_fingerprint.clone(),
            chapter_count: write_readback.chapter_count,
        },
    )?;

    let independent = read_campaign(&campaign_vault)?;
    if independent.campaign != campaign {
        return Err("independent campaign readback differs from authored campaign".into());
    }
    let independent_ledger = read_campaign_ledger(&ledger_vault)?;
    if independent_ledger.ledger != ledger_readback.ledger {
        return Err("independent campaign ledger readback differs".into());
    }

    let request = campaign_request(CAMPAIGN_ID);
    let build = build_campaign_litdgame(&campaign, &chapter_maps, &request)?;
    let writeback = write_quick_share_package(&package_path, &build.package, MAX_BYTES)?;
    let loaded = load_runtime_locked_package(&package_path, "litd-game-cli")?;
    let accepted = loaded
        .campaign
        .as_ref()
        .ok_or("packaged campaign missing at litd-game load")?;
    let accept_ok = accepted.chapters.len() == 3
        && accepted.entry_chapter_id == "m1"
        && accepted.campaign_id == CAMPAIGN_ID
        && !accepted.hero_carryover_slots.is_empty()
        && accepted.hero_carryover_slots[0].hero_key == HERO_KEY;

    let titles: Vec<String> = accepted.chapters.iter().map(|c| c.title.clone()).collect();
    let titles_ok = titles
        == [
            "The Watch That Held".to_string(),
            "Stone Answers Shadow".to_string(),
            "The Last Light Miren Held".to_string(),
        ];

    let missing_map = edge_missing_map(&campaign, &chapter_maps)?;
    let negative = negative_control(&package_path, &corrupt_path)?;

    let verdict = accept_ok
        && titles_ok
        && independent.chapter_count == 3
        && independent.hero_slot_count >= 1
        && missing_map["mentions_missing_map"].as_bool() == Some(true)
        && negative["rejected"].as_bool() == Some(true);

    let report = json!({
        "schema": "litd.editor.issue1788.m1_campaign_held_to_the_last_fsv.v1",
        "issue": 1788,
        "parents": [1784, 1783, 853, 1852],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "Three story-bible missions of Held to the Last (M1-M3 titles), authored through real Campaign/EditorDocument APIs with Doran Ashheart hero carry-over slots, persisted to Calyx Aster with independent readback, packaged via build_campaign_litdgame, and accepted by litd-game load. Missing-map edge fails packaging; corrupted manifest is rejected. Proves the #1788 3+ mission hero campaign CONTENT packaging path; full 5-mission arc + live cinematics/quests remain optional extensions; runtime flow runner already closed on #1852.",
        "package_sha256": writeback.sha256,
        "campaign_id": CAMPAIGN_ID,
        "campaign_fingerprint": write_readback.campaign_fingerprint,
        "persist_readback_ok": true,
        "chapter_count": accepted.chapters.len(),
        "entry_chapter_id": accepted.entry_chapter_id,
        "mission_titles": titles,
        "titles_ok": titles_ok,
        "hero_key": HERO_KEY,
        "hero_slots": accepted.hero_carryover_slots.len(),
        "accept_ok": accept_ok,
        "edge_missing_map": missing_map,
        "negative_corrupt_manifest": negative,
    });

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1788_M1_CAMPAIGN_FSV verdict={} report={} report_sha256={} package_sha256={} chapters={} titles_ok={} accept_ok={} hero_slots={} negatives={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        writeback.sha256,
        accepted.chapters.len(),
        titles_ok,
        accept_ok,
        accepted.hero_carryover_slots.len(),
        negative["rejected"],
    );
    if !verdict {
        return Err("ISSUE1788 M1 campaign FSV verdict RED".into());
    }
    Ok(())
}

fn author_m1_campaign() -> Campaign {
    let mut campaign = Campaign::new(
        CAMPAIGN_ID,
        CampaignMetadata {
            title: "Held to the Last".to_string(),
            description: "M1 hero campaign -- Emberhold hold, then the irreversible loss"
                .to_string(),
            difficulty_tiers: vec!["normal".to_string()],
        },
    );
    campaign.entry_chapter_id = "m1".to_string();
    campaign.upsert_chapter(chapter("m1", 0, "The Watch That Held", "map_watch", &[]));
    campaign.upsert_chapter(chapter(
        "m2",
        1,
        "Stone Answers Shadow",
        "map_stone",
        &["m1"],
    ));
    campaign.upsert_chapter(chapter(
        "m3",
        2,
        "The Last Light Miren Held",
        "map_last_light",
        &["m2"],
    ));
    campaign
        .shared_object_data
        .push(RuntimeCampaignSharedDatum {
            key: HERO_KEY.to_string(),
            category: RuntimeSharedObjectCategory::Hero,
            definition: json!({
                "display_name": "Doran Ashheart",
                "faction": "Emberhold Compact",
                "base_level": 1,
                "role": "hero"
            }),
        });
    campaign.hero_carryover_slots.push(RuntimeCampaignHeroSlot {
        slot_id: "slot_doran".to_string(),
        hero_key: HERO_KEY.to_string(),
        carries_level: true,
        carries_items: true,
        active_through_chapter: None,
    });
    campaign
}

fn chapter(id: &str, order: u32, title: &str, map_ref: &str, unlock: &[&str]) -> CampaignChapter {
    CampaignChapter {
        chapter_id: id.to_string(),
        order,
        title: title.to_string(),
        map_ref: map_ref.to_string(),
        map_document_path: format!("calyx://world/issue1788-{map_ref}"),
        unlock_after: unlock.iter().map(|v| (*v).to_string()).collect(),
        branch_on_victory: Vec::new(),
    }
}

fn build_chapter_worlds() -> Result<BTreeMap<String, EditorDocument>, EngineError> {
    let mut maps = BTreeMap::new();
    for map_ref in ["map_watch", "map_stone", "map_last_light"] {
        maps.insert(map_ref.to_string(), world_document(map_ref)?);
    }
    Ok(maps)
}

fn world_document(map_ref: &str) -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new(
        format!("calyx://world/issue1788-{map_ref}"),
        format!("issue1788-{map_ref}-rev-1"),
    )?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_doran",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(2.0, 0.0, 2.0)),
        )
        .with_property("faction", "Emberhold Compact")
        .with_property("template.owner_player_id", "player_one"),
    )?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|r| {
            let p = r.relative_path.to_ascii_lowercase();
            (p.ends_with(".gltf") || p.ends_with(".glb") || p.ends_with(".mdl"))
                && (p.contains("stan")
                    || p.contains("george")
                    || p.contains("leela")
                    || p.contains("kaykit")
                    || p.contains("polyhaven"))
        })
        .cloned()
        .ok_or_else(|| {
            invalid(
                "world.asset",
                "no bindable vault mesh for campaign chapter unit",
            )
        })?;
    assign_vault_model_to_unit(&mut document, "unit_doran", &record)?;
    Ok(document)
}

fn edge_missing_map(
    campaign: &Campaign,
    chapter_maps: &BTreeMap<String, EditorDocument>,
) -> Result<Value, EngineError> {
    let mut broken = campaign.clone();
    broken.upsert_chapter(chapter(
        "m4",
        3,
        "Ghost Mission",
        "map_does_not_exist",
        &["m3"],
    ));
    let request = campaign_request("issue1788-missing");
    let error = build_campaign_litdgame(&broken, chapter_maps, &request)
        .expect_err("missing map must fail packaging")
        .to_string();
    Ok(json!({
        "name": "missing_chapter_map_rejected",
        "error": error,
        "mentions_missing_map": error.contains("map_does_not_exist"),
    }))
}

fn negative_control(package_path: &Path, corrupt_path: &Path) -> Result<Value, EngineError> {
    let bytes = fs::read(package_path).map_err(|e| invalid("negative.read", e.to_string()))?;
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|e| invalid("negative.parse", e.to_string()))?;
    value["campaign"]["chapters"][0]["title"] = Value::String("HIJACKED".to_string());
    let corrupt_bytes = serde_json::to_vec_pretty(&value)
        .map_err(|e| invalid("negative.serialize", e.to_string()))?;
    fs::write(corrupt_path, &corrupt_bytes)
        .map_err(|e| invalid("negative.write", e.to_string()))?;
    let error = match load_runtime_locked_package(corrupt_path, "litd-game-cli") {
        Ok(_) => {
            return Err(invalid(
                "negative.accepted",
                "litd-game accepted a corrupted campaign manifest",
            ));
        }
        Err(e) => e.to_string(),
    };
    Ok(json!({
        "name": "corrupt_campaign_manifest_rejected",
        "corrupt_sha256": sha256_hex(&corrupt_bytes),
        "rejected": true,
        "error": error,
        "mentions_fingerprint": error.contains("fingerprint"),
    }))
}

fn campaign_request(share_id: &str) -> QuickShareBuildRequest {
    QuickShareBuildRequest::new(
        share_id,
        "issue1788.fsv",
        QuickShareEndpoint::loopback(share_id, 11878),
        MAX_BYTES,
    )
}

fn invalid(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("issue1788.campaign_fsv.{field}"),
        reason: reason.into(),
    }
}
