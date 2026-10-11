//! #1852: a REAL multi-chapter lowered campaign package, built through the genuine engine
//! lowering primitives (`EconomyRulesetAuthor::lower` / `MatchBlueprint::lower` /
//! `lower_runtime_constants` / `RuntimeDatasetsManifest::assemble`) -- the same `.lower()`
//! calls the editor object-data valve drives, exercised programmatically because a
//! library fixture cannot author `EditorObject`s.
//!
//! Shared by the campaign-flow unit tests (`cfg(test)`) and the #1852 FSV example
//! (`feature = "fsv"`), so the two-chapter campaign is built ONE way. Not compiled into a
//! routine release build (no dead code): gated `cfg(any(test, feature = "fsv"))`.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::game_package::{
    LITD_CAMPAIGN_MANIFEST_SCHEMA, LITD_LOWERED_DOCUMENT_SCHEMA, RuntimeCampaignBranch,
    RuntimeCampaignChapter, RuntimeCampaignHeroSlot, RuntimeCampaignManifest,
    RuntimeCampaignSharedDatum, RuntimeSharedObjectCategory, stable_package_fingerprint,
};

use super::blueprint::{AuthoredUnit, MatchBlueprint};
use super::datasets::{RuntimeDatasetsManifest, lower_runtime_constants};
use super::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredStructureType,
    LoweredUnitEconomy, LoweredUpkeepTier,
};
use super::players::{PlayerController, PlayerSlot};
use super::victory::VictoryCondition;

/// The victory condition string chapter 1 routes on (matches its `branch_on_victory` edge).
pub const VICTORY_CLEARED: &str = "cleared";
/// The hero carry-over slot id the fixture campaign declares.
pub const HERO_SLOT_ID: &str = "slot_vanguard";
/// The shared Hero datum key that slot references.
pub const HERO_KEY: &str = "hero_vanguard";
/// #1853: Miren's permanent-death companion slot (active through m3).
pub const MIREN_SLOT_ID: &str = "slot_miren";
/// #1853: Miren's hero key.
pub const MIREN_HERO_KEY: &str = "hero_miren";
/// Player 1's (the AI's) structure unit handle -- destroying it wins the melee for player 0.
pub const ENEMY_STRUCTURE_UNIT: u32 = 2;
/// Player 0's (the human's) structure unit handle.
pub const OWN_STRUCTURE_UNIT: u32 = 1;

const TICK_RATE: u32 = 4;
const GRID: u32 = 16;

/// A fully-built two-chapter campaign package payload.
pub struct CampaignFixture {
    pub manifest: RuntimeCampaignManifest,
    /// The entry chapter's lowered map document (the package boot `document`).
    pub entry_document: Value,
    /// Its stable fingerprint (the package `document_fingerprint`).
    pub entry_document_fingerprint: String,
}

/// Build a real 2-chapter campaign: `ch1` (map_a) -> `ch2` (map_b), reachable both by an
/// explicit `cleared` victory branch and by `unlock_after`. Each chapter embeds its own
/// lowered runtime datasets so both chapters BOOT as real melee matches. One hero carry-over
/// slot carries level + items across the chapter boundary.
pub fn two_chapter_campaign() -> CampaignFixture {
    let mut maps: BTreeMap<String, Value> = BTreeMap::new();
    maps.insert("map_a".to_string(), lowered_map("ashford"));
    maps.insert("map_b".to_string(), lowered_map("veilspire"));

    let mut chapter_runtime_datasets: BTreeMap<String, RuntimeDatasetsManifest> = BTreeMap::new();
    chapter_runtime_datasets.insert("map_a".to_string(), sample_datasets(0x1852_0001));
    chapter_runtime_datasets.insert("map_b".to_string(), sample_datasets(0x1852_0002));

    let entry_document = maps["map_a"].clone();
    let entry_document_fingerprint = stable_package_fingerprint(&entry_document).expect("fp");

    let chapters = vec![
        RuntimeCampaignChapter {
            chapter_id: "ch1".to_string(),
            order: 0,
            title: "Ashford Crossing".to_string(),
            map_ref: "map_a".to_string(),
            map_document_fingerprint: stable_package_fingerprint(&maps["map_a"]).expect("fp"),
            unlock_after: Vec::new(),
            branch_on_victory: vec![RuntimeCampaignBranch {
                victory_condition: VICTORY_CLEARED.to_string(),
                next_chapter_id: "ch2".to_string(),
            }],
            shared_data_resolution: Vec::new(),
        },
        RuntimeCampaignChapter {
            chapter_id: "ch2".to_string(),
            order: 1,
            title: "Veilspire Reach".to_string(),
            map_ref: "map_b".to_string(),
            map_document_fingerprint: stable_package_fingerprint(&maps["map_b"]).expect("fp"),
            unlock_after: vec!["ch1".to_string()],
            branch_on_victory: Vec::new(),
            shared_data_resolution: Vec::new(),
        },
    ];

    let mut manifest = RuntimeCampaignManifest {
        schema: LITD_CAMPAIGN_MANIFEST_SCHEMA.to_string(),
        campaign_id: "camp.held_to_the_last".to_string(),
        title: "Held to the Last".to_string(),
        description: "M1 proof campaign".to_string(),
        difficulty_tiers: vec!["normal".to_string(), "hard".to_string()],
        entry_chapter_id: "ch1".to_string(),
        chapters,
        shared_object_data: vec![RuntimeCampaignSharedDatum {
            key: HERO_KEY.to_string(),
            category: RuntimeSharedObjectCategory::Hero,
            definition: json!({ "base_level": 1 }),
        }],
        hero_carryover_slots: vec![RuntimeCampaignHeroSlot {
            slot_id: HERO_SLOT_ID.to_string(),
            hero_key: HERO_KEY.to_string(),
            carries_level: true,
            carries_items: true,
            active_through_chapter: None,
        }],
        maps,
        chapter_runtime_datasets,
        manifest_fingerprint: String::new(),
    };
    manifest.manifest_fingerprint = manifest.compute_fingerprint().expect("fingerprint");

    CampaignFixture {
        manifest,
        entry_document,
        entry_document_fingerprint,
    }
}

/// #1853 permanent-death fixture: m2 -> m3 -> m5.
/// - `slot_vanguard` lives the whole campaign.
/// - `slot_miren` is authored `active_through_chapter = m3` and is script-tombstoned mid-m3.
///
/// After m3, chapter seeding must omit Miren; the save file holds `hero_roster` FALLEN.
pub fn three_chapter_permanent_death_campaign() -> CampaignFixture {
    let mut maps: BTreeMap<String, Value> = BTreeMap::new();
    maps.insert("map_m2".to_string(), lowered_map("m2_ashford"));
    maps.insert("map_m3".to_string(), lowered_map("m3_hold"));
    maps.insert("map_m5".to_string(), lowered_map("m5_last"));

    let mut chapter_runtime_datasets: BTreeMap<String, RuntimeDatasetsManifest> = BTreeMap::new();
    chapter_runtime_datasets.insert("map_m2".to_string(), sample_datasets(0x1853_0002));
    chapter_runtime_datasets.insert("map_m3".to_string(), sample_datasets(0x1853_0003));
    chapter_runtime_datasets.insert("map_m5".to_string(), sample_datasets(0x1853_0005));

    let entry_document = maps["map_m2"].clone();
    let entry_document_fingerprint = stable_package_fingerprint(&entry_document).expect("fp");

    let chapters = vec![
        RuntimeCampaignChapter {
            chapter_id: "m2".to_string(),
            order: 0,
            title: "Ashford Crossroads".to_string(),
            map_ref: "map_m2".to_string(),
            map_document_fingerprint: stable_package_fingerprint(&maps["map_m2"]).expect("fp"),
            unlock_after: Vec::new(),
            branch_on_victory: vec![RuntimeCampaignBranch {
                victory_condition: VICTORY_CLEARED.to_string(),
                next_chapter_id: "m3".to_string(),
            }],
            shared_data_resolution: Vec::new(),
        },
        RuntimeCampaignChapter {
            chapter_id: "m3".to_string(),
            order: 1,
            title: "The Hold".to_string(),
            map_ref: "map_m3".to_string(),
            map_document_fingerprint: stable_package_fingerprint(&maps["map_m3"]).expect("fp"),
            unlock_after: vec!["m2".to_string()],
            branch_on_victory: vec![RuntimeCampaignBranch {
                victory_condition: VICTORY_CLEARED.to_string(),
                next_chapter_id: "m5".to_string(),
            }],
            shared_data_resolution: Vec::new(),
        },
        RuntimeCampaignChapter {
            chapter_id: "m5".to_string(),
            order: 2,
            title: "Held to the Last".to_string(),
            map_ref: "map_m5".to_string(),
            map_document_fingerprint: stable_package_fingerprint(&maps["map_m5"]).expect("fp"),
            unlock_after: vec!["m3".to_string()],
            branch_on_victory: Vec::new(),
            shared_data_resolution: Vec::new(),
        },
    ];

    let mut manifest = RuntimeCampaignManifest {
        schema: LITD_CAMPAIGN_MANIFEST_SCHEMA.to_string(),
        campaign_id: "camp.held_to_the_last.death".to_string(),
        title: "Held to the Last".to_string(),
        description: "M1 permanent-death spine (#1853)".to_string(),
        difficulty_tiers: vec!["normal".to_string()],
        entry_chapter_id: "m2".to_string(),
        chapters,
        shared_object_data: vec![
            RuntimeCampaignSharedDatum {
                key: HERO_KEY.to_string(),
                category: RuntimeSharedObjectCategory::Hero,
                definition: json!({ "base_level": 1 }),
            },
            RuntimeCampaignSharedDatum {
                key: MIREN_HERO_KEY.to_string(),
                category: RuntimeSharedObjectCategory::Hero,
                definition: json!({ "base_level": 1 }),
            },
        ],
        hero_carryover_slots: vec![
            RuntimeCampaignHeroSlot {
                slot_id: HERO_SLOT_ID.to_string(),
                hero_key: HERO_KEY.to_string(),
                carries_level: true,
                carries_items: true,
                active_through_chapter: None,
            },
            RuntimeCampaignHeroSlot {
                slot_id: MIREN_SLOT_ID.to_string(),
                hero_key: MIREN_HERO_KEY.to_string(),
                carries_level: true,
                carries_items: true,
                active_through_chapter: Some("m3".to_string()),
            },
        ],
        maps,
        chapter_runtime_datasets,
        manifest_fingerprint: String::new(),
    };
    manifest.manifest_fingerprint = manifest.compute_fingerprint().expect("fingerprint");

    CampaignFixture {
        manifest,
        entry_document,
        entry_document_fingerprint,
    }
}

fn lowered_map(id: &str) -> Value {
    json!({
        "schema": LITD_LOWERED_DOCUMENT_SCHEMA,
        "calyx_document_id": format!("calyx://world/{id}"),
        "revision_id": "rev-1",
        "objects": { HERO_KEY: { "id": HERO_KEY, "kind": "Unit" } },
    })
}

/// A minimal-but-real 2-player melee dataset: one structure + one field unit per player, a
/// gold node, `AllEnemyStructuresDestroyed` victory. Distinct `seed` gives each chapter a
/// distinct datasets fingerprint so per-chapter binding is provable on readback.
pub fn sample_datasets(seed: u64) -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 10_000,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: 10_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "no_upkeep".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: "soldier".to_string(),
            gold_cost: 100,
            lumber_cost: 10,
            food_cost: 2,
            build_time_secs: 1,
            food_provided: 0,
        })
        .node(LoweredResourceNode {
            node_id: "gold_mine".to_string(),
            resource_key: "gold".to_string(),
            cell_x: 5,
            cell_z: 3,
            amount_total: 5_000,
            yield_per_trip: 10,
        })
        .structure(LoweredStructureType {
            struct_id: "town_hall".to_string(),
            gold_cost: 200,
            lumber_cost: 0,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 20,
            trains: vec!["soldier".to_string()],
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
    let mut blueprint = MatchBlueprint::new("mini_melee", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    blueprint.author_unit(AuthoredUnit {
        unit: OWN_STRUCTURE_UNIT,
        owner: 0,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: ENEMY_STRUCTURE_UNIT,
        owner: 1,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 10,
        owner: 0,
        is_structure: false,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 11,
        owner: 1,
        is_structure: false,
        capabilities: Vec::new(),
    });
    let lowered_match = blueprint.lower().expect("lower match");

    let constants = lower_runtime_constants(TICK_RATE, 200, 6_000, 500, 1_000, GRID, GRID, seed)
        .expect("lower constants");

    RuntimeDatasetsManifest::assemble(economy, lowered_match, constants).expect("assemble")
}
