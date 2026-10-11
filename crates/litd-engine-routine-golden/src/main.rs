//! Production-profile deterministic engine golden path.
//!
//! This is an executable rather than a Cargo test target so the routine gate can execute the
//! real public engine path without building a second cfg(test) artifact. Any mismatch exits
//! non-zero with the failing source-of-truth values.

use std::{collections::BTreeMap, error::Error, fs, path::PathBuf};

use litd_engine::game_package::{
    LITD_CAMPAIGN_MANIFEST_SCHEMA, LITD_LOCK_IN_POLICY, LITD_LOWERED_DOCUMENT_SCHEMA,
    LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimeCampaignBranch, RuntimeCampaignChapter,
    RuntimeCampaignHeroSlot, RuntimeCampaignManifest, RuntimeCampaignSharedDatum,
    RuntimePackageAssetRef, RuntimePackageEndpoint, RuntimePackageHostMode,
    RuntimeSharedObjectCategory, compute_runtime_manifest_fingerprint, stable_package_fingerprint,
    verify_runtime_package_admission,
};
use litd_engine::{
    editor_min_spec::{AdapterClass, EditorDegradeSettings},
    game_runtime::LITD_TRIGGER_GRAPH_SCHEMA,
    game_runtime::{
        AuthoredUnit, CampaignFlow, CampaignKvStore, EconomyRulesetAuthor, GameRuntime,
        HeroProgress, LoweredResourceKind, LoweredTrigger, LoweredTriggerGraph, LoweredUpkeepTier,
        MatchBlueprint, MatchRuntimeSession, PlayerController, PlayerSlot, RuntimeDatasetsManifest,
        SessionOrder, TimeOfDayConfig, TimeOfDayState, TriggerAction, TriggerEvent, TriggerValue,
        TriggerVarInit, VictoryCondition, lower_runtime_constants, lower_trigger_graph,
    },
    sim::SimConfig,
};
use serde_json::json;

mod economy_golden;

fn run_issue805_golden() -> Result<String, Box<dyn Error>> {
    let share_id = "issue805-golden";
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 11650,
        path: format!("/share/{share_id}"),
    };
    let document_fingerprint = "issue805-document";
    let play_build_fingerprint = "issue805-play-build";
    let assets = vec![RuntimePackageAssetRef {
        product_id: "litd.asset.issue805.golden".to_string(),
        source_relative_path: "assets/issue805/golden.bin".to_string(),
        source_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string(),
        source_bytes: 1,
    }];
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "litd-engine-routine-golden",
        LITD_RUNTIME_ID,
        &endpoint,
        document_fingerprint,
        play_build_fingerprint,
        &assets,
    )?;
    let package = json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": "litd.quick_share.issue805_golden",
        "created_by": "litd-engine-routine-golden",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": "litd-share://127.0.0.1:11650/share/issue805-golden",
        "document": {"schema": "litd.lowered_document.v1", "document_id": "issue805"},
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": 1,
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint,
    });
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("fsv")
        .join("issue805-package-proof");
    fs::create_dir_all(&root)?;
    let path = root.join("guest-package.litdgame.json");
    let bytes = serde_json::to_vec_pretty(&package)?;
    fs::write(&path, &bytes)?;
    let expected_sha256 = litd_hash::sha256_hex(&bytes);
    let report = verify_runtime_package_admission(
        &path,
        "litd.quick_share.issue805_golden",
        &expected_sha256,
        bytes.len(),
        "litd-engine-routine-golden",
    )?;
    if report.package_id != "litd.quick_share.issue805_golden"
        || !report.runtime_lock_verified
        || !report.playable_on_litd_runtime
    {
        return Err(format!("issue805 package proof readback mismatch: {report:?}").into());
    }
    if fs::read(&path)? != bytes {
        return Err("issue805 package source-of-truth bytes changed during admission".into());
    }
    fs::remove_file(&path)?;
    Ok(expected_sha256)
}

fn run_trigger_graph_golden() -> Result<String, Box<dyn Error>> {
    let mut graph = LoweredTriggerGraph {
        schema: LITD_TRIGGER_GRAPH_SCHEMA.to_string(),
        variables: vec![TriggerVarInit {
            name: "campaign_started".to_string(),
            value: TriggerValue::Flag(false),
        }],
        triggers: vec![LoweredTrigger {
            trigger_id: "routine_golden_campaign_start".to_string(),
            enabled: true,
            initially_on: true,
            events: vec![TriggerEvent::MapInitialization],
            conditions: Vec::new(),
            actions: vec![TriggerAction::SetVariable {
                name: "campaign_started".to_string(),
                value: TriggerValue::Flag(true),
            }],
        }],
        graph_fingerprint: String::new(),
    };
    graph.graph_fingerprint = graph.compute_fingerprint()?;
    graph.validate()?;
    if graph.variable("campaign_started").map(|entry| &entry.value)
        != Some(&TriggerValue::Flag(false))
    {
        return Err("trigger graph golden variable readback mismatch".into());
    }
    Ok(graph.graph_fingerprint)
}

fn run_trigger_runtime_golden() -> Result<u64, Box<dyn Error>> {
    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .lower()?;
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
    let mut blueprint = MatchBlueprint::new("routine_trigger", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    blueprint.author_unit(AuthoredUnit {
        unit: 1,
        owner: 0,
        is_structure: true,
        capabilities: Vec::new(),
    });
    blueprint.author_unit(AuthoredUnit {
        unit: 2,
        owner: 1,
        is_structure: true,
        capabilities: Vec::new(),
    });
    let graph = lower_trigger_graph(
        vec![TriggerVarInit {
            name: "campaign_started".to_string(),
            value: TriggerValue::Flag(false),
        }],
        vec![LoweredTrigger {
            trigger_id: "routine_golden_campaign_start".to_string(),
            enabled: true,
            initially_on: true,
            events: vec![TriggerEvent::MapInitialization],
            conditions: Vec::new(),
            actions: vec![TriggerAction::SetVariable {
                name: "campaign_started".to_string(),
                value: TriggerValue::Flag(true),
            }],
        }],
    )?;
    let constants = lower_runtime_constants(4, 200, 6_000, 500, 1_000, 16, 16, 0x805)?;
    let manifest = litd_engine::game_runtime::RuntimeDatasetsManifest::assemble_with_triggers(
        economy,
        blueprint.lower()?,
        constants,
        Vec::new(),
        Some(graph),
    )?;
    let mut session = MatchRuntimeSession::from_manifest(&manifest)?;
    if session.triggers().variable("campaign_started") != Some(&TriggerValue::Flag(false)) {
        return Err("trigger runtime initial variable mismatch".into());
    }
    session.apply(SessionOrder::Tick)?;
    if session.triggers().variable("campaign_started") != Some(&TriggerValue::Flag(true))
        || session
            .triggers()
            .fire_count("routine_golden_campaign_start")
            != Some(1)
    {
        return Err("trigger runtime map-init readback mismatch".into());
    }
    Ok(session.combined_state_hash())
}

fn campaign_dataset(seed: u64) -> Result<RuntimeDatasetsManifest, Box<dyn Error>> {
    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .lower()?;
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
    let mut blueprint = MatchBlueprint::new("routine_campaign_map", slots, 2);
    blueprint.author_condition(VictoryCondition::AllEnemyStructuresDestroyed);
    for (unit, owner) in [(1, 0), (2, 1)] {
        blueprint.author_unit(AuthoredUnit {
            unit,
            owner,
            is_structure: true,
            capabilities: Vec::new(),
        });
    }
    let constants = lower_runtime_constants(4, 200, 6_000, 500, 1_000, 16, 16, seed)?;
    Ok(RuntimeDatasetsManifest::assemble(
        economy,
        blueprint.lower()?,
        constants,
    )?)
}

fn run_campaign_golden() -> Result<u64, Box<dyn Error>> {
    let mut maps = BTreeMap::new();
    for (map_id, name) in [("map_a", "Ashford"), ("map_b", "Veilspire")] {
        maps.insert(
            map_id.to_string(),
            json!({
                "schema": LITD_LOWERED_DOCUMENT_SCHEMA,
                "calyx_document_id": format!("calyx://world/{name}"),
                "revision_id": "routine-golden",
            }),
        );
    }
    let mut chapter_runtime_datasets = BTreeMap::new();
    chapter_runtime_datasets.insert("map_a".to_string(), campaign_dataset(0x805_0001)?);
    chapter_runtime_datasets.insert("map_b".to_string(), campaign_dataset(0x805_0002)?);
    let chapter =
        |id: &str, order: u32, map_ref: &str| -> Result<RuntimeCampaignChapter, Box<dyn Error>> {
            Ok(RuntimeCampaignChapter {
                chapter_id: id.to_string(),
                order,
                title: id.to_string(),
                map_ref: map_ref.to_string(),
                map_document_fingerprint: stable_package_fingerprint(&maps[map_ref])?,
                unlock_after: (order > 0)
                    .then(|| vec!["ch1".to_string()])
                    .unwrap_or_default(),
                branch_on_victory: (order == 0)
                    .then(|| {
                        vec![RuntimeCampaignBranch {
                            victory_condition: "cleared".to_string(),
                            next_chapter_id: "ch2".to_string(),
                        }]
                    })
                    .unwrap_or_default(),
                shared_data_resolution: Vec::new(),
            })
        };
    let mut manifest = RuntimeCampaignManifest {
        schema: LITD_CAMPAIGN_MANIFEST_SCHEMA.to_string(),
        campaign_id: "routine-golden-campaign".to_string(),
        title: "Routine Golden Campaign".to_string(),
        description: "Two authored chapters".to_string(),
        difficulty_tiers: vec!["normal".to_string()],
        entry_chapter_id: "ch1".to_string(),
        chapters: vec![chapter("ch1", 0, "map_a")?, chapter("ch2", 1, "map_b")?],
        shared_object_data: vec![RuntimeCampaignSharedDatum {
            key: "hero_vanguard".to_string(),
            category: RuntimeSharedObjectCategory::Hero,
            definition: json!({"base_level": 1}),
        }],
        hero_carryover_slots: vec![RuntimeCampaignHeroSlot {
            slot_id: "slot_vanguard".to_string(),
            hero_key: "hero_vanguard".to_string(),
            carries_level: true,
            carries_items: true,
            active_through_chapter: None,
        }],
        maps,
        chapter_runtime_datasets,
        manifest_fingerprint: String::new(),
    };
    manifest.manifest_fingerprint = manifest.compute_fingerprint()?;

    let mut flow = CampaignFlow::boot(manifest, 0)?;
    if flow.current_chapter_id() != "ch1" || !flow.current_chapter_boots_match() {
        return Err("campaign boot readback mismatch".into());
    }
    let first_hash = flow.instantiate_current_session()?.combined_state_hash();
    let progress = HeroProgress {
        hero_key: "hero_vanguard".to_string(),
        level: 4,
        experience: 900,
        hp_milli: 8_000,
        item_keys: vec!["iron_sword".to_string()],
    };
    let advance =
        flow.complete_chapter(Some("cleared"), &[("slot_vanguard".to_string(), progress)])?;
    if advance.next_chapter_id.as_deref() != Some("ch2")
        || flow.current_chapter_id() != "ch2"
        || flow.rehydrate_hero("slot_vanguard")?.level != 4
    {
        return Err("campaign chapter-advance readback mismatch".into());
    }
    let second_hash = flow.instantiate_current_session()?.combined_state_hash();
    let advance = flow.complete_chapter(
        None,
        &[(
            "slot_vanguard".to_string(),
            HeroProgress {
                hero_key: "hero_vanguard".to_string(),
                level: 5,
                experience: 1_250,
                hp_milli: 9_000,
                item_keys: vec!["iron_sword".to_string()],
            },
        )],
    )?;
    if !advance.campaign_complete || flow.completed_chapters().len() != 2 {
        return Err("campaign completion readback mismatch".into());
    }
    Ok(first_hash ^ second_hash ^ flow.kv().state_hash())
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = SimConfig::new(20)?;
    let time = TimeOfDayState::new(
        TimeOfDayConfig {
            day_length_ticks: 100,
            base_sight_radius_mm: 10_000,
            night_sight_penalty_permille: 250,
        },
        0,
        0,
    )?;
    let store = CampaignKvStore::new("routine-golden", 0)?;
    let mut first = GameRuntime::<8, 16>::new(config, 42, time.clone(), store.clone())?;
    let mut second = GameRuntime::<8, 16>::new(config, 42, time, store)?;
    let first_result = first.run_ticks(3)?;
    let second_result = second.run_ticks(3)?;
    if first_result != second_result {
        return Err(format!(
            "deterministic replay mismatch: first={first_result:?} second={second_result:?}"
        )
        .into());
    }
    let first_hash = first.combined_state_hash();
    let second_hash = second.combined_state_hash();
    if first_hash != second_hash {
        return Err(format!(
            "deterministic state hash mismatch: first={first_hash:#x} second={second_hash:#x}"
        )
        .into());
    }

    let settings = EditorDegradeSettings::defaults_for(AdapterClass::Unknown);
    settings.validate()?;
    if settings.shadow_atlas_size != 1024 || settings.viewport_resolution_scale_percent != 75 {
        return Err(format!(
            "min-spec defaults changed: shadow_atlas_size={} viewport_scale={}",
            settings.shadow_atlas_size, settings.viewport_resolution_scale_percent
        )
        .into());
    }

    let issue805_sha256 = run_issue805_golden()?;
    let trigger_graph_fingerprint = run_trigger_graph_golden()?;
    let trigger_runtime_hash = run_trigger_runtime_golden()?;
    let campaign_golden_hash = run_campaign_golden()?;
    let economy_golden_hash = economy_golden::run()?;

    println!(
        "ENGINE_GOLDEN_PASS replay_ticks=3 state_hash={first_hash:#x} issue805_package_sha256={issue805_sha256} trigger_graph_fingerprint={trigger_graph_fingerprint} trigger_runtime_hash={trigger_runtime_hash:#x} campaign_golden_hash={campaign_golden_hash:#x} economy_golden_hash={economy_golden_hash:#x} shadow_atlas_size={} viewport_scale={} ",
        settings.shadow_atlas_size, settings.viewport_resolution_scale_percent
    );
    Ok(())
}
