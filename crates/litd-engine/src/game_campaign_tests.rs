use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::LITD_LOWERED_DOCUMENT_SCHEMA;
use super::*;

fn lowered_map(id: &str) -> Value {
    json!({
        "schema": LITD_LOWERED_DOCUMENT_SCHEMA,
        "calyx_document_id": format!("calyx://world/{id}"),
        "revision_id": "rev-1",
        "objects": { "hero": { "id": "hero", "kind": "Unit" } },
    })
}

fn manifest_with(maps: BTreeMap<String, Value>) -> RuntimeCampaignManifest {
    let mut manifest = RuntimeCampaignManifest {
        schema: LITD_CAMPAIGN_MANIFEST_SCHEMA.to_string(),
        campaign_id: "camp.test".to_string(),
        title: "Test Campaign".to_string(),
        description: "three chapters".to_string(),
        difficulty_tiers: vec!["normal".to_string(), "hard".to_string()],
        entry_chapter_id: "ch1".to_string(),
        chapters: vec![
            chapter("ch1", 0, "map_a", &maps, &[], &[]),
            chapter("ch2", 1, "map_b", &maps, &["ch1"], &[("cleared", "ch3")]),
            chapter("ch3", 2, "map_c", &maps, &["ch2"], &[]),
        ],
        shared_object_data: vec![RuntimeCampaignSharedDatum {
            key: "shared_hero".to_string(),
            category: RuntimeSharedObjectCategory::Hero,
            definition: json!({ "level": 1 }),
        }],
        hero_carryover_slots: vec![RuntimeCampaignHeroSlot {
            slot_id: "slot1".to_string(),
            hero_key: "shared_hero".to_string(),
            carries_level: true,
            carries_items: true,
            active_through_chapter: None,
        }],
        maps,
        chapter_runtime_datasets: BTreeMap::new(),
        manifest_fingerprint: String::new(),
    };
    manifest.manifest_fingerprint = manifest.compute_fingerprint().unwrap();
    manifest
}

fn chapter(
    id: &str,
    order: u32,
    map_ref: &str,
    maps: &BTreeMap<String, Value>,
    unlock: &[&str],
    branches: &[(&str, &str)],
) -> RuntimeCampaignChapter {
    RuntimeCampaignChapter {
        chapter_id: id.to_string(),
        order,
        title: format!("Chapter {id}"),
        map_ref: map_ref.to_string(),
        map_document_fingerprint: stable_package_fingerprint(&maps[map_ref]).unwrap(),
        unlock_after: unlock.iter().map(|value| value.to_string()).collect(),
        branch_on_victory: branches
            .iter()
            .map(|(condition, next)| RuntimeCampaignBranch {
                victory_condition: condition.to_string(),
                next_chapter_id: next.to_string(),
            })
            .collect(),
        shared_data_resolution: Vec::new(),
    }
}

fn three_map_manifest() -> (RuntimeCampaignManifest, Value) {
    let mut maps = BTreeMap::new();
    maps.insert("map_a".to_string(), lowered_map("a"));
    maps.insert("map_b".to_string(), lowered_map("b"));
    maps.insert("map_c".to_string(), lowered_map("c"));
    let entry_document = maps["map_a"].clone();
    (manifest_with(maps), entry_document)
}

#[test]
fn valid_three_chapter_campaign_accepts() {
    let (manifest, document) = three_map_manifest();
    let fingerprint = stable_package_fingerprint(&document).unwrap();
    validate_campaign_manifest(&manifest, &document, &fingerprint).expect("valid campaign");
}

#[test]
fn missing_chapter_map_fails_loud() {
    let (mut manifest, document) = three_map_manifest();
    let fingerprint = stable_package_fingerprint(&document).unwrap();
    manifest.maps.remove("map_c");
    manifest.manifest_fingerprint = manifest.compute_fingerprint().unwrap();
    let error = validate_campaign_manifest(&manifest, &document, &fingerprint)
        .unwrap_err()
        .to_string();
    assert!(error.contains("map_c"), "error was {error}");
}

#[test]
fn corrupted_manifest_fingerprint_fails_loud() {
    let (mut manifest, document) = three_map_manifest();
    let fingerprint = stable_package_fingerprint(&document).unwrap();
    // Tamper the title without recomputing the fingerprint: the negative control.
    manifest.title = "Hijacked Campaign".to_string();
    let error = validate_campaign_manifest(&manifest, &document, &fingerprint)
        .unwrap_err()
        .to_string();
    assert!(error.contains("fingerprint"), "error was {error}");
}

#[test]
fn dangling_branch_target_fails_loud() {
    let (mut manifest, document) = three_map_manifest();
    let fingerprint = stable_package_fingerprint(&document).unwrap();
    manifest.chapters[1].branch_on_victory[0].next_chapter_id = "ch_missing".to_string();
    manifest.manifest_fingerprint = manifest.compute_fingerprint().unwrap();
    let error = validate_campaign_manifest(&manifest, &document, &fingerprint)
        .unwrap_err()
        .to_string();
    assert!(error.contains("ch_missing"), "error was {error}");
}

#[test]
fn hero_slot_without_shared_hero_fails_loud() {
    let (mut manifest, document) = three_map_manifest();
    let fingerprint = stable_package_fingerprint(&document).unwrap();
    manifest.hero_carryover_slots[0].hero_key = "not_a_hero".to_string();
    manifest.manifest_fingerprint = manifest.compute_fingerprint().unwrap();
    let error = validate_campaign_manifest(&manifest, &document, &fingerprint)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not_a_hero"), "error was {error}");
}

#[test]
fn reordering_chapters_changes_fingerprint_deterministically() {
    let (manifest, _document) = three_map_manifest();
    let before = manifest.manifest_fingerprint.clone();
    let mut reordered = manifest.clone();
    reordered.chapters.swap(0, 2);
    reordered.chapters[0].order = 0;
    reordered.chapters[2].order = 2;
    let after = reordered.compute_fingerprint().unwrap();
    assert_ne!(before, after, "reordering must change the fingerprint");
    // Determinism: recomputing yields the identical value both runs.
    assert_eq!(after, reordered.compute_fingerprint().unwrap());
}

// --- #1852: per-chapter embedded runtime datasets ---

#[test]
fn campaign_with_per_chapter_runtime_datasets_validates() {
    let fixture = crate::game_runtime::campaign_flow_fixture::two_chapter_campaign();
    validate_campaign_manifest(
        &fixture.manifest,
        &fixture.entry_document,
        &fixture.entry_document_fingerprint,
    )
    .expect("2-chapter datasets campaign validates");
    // Every combat chapter's map_ref resolves to embedded datasets.
    assert!(fixture.manifest.chapter_runtime_datasets("map_a").is_some());
    assert!(fixture.manifest.chapter_runtime_datasets("map_b").is_some());
}

#[test]
fn runtime_datasets_keyed_by_unknown_map_ref_fails_loud() {
    let mut fixture = crate::game_runtime::campaign_flow_fixture::two_chapter_campaign();
    let stolen = fixture
        .manifest
        .chapter_runtime_datasets("map_a")
        .unwrap()
        .clone();
    fixture
        .manifest
        .chapter_runtime_datasets
        .insert("map_ghost".to_string(), stolen);
    fixture.manifest.manifest_fingerprint = fixture.manifest.compute_fingerprint().unwrap();
    let error = validate_campaign_manifest(
        &fixture.manifest,
        &fixture.entry_document,
        &fixture.entry_document_fingerprint,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("map_ghost"), "error was {error}");
}

#[test]
fn tampered_chapter_runtime_datasets_fails_loud() {
    let mut fixture = crate::game_runtime::campaign_flow_fixture::two_chapter_campaign();
    // Mutate a datasets sub-field WITHOUT re-deriving its own fingerprint, then re-stamp the
    // manifest fingerprint so the tamper must be caught by the datasets re-validation.
    let datasets = fixture
        .manifest
        .chapter_runtime_datasets
        .get_mut("map_a")
        .unwrap();
    datasets.constants.match_seed ^= 0xFFFF;
    fixture.manifest.manifest_fingerprint = fixture.manifest.compute_fingerprint().unwrap();
    let error = validate_campaign_manifest(
        &fixture.manifest,
        &fixture.entry_document,
        &fixture.entry_document_fingerprint,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.to_lowercase().contains("fingerprint"),
        "error was {error}"
    );
}
