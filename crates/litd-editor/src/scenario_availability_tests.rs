use super::*;

#[test]
fn runtime_availability_applies_gates_clamps_and_priorities() {
    let record = fixture_record();
    let runtime = build_runtime_availability(&record).unwrap();
    let player_one = &runtime.player_states[0];

    assert_eq!(record.schema_version, 1);
    assert_eq!(record.source_issue, 1275);
    assert_eq!(runtime.player_count, 6);
    assert_eq!(player_one.player_id, 1);
    assert_eq!(
        player_one.unavailable_tech,
        vec!["hcas".to_string(), "hkee".to_string()]
    );
    assert_eq!(player_one.unavailable_abilities, vec!["AHbz".to_string()]);
    assert_eq!(player_one.upgrades[0].upgrade_id, "Rhme");
    assert_eq!(player_one.upgrades[0].starting_level, 3);
    assert_eq!(runtime.blocked_dependents[0].dependent_tech_id, "hcas");
    assert_eq!(runtime.clamped_upgrades[0].requested_level, 5);
    assert_eq!(runtime.clamped_upgrades[0].clamped_level, 3);
    assert_eq!(runtime.placement_plan[0].near_allies, vec![2]);
    assert_eq!(runtime.placement_plan[0].soft_near_allies, vec![3]);
    assert_eq!(runtime.placement_plan[0].far_enemies, vec![5]);
    assert_eq!(runtime.placement_plan[0].soft_far_enemies, vec![6]);
}

#[test]
fn invalid_rawcode_fails_loud() {
    let mut record = fixture_record();
    record.player_overrides[0]
        .disabled_tech
        .push("too-long".to_string());
    let error = record.validate().unwrap_err();

    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
}

#[test]
fn priority_self_and_overlap_fail_loud() {
    let mut record = fixture_record();
    record.player_overrides[0]
        .placement_priorities
        .ally_high
        .push(1);
    let error = record.validate().unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");

    let mut record = fixture_record();
    record.player_overrides[0]
        .placement_priorities
        .enemy_low
        .push(5);
    let error = record.validate().unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
}

#[test]
fn invalid_player_count_and_upgrade_max_fail_loud() {
    let error = ScenarioAvailabilityRecord::new(0, Vec::new(), Vec::new()).unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");

    let mut record = fixture_record();
    record.player_overrides[0].upgrades[0].max_level = 0;
    let error = record.validate().unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
}

fn fixture_record() -> ScenarioAvailabilityRecord {
    ScenarioAvailabilityRecord::new(
        6,
        vec![
            PlayerAvailabilityOverrides {
                player_id: 1,
                disabled_tech: vec!["hkee".to_string()],
                disabled_abilities: vec!["AHbz".to_string()],
                upgrades: vec![PlayerUpgradeOverride {
                    upgrade_id: "Rhme".to_string(),
                    availability: UpgradeAvailability::Researched,
                    starting_level: 5,
                    max_level: 3,
                }],
                placement_priorities: PlayerPlacementPriorities {
                    ally_low: vec![3],
                    ally_high: vec![2],
                    enemy_low: vec![6],
                    enemy_high: vec![5],
                },
            },
            PlayerAvailabilityOverrides {
                player_id: 2,
                disabled_tech: vec!["ogru".to_string()],
                disabled_abilities: vec!["AOwk".to_string()],
                upgrades: vec![PlayerUpgradeOverride {
                    upgrade_id: "Rosp".to_string(),
                    availability: UpgradeAvailability::Available,
                    starting_level: 1,
                    max_level: 3,
                }],
                placement_priorities: PlayerPlacementPriorities {
                    ally_low: Vec::new(),
                    ally_high: vec![1],
                    enemy_low: Vec::new(),
                    enemy_high: vec![5],
                },
            },
        ],
        vec![TechDependencyRule {
            required_tech_id: "hkee".to_string(),
            dependent_tech_id: "hcas".to_string(),
        }],
    )
    .unwrap()
}
