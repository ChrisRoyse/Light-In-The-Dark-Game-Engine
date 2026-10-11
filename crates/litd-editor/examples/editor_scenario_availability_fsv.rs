use litd_engine::fsv_artifacts::env_path;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use litd_editor::{
    history::{stable_fingerprint, to_stable_ron},
    scenario_availability::{
        PlayerAvailabilityOverrides, PlayerPlacementPriorities, PlayerUpgradeOverride,
        ScenarioAvailabilityRecord, TechDependencyRule, UpgradeAvailability,
        build_runtime_availability, load_scenario_availability_record,
        save_scenario_availability_record,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_SCENARIO_AVAILABILITY_FSV_STATS",
        "tmp/issue1275-scenario-availability/stats.txt",
    );
    let record_path = env_path(
        "LITD_EDITOR_SCENARIO_AVAILABILITY_FSV_RECORD",
        "tmp/issue1275-scenario-availability/availability_record.ron",
    );
    let runtime_path = env_path(
        "LITD_EDITOR_SCENARIO_AVAILABILITY_FSV_RUNTIME",
        "tmp/issue1275-scenario-availability/runtime_availability.ron",
    );

    let record = fixture_record()?;
    let before_fingerprint = record.stable_fingerprint()?;
    save_scenario_availability_record(&record_path, &record)?;
    let readback = load_scenario_availability_record(&record_path)?;
    let readback_fingerprint = readback.stable_fingerprint()?;
    let runtime = build_runtime_availability(&readback)?;
    write_text(&runtime_path, &to_stable_ron(&runtime)?)?;
    let runtime_readback_text = fs::read_to_string(&runtime_path)?;
    let runtime_readback_hash = stable_fingerprint(&runtime)?;

    let edge_invalid_rawcode = edge_invalid_rawcode(&record_path)?;
    let edge_self_priority = edge_self_priority(&record_path)?;
    let edge_upgrade_max_zero = edge_upgrade_max_zero(&record_path)?;

    let player_one = runtime
        .player_states
        .iter()
        .find(|state| state.player_id == 1)
        .expect("fixture contains player 1");
    let player_one_plan = runtime
        .placement_plan
        .iter()
        .find(|plan| plan.player_id == 1)
        .expect("fixture contains player 1 placement");
    let player_one_upgrade = player_one
        .upgrades
        .iter()
        .find(|upgrade| upgrade.upgrade_id == "Rhme")
        .expect("fixture contains Rhme");

    write_text(
        &stats_path,
        &format!(
            "issue=1275\n\
             record_file={}\n\
             runtime_file={}\n\
             record_before_fingerprint={}\n\
             record_readback_fingerprint={}\n\
             record_readback_matches={}\n\
             player_count={}\n\
             player_override_count={}\n\
             tech_dependency_count={}\n\
             player1_unavailable_tech={}\n\
             player1_unavailable_abilities={}\n\
             player1_upgrade=Rhme\n\
             player1_upgrade_availability={:?}\n\
             player1_upgrade_starting_level={}\n\
             blocked_dependents={}\n\
             clamped_upgrades={}\n\
             placement_player1_near_allies={}\n\
             placement_player1_soft_near_allies={}\n\
             placement_player1_far_enemies={}\n\
             placement_player1_soft_far_enemies={}\n\
             runtime_readback_contains_blocked_dependent={}\n\
             runtime_readback_contains_clamped_upgrade={}\n\
             runtime_readback_contains_placement={}\n\
             runtime_readback_fingerprint={}\n\
             edge_invalid_rawcode_error={}\n\
             edge_invalid_rawcode_before_disabled_tech={}\n\
             edge_invalid_rawcode_attempted_disabled_tech={}\n\
             edge_invalid_rawcode_after_disabled_tech={}\n\
             edge_invalid_rawcode_before_fingerprint={}\n\
             edge_invalid_rawcode_after_fingerprint={}\n\
             edge_invalid_rawcode_record_unchanged={}\n\
             edge_self_priority_error={}\n\
             edge_self_priority_before_ally_high={}\n\
             edge_self_priority_attempted_ally_high={}\n\
             edge_self_priority_after_ally_high={}\n\
             edge_self_priority_before_fingerprint={}\n\
             edge_self_priority_after_fingerprint={}\n\
             edge_self_priority_record_unchanged={}\n\
             edge_upgrade_max_zero_error={}\n\
             edge_upgrade_max_zero_before_upgrade={}\n\
             edge_upgrade_max_zero_attempted_upgrade={}\n\
             edge_upgrade_max_zero_after_upgrade={}\n\
             edge_upgrade_max_zero_before_fingerprint={}\n\
             edge_upgrade_max_zero_after_fingerprint={}\n\
             edge_upgrade_max_zero_record_unchanged={}\n",
            record_path.display(),
            runtime_path.display(),
            before_fingerprint,
            readback_fingerprint,
            before_fingerprint == readback_fingerprint,
            readback.player_count,
            readback.player_overrides.len(),
            readback.tech_dependencies.len(),
            player_one.unavailable_tech.join("|"),
            player_one.unavailable_abilities.join("|"),
            player_one_upgrade.availability,
            player_one_upgrade.starting_level,
            blocked_summary(&runtime.blocked_dependents),
            clamp_summary(&runtime.clamped_upgrades),
            player_one_plan
                .near_allies
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            player_one_plan
                .soft_near_allies
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            player_one_plan
                .far_enemies
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            player_one_plan
                .soft_far_enemies
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            runtime_readback_text.contains("dependent_tech_id: \"hcas\""),
            runtime_readback_text.contains("clamped_level: 3"),
            runtime_readback_text.contains("near_allies: [2]"),
            runtime_readback_hash,
            edge_invalid_rawcode.error_code,
            edge_invalid_rawcode.before_state,
            edge_invalid_rawcode.attempted_state,
            edge_invalid_rawcode.after_state,
            edge_invalid_rawcode.before_fingerprint,
            edge_invalid_rawcode.after_fingerprint,
            edge_invalid_rawcode.record_unchanged(),
            edge_self_priority.error_code,
            edge_self_priority.before_state,
            edge_self_priority.attempted_state,
            edge_self_priority.after_state,
            edge_self_priority.before_fingerprint,
            edge_self_priority.after_fingerprint,
            edge_self_priority.record_unchanged(),
            edge_upgrade_max_zero.error_code,
            edge_upgrade_max_zero.before_state,
            edge_upgrade_max_zero.attempted_state,
            edge_upgrade_max_zero.after_state,
            edge_upgrade_max_zero.before_fingerprint,
            edge_upgrade_max_zero.after_fingerprint,
            edge_upgrade_max_zero.record_unchanged(),
        ),
    )?;
    println!(
        "wrote scenario availability FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

fn fixture_record() -> Result<ScenarioAvailabilityRecord, litd_engine::EngineError> {
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
}

struct EdgeAudit {
    error_code: String,
    before_state: String,
    attempted_state: String,
    after_state: String,
    before_fingerprint: String,
    after_fingerprint: String,
}

impl EdgeAudit {
    fn record_unchanged(&self) -> bool {
        self.before_fingerprint == self.after_fingerprint && self.before_state == self.after_state
    }
}

fn edge_invalid_rawcode(record_path: &Path) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before = load_scenario_availability_record(record_path)?;
    let before_fingerprint = before.stable_fingerprint()?;
    let before_state = join_strings(&player_one(&before).disabled_tech);
    let mut candidate = before.clone();
    candidate.player_overrides[0]
        .disabled_tech
        .push("too-long".to_string());
    let attempted_state = join_strings(&candidate.player_overrides[0].disabled_tech);
    let error_code =
        validation_error_code(candidate.validate(), "invalid rawcode unexpectedly passed")?;
    let after = load_scenario_availability_record(record_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        attempted_state,
        after_state: join_strings(&player_one(&after).disabled_tech),
        before_fingerprint,
        after_fingerprint: after.stable_fingerprint()?,
    })
}

fn edge_self_priority(record_path: &Path) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before = load_scenario_availability_record(record_path)?;
    let before_fingerprint = before.stable_fingerprint()?;
    let before_state = join_players(&player_one(&before).placement_priorities.ally_high);
    let mut candidate = before.clone();
    candidate.player_overrides[0]
        .placement_priorities
        .ally_high
        .push(1);
    let attempted_state =
        join_players(&candidate.player_overrides[0].placement_priorities.ally_high);
    let error_code =
        validation_error_code(candidate.validate(), "self priority unexpectedly passed")?;
    let after = load_scenario_availability_record(record_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        attempted_state,
        after_state: join_players(&player_one(&after).placement_priorities.ally_high),
        before_fingerprint,
        after_fingerprint: after.stable_fingerprint()?,
    })
}

fn edge_upgrade_max_zero(record_path: &Path) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before = load_scenario_availability_record(record_path)?;
    let before_fingerprint = before.stable_fingerprint()?;
    let before_state = upgrade_state(player_one(&before));
    let mut candidate = before.clone();
    candidate.player_overrides[0].upgrades[0].max_level = 0;
    let attempted_state = upgrade_state(&candidate.player_overrides[0]);
    let error_code =
        validation_error_code(candidate.validate(), "zero max upgrade unexpectedly passed")?;
    let after = load_scenario_availability_record(record_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        attempted_state,
        after_state: upgrade_state(player_one(&after)),
        before_fingerprint,
        after_fingerprint: after.stable_fingerprint()?,
    })
}

fn blocked_summary(blocked: &[litd_editor::scenario_availability::BlockedDependentTech]) -> String {
    blocked
        .iter()
        .map(|entry| {
            format!(
                "p{}:{}->{}",
                entry.player_id, entry.required_tech_id, entry.dependent_tech_id
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn clamp_summary(clamps: &[litd_editor::scenario_availability::ClampedUpgradeLevel]) -> String {
    clamps
        .iter()
        .map(|entry| {
            format!(
                "p{}:{}:{}->{}",
                entry.player_id, entry.upgrade_id, entry.requested_level, entry.clamped_level
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn player_one(record: &ScenarioAvailabilityRecord) -> &PlayerAvailabilityOverrides {
    record
        .player_overrides
        .iter()
        .find(|player| player.player_id == 1)
        .expect("fixture contains player 1")
}

fn join_strings(values: &[String]) -> String {
    values.join("|")
}

fn join_players(values: &[u8]) -> String {
    values
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join("|")
}

fn upgrade_state(player: &PlayerAvailabilityOverrides) -> String {
    player
        .upgrades
        .iter()
        .map(|upgrade| {
            format!(
                "{}:starting{}:max{}:{:?}",
                upgrade.upgrade_id, upgrade.starting_level, upgrade.max_level, upgrade.availability
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn validation_error_code(
    result: Result<(), litd_engine::EngineError>,
    message: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    match result {
        Ok(()) => Err(io::Error::new(io::ErrorKind::InvalidData, message).into()),
        Err(error) => Ok(error.code().to_string()),
    }
}

fn write_text(path: &PathBuf, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
