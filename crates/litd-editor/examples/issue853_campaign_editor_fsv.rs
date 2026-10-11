//! #853 FSV: author a 3-chapter single-player campaign over real generated worlds,
//! persist to Calyx, independently read it back, lower + package to `.litdgame`, and
//! prove litd-game's validation ACCEPTS it. Covers three edge cases and a negative
//! control (a corrupted embedded campaign manifest must be REJECTED by litd-game).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    campaign::{
        Campaign, CampaignChapter, CampaignMetadata, CampaignStoreRecord, RuntimeCampaignHeroSlot,
        RuntimeCampaignSharedDatum, RuntimeSharedObjectCategory, build_campaign_litdgame,
        lower_campaign, read_campaign, read_campaign_ledger, write_campaign, write_campaign_ledger,
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

const SHARED_HERO_KEY: &str = "hero_emberblade";
const MAX_BYTES: usize = 512 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE853_FSV_ROOT",
        "issue853_campaign_editor",
        "manual",
    )?;
    let campaign_vault = root.join("campaign-aster");
    let campaign_ledger_vault = root.join("campaign-ledger-aster");
    let package_path = root.join("ashen-campaign.litdgame.json");
    let corrupt_path = root.join("ashen-campaign-corrupt.litdgame.json");
    let artifact_path = root.join("issue853-campaign-readback.json");

    // 1. Author over three real generated worlds. Chapter 2's map authors an object
    //    colliding with the shared hero key to exercise precedence (edge case 2).
    let chapter_maps = build_chapter_worlds()?;
    let campaign = author_campaign();
    campaign.validate()?;

    // 2. Persist to Calyx + record the write ledger.
    let write_readback = write_campaign(&campaign_vault, &campaign)?;
    let ledger_readback = write_campaign_ledger(
        &campaign_ledger_vault,
        "issue853-campaign-ledger",
        CampaignStoreRecord {
            request_id: "issue853-author".to_string(),
            requested_by: "human.qa.ashen".to_string(),
            action_summary: "authored ashen-march campaign".to_string(),
            campaign_id: campaign.campaign_id.clone(),
            campaign_sha256: write_readback.sha256.clone(),
            campaign_bytes: write_readback.bytes,
            campaign_fingerprint: write_readback.campaign_fingerprint.clone(),
            chapter_count: write_readback.chapter_count,
        },
    )?;

    // 3. INDEPENDENT readback: fresh handles re-read the Aster source of truth.
    let independent = read_campaign(&campaign_vault)?;
    if independent.campaign != campaign {
        return Err("independent campaign readback differs from authored campaign".into());
    }
    let independent_ledger = read_campaign_ledger(&campaign_ledger_vault)?;
    if independent_ledger.ledger != ledger_readback.ledger {
        return Err("independent campaign ledger readback differs".into());
    }
    println!(
        "persist+readback campaign_id={} chapters={} shared={} hero_slots={} fp={} ledger_records={}",
        independent.campaign.campaign_id,
        independent.chapter_count,
        independent.shared_datum_count,
        independent.hero_slot_count,
        independent.campaign_fingerprint,
        independent_ledger.ledger.record_count
    );

    // 4. Lower + package to .litdgame, then prove litd-game ACCEPTS it.
    let request = campaign_request("issue853-ashen-march");
    let build = build_campaign_litdgame(&campaign, &chapter_maps, &request)?;
    let writeback = write_quick_share_package(&package_path, &build.package, MAX_BYTES)?;
    let accepted = load_runtime_locked_package(&package_path, "litd-game-cli")?;
    let accepted_campaign = accepted
        .campaign
        .as_ref()
        .ok_or("packaged .litdgame carried no campaign manifest")?;
    if !accepted.manifest_integrity_verified
        || !accepted.runtime_lock_verified
        || !accepted.playable_on_litd_runtime
        || accepted_campaign.chapters.len() != 3
    {
        return Err("litd-game did not accept the campaign package".into());
    }
    println!(
        "litd-game ACCEPT package_sha={} chapters={} entry={} manifest_fp={} integrity={} playable={}",
        writeback.sha256,
        accepted_campaign.chapters.len(),
        accepted_campaign.entry_chapter_id,
        accepted_campaign.manifest_fingerprint,
        accepted.manifest_integrity_verified,
        accepted.playable_on_litd_runtime
    );

    let edge_missing = edge_missing_map(&campaign, &chapter_maps, &root)?;
    let edge_precedence = edge_precedence(accepted_campaign)?;
    let edge_reorder = edge_reorder(&campaign, &chapter_maps)?;
    let negative = negative_control(&package_path, &corrupt_path)?;

    let artifact = json!({
        "issue": 853,
        "source_of_truth": "Calyx Aster campaign vault + campaign write ledger + packaged .litdgame validated by litd-game",
        "artifact_path": artifact_path,
        "happy_path": {
            "campaign_id": campaign.campaign_id,
            "chapters": independent.chapter_count,
            "shared_datum_count": independent.shared_datum_count,
            "hero_slot_count": independent.hero_slot_count,
            "authored_fingerprint": independent.campaign_fingerprint,
            "independent_readback_matches": independent.campaign == campaign,
            "ledger_records": independent_ledger.ledger.record_count,
            "package_path": package_path,
            "package_sha256": writeback.sha256,
            "manifest_fingerprint": accepted_campaign.manifest_fingerprint,
            "litd_game_accepted": accepted.playable_on_litd_runtime && accepted.manifest_integrity_verified,
        },
        "edge_cases": [edge_missing, edge_precedence, edge_reorder],
        "negative_control": negative,
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let disk = fs::read(&artifact_path)?;
    if disk != artifact_bytes {
        return Err("artifact readback mismatch".into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue853 campaign FSV artifact={} bytes={} artifact_sha={} package_sha={}",
        artifact_path.display(),
        disk.len(),
        report_readback.sha256,
        writeback.sha256
    );
    Ok(())
}

/// Edge case 1: a chapter referencing a map with no authored world fails packaging
/// with a typed error, and no package file is written.
fn edge_missing_map(
    campaign: &Campaign,
    chapter_maps: &BTreeMap<String, EditorDocument>,
    root: &Path,
) -> Result<Value, EngineError> {
    let package_path = root.join("edge-missing.litdgame.json");
    let before_file_exists = package_path.exists();
    let mut broken = campaign.clone();
    broken.upsert_chapter(CampaignChapter {
        chapter_id: "ch4".to_string(),
        order: 3,
        title: "Chapter ch4".to_string(),
        map_ref: "map_does_not_exist".to_string(),
        map_document_path: "calyx://world/ghost".to_string(),
        unlock_after: vec!["ch3".to_string()],
        branch_on_victory: Vec::new(),
    });
    let request = campaign_request("issue853-missing");
    let error = build_campaign_litdgame(&broken, chapter_maps, &request)
        .expect_err("missing map must fail packaging")
        .to_string();
    println!(
        "edge_missing before_file={} after_file={} error={}",
        before_file_exists,
        package_path.exists(),
        error
    );
    Ok(json!({
        "name": "missing_chapter_map_rejected",
        "before_file_exists": before_file_exists,
        "after_file_exists": package_path.exists(),
        "state_unchanged": package_path.exists() == before_file_exists,
        "error": error,
        "mentions_missing_map": error.contains("map_does_not_exist"),
    }))
}

/// Edge case 2: shared-data collision with a map-local override resolves by
/// documented precedence â€” the map-local object wins, proven by readback.
fn edge_precedence(
    campaign: &litd_engine::game_package::RuntimeCampaignManifest,
) -> Result<Value, EngineError> {
    let overriding = campaign
        .chapters
        .iter()
        .find(|chapter| chapter.chapter_id == "ch2")
        .ok_or_else(|| invalid("precedence.chapter", "ch2 missing"))?;
    let plain = campaign
        .chapters
        .iter()
        .find(|chapter| chapter.chapter_id == "ch1")
        .ok_or_else(|| invalid("precedence.chapter", "ch1 missing"))?;
    let override_res = overriding
        .shared_data_resolution
        .iter()
        .find(|res| res.key == SHARED_HERO_KEY)
        .ok_or_else(|| invalid("precedence.resolution", "ch2 shared resolution missing"))?;
    let plain_res = plain
        .shared_data_resolution
        .iter()
        .find(|res| res.key == SHARED_HERO_KEY)
        .ok_or_else(|| invalid("precedence.resolution", "ch1 shared resolution missing"))?;
    println!(
        "edge_precedence ch2_winner={} ch1_winner={}",
        override_res.winner, plain_res.winner
    );
    if override_res.winner != "map_local" || plain_res.winner != "campaign_shared" {
        return Err(invalid(
            "precedence.winner",
            "documented precedence did not resolve map-local override",
        ));
    }
    Ok(json!({
        "name": "shared_data_precedence_resolved",
        "chapter_with_override": overriding.chapter_id,
        "override_winner": override_res.winner,
        "override_source": override_res.source,
        "chapter_without_override": plain.chapter_id,
        "no_override_winner": plain_res.winner,
    }))
}

/// Edge case 3: reordering chapters changes the lowered manifest fingerprint
/// deterministically.
fn edge_reorder(
    campaign: &Campaign,
    chapter_maps: &BTreeMap<String, EditorDocument>,
) -> Result<Value, EngineError> {
    let lowered_maps = lower_all_maps(campaign, chapter_maps)?;
    let baseline = lower_campaign(campaign, &lowered_maps)?;
    let mut reordered = campaign.clone();
    // Swap the order indices of ch2 and ch3 (keep entry ch1 first).
    for chapter in &mut reordered.chapters {
        chapter.order = match chapter.chapter_id.as_str() {
            "ch2" => 2,
            "ch3" => 1,
            _ => chapter.order,
        };
    }
    reordered.chapters.sort_by_key(|chapter| chapter.order);
    let reordered_manifest = lower_campaign(&reordered, &lowered_maps)?;
    let recomputed = lower_campaign(&reordered, &lowered_maps)?;
    println!(
        "edge_reorder before_fp={} after_fp={} deterministic={}",
        baseline.manifest_fingerprint,
        reordered_manifest.manifest_fingerprint,
        reordered_manifest.manifest_fingerprint == recomputed.manifest_fingerprint
    );
    if baseline.manifest_fingerprint == reordered_manifest.manifest_fingerprint {
        return Err(invalid(
            "reorder.fingerprint",
            "reordering chapters did not change the manifest fingerprint",
        ));
    }
    if reordered_manifest.manifest_fingerprint != recomputed.manifest_fingerprint {
        return Err(invalid(
            "reorder.determinism",
            "manifest fingerprint is non-deterministic",
        ));
    }
    Ok(json!({
        "name": "reorder_changes_fingerprint_deterministically",
        "before_fingerprint": baseline.manifest_fingerprint,
        "after_fingerprint": reordered_manifest.manifest_fingerprint,
        "changed": baseline.manifest_fingerprint != reordered_manifest.manifest_fingerprint,
        "deterministic": reordered_manifest.manifest_fingerprint == recomputed.manifest_fingerprint,
    }))
}

/// Negative control: corrupt the campaign manifest embedded in the package (mutate a
/// chapter title without fixing the manifest fingerprint) and prove litd-game REJECTS
/// it. If campaign validation were a no-op, this load would succeed and the example
/// would fail â€” so the control fails when the feature is sabotaged.
fn negative_control(package_path: &Path, corrupt_path: &Path) -> Result<Value, EngineError> {
    let bytes =
        fs::read(package_path).map_err(|error| invalid("negative.read", error.to_string()))?;
    let mut value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| invalid("negative.parse", error.to_string()))?;
    let original_title = value["campaign"]["chapters"][0]["title"].clone();
    value["campaign"]["chapters"][0]["title"] = Value::String("HIJACKED CHAPTER".to_string());
    let corrupt_bytes = serde_json::to_vec_pretty(&value)
        .map_err(|error| invalid("negative.serialize", error.to_string()))?;
    fs::write(corrupt_path, &corrupt_bytes)
        .map_err(|error| invalid("negative.write", error.to_string()))?;
    let result = load_runtime_locked_package(corrupt_path, "litd-game-cli");
    let error = match result {
        Ok(_) => {
            return Err(invalid(
                "negative.accepted",
                "litd-game accepted a corrupted campaign manifest (feature is broken)",
            ));
        }
        Err(error) => error.to_string(),
    };
    println!(
        "negative_control original_title={} corrupt_sha={} rejected_error={}",
        original_title,
        sha256_hex(&corrupt_bytes),
        error
    );
    Ok(json!({
        "name": "corrupt_campaign_manifest_rejected",
        "corrupt_package_path": corrupt_path.display().to_string(),
        "corrupt_sha256": sha256_hex(&corrupt_bytes),
        "tampered_field": "campaign.chapters[0].title",
        "rejected": true,
        "error": error,
        "mentions_fingerprint": error.contains("fingerprint"),
    }))
}

fn lower_all_maps(
    campaign: &Campaign,
    chapter_maps: &BTreeMap<String, EditorDocument>,
) -> Result<BTreeMap<String, litd_editor::quick_share::QuickSharePackage>, EngineError> {
    let mut lowered = BTreeMap::new();
    for chapter in &campaign.chapters {
        if lowered.contains_key(&chapter.map_ref) {
            continue;
        }
        let document = chapter_maps
            .get(&chapter.map_ref)
            .ok_or_else(|| invalid("lower.map", format!("missing map {}", chapter.map_ref)))?;
        let request = campaign_request(&format!("issue853-{}", chapter.map_ref));
        lowered.insert(
            chapter.map_ref.clone(),
            litd_editor::quick_share::build_quick_share_package(document, &request)?,
        );
    }
    Ok(lowered)
}

fn author_campaign() -> Campaign {
    let mut campaign = Campaign::new(
        "camp.ashen-march",
        CampaignMetadata {
            title: "The Ashen March".to_string(),
            description: "A three-chapter descent into the dark".to_string(),
            difficulty_tiers: vec!["normal".to_string(), "brutal".to_string()],
        },
    );
    campaign.entry_chapter_id = "ch1".to_string();
    campaign.upsert_chapter(chapter("ch1", 0, "map_a", &[]));
    campaign.upsert_chapter(chapter("ch2", 1, "map_b", &["ch1"]));
    campaign.upsert_chapter(chapter("ch3", 2, "map_c", &["ch2"]));
    campaign
        .shared_object_data
        .push(RuntimeCampaignSharedDatum {
            key: SHARED_HERO_KEY.to_string(),
            category: RuntimeSharedObjectCategory::Hero,
            definition: json!({ "base_level": 1, "strength": 14, "role": "hero" }),
        });
    campaign.hero_carryover_slots.push(RuntimeCampaignHeroSlot {
        slot_id: "slot_primary".to_string(),
        hero_key: SHARED_HERO_KEY.to_string(),
        carries_level: true,
        carries_items: true,
        active_through_chapter: None,
    });
    campaign
}

fn chapter(id: &str, order: u32, map_ref: &str, unlock: &[&str]) -> CampaignChapter {
    CampaignChapter {
        chapter_id: id.to_string(),
        order,
        title: format!("Chapter {id}"),
        map_ref: map_ref.to_string(),
        map_document_path: format!("calyx://world/{id}"),
        unlock_after: unlock.iter().map(|value| value.to_string()).collect(),
        branch_on_victory: Vec::new(),
    }
}

fn build_chapter_worlds() -> Result<BTreeMap<String, EditorDocument>, EngineError> {
    let mut maps = BTreeMap::new();
    maps.insert("map_a".to_string(), world_document("map_a", false)?);
    // Chapter 2's map authors an object colliding with the shared hero key.
    maps.insert("map_b".to_string(), world_document("map_b", true)?);
    maps.insert("map_c".to_string(), world_document("map_c", false)?);
    Ok(maps)
}

fn world_document(
    map_ref: &str,
    override_shared_hero: bool,
) -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new(
        format!("calyx://world/issue853-{map_ref}"),
        format!("issue853-{map_ref}-rev-1"),
    )?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_playtest_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    if override_shared_hero {
        document.upsert_object(
            EditorObject::new(
                SHARED_HERO_KEY,
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(-2.0, 0.0, 4.0)),
            )
            .with_property("override", "map_local"),
        )?;
    }
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
        .ok_or_else(|| {
            invalid(
                "world.asset",
                "missing real vault model record (need rematerialized Stan.gltf or other vault mesh)",
            )
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    if override_shared_hero {
        assign_vault_model_to_unit(&mut document, SHARED_HERO_KEY, &record)?;
    }
    Ok(document)
}

fn campaign_request(share_id: &str) -> QuickShareBuildRequest {
    QuickShareBuildRequest::new(
        share_id,
        "human.qa.ashen",
        QuickShareEndpoint::loopback(share_id, 11870),
        MAX_BYTES,
    )
}

fn invalid(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("issue853.fsv.{field}"),
        reason: reason.into(),
    }
}
