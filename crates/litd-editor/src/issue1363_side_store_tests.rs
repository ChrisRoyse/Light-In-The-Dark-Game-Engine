use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    math::{Aabb, Vec3},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset,
        VfxEmitter, VfxGradient, VfxGradientStop, VfxInitializeModule, VfxOutputModule, VfxRange,
        VfxShape, VfxSpawnModule, VfxUpdateModule, VfxVelocity, read_effect_from_content_store,
        write_effect_to_content_store,
    },
};
use serde_json::{Value, json};

use crate::{
    scenario::{
        DayNightCycle, MapDescription, MapOptions, MapRuleset, ScenarioWorldInfo,
        StartLocationPolicy, build_lobby_summary, load_scenario_world_info,
        save_scenario_world_info,
    },
    scenario_availability::{
        PlayerAvailabilityOverrides, PlayerPlacementPriorities, PlayerUpgradeOverride,
        ScenarioAvailabilityRecord, TechDependencyRule, UpgradeAvailability,
        build_runtime_availability, load_scenario_availability_record,
        save_scenario_availability_record,
    },
};

#[test]
fn issue1363_side_store_eradication_writes_only_calyx_aster_fsv() {
    let root = unique_fsv_root("happy");
    let scenario_path = root.join("scenario-world-info-aster");
    let availability_path = root.join("scenario-availability-aster");
    let vfx_root = root.join("vfx-content-aster");

    let before = json!({
        "scenario": path_state(&scenario_path),
        "availability": path_state(&availability_path),
        "vfx": path_state(&vfx_root),
    });

    let world_info = scenario_world_info();
    save_scenario_world_info(&scenario_path, &world_info).unwrap();
    let loaded_world_info = load_scenario_world_info(&scenario_path).unwrap();
    let lobby = build_lobby_summary(&loaded_world_info).unwrap();

    let availability = availability_record();
    save_scenario_availability_record(&availability_path, &availability).unwrap();
    let loaded_availability = load_scenario_availability_record(&availability_path).unwrap();
    let runtime_availability = build_runtime_availability(&loaded_availability).unwrap();

    let effect = vfx_effect();
    let first_vfx = write_effect_to_content_store(&vfx_root, &effect).unwrap();
    let second_vfx = write_effect_to_content_store(&vfx_root, &effect).unwrap();
    let vfx_read = read_effect_from_content_store(&vfx_root, &first_vfx.descriptor).unwrap();

    assert_eq!(loaded_world_info, world_info);
    assert_eq!(lobby.name, "Issue 1363 Aster Scenario");
    assert_eq!(loaded_availability, availability);
    assert_eq!(runtime_availability.player_count, 4);
    assert_eq!(vfx_read.effect, effect);
    assert!(!first_vfx.deduplicated);
    assert!(second_vfx.deduplicated);
    assert!(!scenario_path.with_extension("ron").exists());
    assert!(!availability_path.with_extension("ron").exists());
    assert!(!first_vfx.object_path.exists());

    let after = json!({
        "scenario": path_state(&scenario_path),
        "availability": path_state(&availability_path),
        "vfx": path_state(&vfx_root),
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1363,
        "source_of_truth": "Calyx Aster vault rows for scenario, scenario availability, and VFX structured content",
        "before": before,
        "after": after,
        "happy_path": {
            "scenario_name": loaded_world_info.metadata.name,
            "scenario_fingerprint": loaded_world_info.stable_fingerprint().unwrap(),
            "availability_fingerprint": loaded_availability.stable_fingerprint().unwrap(),
            "runtime_player_count": runtime_availability.player_count,
            "vfx_descriptor_digest": first_vfx.descriptor.digest,
            "vfx_sha256": vfx_read.sha256,
            "vfx_bytes": vfx_read.bytes,
            "vfx_deduplicated_second_write": second_vfx.deduplicated,
            "virtual_vfx_object_path_exists": first_vfx.object_path.exists(),
        },
        "physical_aster_files": physical_summary(&[
            ("scenario", scenario_path.clone()),
            ("availability", availability_path.clone()),
            ("vfx", vfx_root.clone()),
        ]),
    });

    let report_path = root.join("manual-fsv-report.json");
    let report_bytes = serde_json::to_vec_pretty(&report).unwrap();
    fs::write(&report_path, &report_bytes).unwrap();
    let report_readback = fs::read(&report_path).unwrap();
    assert_eq!(report_readback, report_bytes);
    println!(
        "ISSUE1363_FSV_REPORT path={} bytes={} scenario_files={} availability_files={} vfx_files={} vfx_sha={}",
        report_path.display(),
        report_readback.len(),
        report["physical_aster_files"]["stores"][0]["file_count"],
        report["physical_aster_files"]["stores"][1]["file_count"],
        report["physical_aster_files"]["stores"][2]["file_count"],
        vfx_read.sha256,
    );
}

fn scenario_world_info() -> ScenarioWorldInfo {
    ScenarioWorldInfo::new(
        MapDescription {
            name: "Issue 1363 Aster Scenario".to_string(),
            suggested_players_min: 2,
            suggested_players_max: 4,
            author: "Light in the Dark".to_string(),
            description: "Structured scenario metadata persisted through Calyx Aster.".to_string(),
            recommended_players: "2v2 verification match".to_string(),
        },
        MapOptions {
            ruleset: MapRuleset::Custom,
            start_locations: StartLocationPolicy::Fixed,
            allow_observers: true,
            hide_minimap_preview: false,
            mask_partial_vision: true,
            fixed_player_settings: true,
            fixed_forces: true,
            water_waves_cliff_shores: false,
            water_waves_rolling_shores: true,
            terrain_fog_enabled: true,
            day_night_cycle: DayNightCycle::Enabled,
        },
    )
    .unwrap()
}

fn availability_record() -> ScenarioAvailabilityRecord {
    ScenarioAvailabilityRecord::new(
        4,
        vec![PlayerAvailabilityOverrides {
            player_id: 1,
            disabled_tech: vec!["hkee".to_string()],
            disabled_abilities: vec!["AHbz".to_string()],
            upgrades: vec![PlayerUpgradeOverride {
                upgrade_id: "Rhme".to_string(),
                availability: UpgradeAvailability::Researched,
                starting_level: 2,
                max_level: 3,
            }],
            placement_priorities: PlayerPlacementPriorities {
                ally_low: vec![3],
                ally_high: vec![2],
                enemy_low: vec![4],
                enemy_high: Vec::new(),
            },
        }],
        vec![TechDependencyRule {
            required_tech_id: "hkee".to_string(),
            dependent_tech_id: "hcas".to_string(),
        }],
    )
    .unwrap()
}

fn vfx_effect() -> VfxEffectAsset {
    VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: "issue1363.vfx.aster_row".to_string(),
        display_name: "Issue 1363 Aster VFX".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "issue1363.spark".to_string(),
            display_name: "Issue 1363 Spark".to_string(),
            max_particles: 128,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(3.0)).unwrap(),
            spawn: vec![VfxSpawnModule::Burst {
                count: 32,
                time_seconds: 0.0,
            }],
            initialize: vec![
                VfxInitializeModule::Shape(VfxShape::Sphere { radius_meters: 0.5 }),
                VfxInitializeModule::Velocity(VfxVelocity::Radial {
                    speed_meters_per_second: VfxRange::new(1.0, 3.0),
                }),
                VfxInitializeModule::Color {
                    color: VfxColor::rgba(0.2, 0.7, 1.0, 1.0),
                },
                VfxInitializeModule::Size {
                    pixels: VfxRange::new(6.0, 16.0),
                },
                VfxInitializeModule::Lifetime {
                    seconds: VfxRange::new(0.25, 0.75),
                },
            ],
            update: vec![
                VfxUpdateModule::Drag { coefficient: 0.2 },
                VfxUpdateModule::ColorOverLife {
                    gradient: VfxGradient {
                        stops: vec![
                            VfxGradientStop {
                                t: 0.0,
                                color: VfxColor::rgba(0.2, 0.7, 1.0, 1.0),
                            },
                            VfxGradientStop {
                                t: 1.0,
                                color: VfxColor::rgba(0.0, 0.0, 0.0, 0.0),
                            },
                        ],
                    },
                },
                VfxUpdateModule::SizeOverLife {
                    curve: VfxCurve {
                        points: vec![
                            VfxCurvePoint { t: 0.0, value: 0.3 },
                            VfxCurvePoint { t: 0.4, value: 1.0 },
                            VfxCurvePoint { t: 1.0, value: 0.0 },
                        ],
                    },
                },
            ],
            output: vec![VfxOutputModule::Sprite {
                material_id: "mat.issue1363.spark".to_string(),
                texture_atlas_slot: "atlas.issue1363.spark".to_string(),
                blend_mode: VfxBlendMode::SoftAdditive,
            }],
        }],
    }
}

fn unique_fsv_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1363_side_store_eradication", label)
}

fn path_state(path: &Path) -> Value {
    json!({
        "path": path,
        "exists": path.exists(),
        "is_file": path.is_file(),
        "is_dir": path.is_dir(),
        "file_count": count_files(path),
    })
}

fn physical_summary(stores: &[(&str, PathBuf)]) -> Value {
    let stores = stores
        .iter()
        .map(|(name, path)| {
            json!({
                "name": name,
                "path": path,
                "exists": path.exists(),
                "is_dir": path.is_dir(),
                "file_count": count_files(path),
            })
        })
        .collect::<Vec<_>>();
    let total_files = stores
        .iter()
        .map(|store| store["file_count"].as_u64().unwrap_or_default())
        .sum::<u64>();
    json!({
        "store_count": stores.len(),
        "total_files": total_files,
        "stores": stores,
    })
}

fn count_files(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
            } else if entry_path.is_file() {
                total += 1;
            }
        }
    }
    total
}
