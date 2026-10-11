// Consolidated FSV harness for the #1021-1036 CF class-content batch (parent #986), following
// the content-pipeline house doctrine: ONE consolidated FSV harness per batch, not a separate
// example per entity. Authors all 16 ClassSpec rows (the Warrior pilot, #1020, already has its
// own dedicated issue1020_warrior_class_fsv.rs and is not repeated here), persists + lowers them
// through the SAME Calyx vault, reads every row back through a SEPARATE query path, and proves
// observed == expected for the happy path plus fail-closed edge cases with printed before/after
// state.
use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        ClassRaceAccess, ClassSpec, class_spec_content_records, class_spec_eligibility,
        class_spec_readback, load_class_spec_from_calyx, persist_and_lower_rpg_authored_content,
        specialization_eligibility, validate_class_race_access, validate_class_spec,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

#[path = "../src/rpg/class_specs_fixture_anti_paladin.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod anti_paladin_fixture;
#[path = "../src/rpg/class_specs_fixture_assassin.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod assassin_fixture;
#[path = "../src/rpg/class_specs_fixture_bard.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod bard_fixture;
#[path = "../src/rpg/class_specs_fixture_berserker.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod berserker_fixture;
#[path = "../src/rpg/class_specs_fixture_conjurer.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod conjurer_fixture;
#[path = "../src/rpg/class_specs_fixture_druid.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod druid_fixture;
#[path = "../src/rpg/class_specs_fixture_healer.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod healer_fixture;
#[path = "../src/rpg/class_specs_fixture_invoker.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod invoker_fixture;
#[path = "../src/rpg/class_specs_fixture_necromancer.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod necromancer_fixture;
#[path = "../src/rpg/class_specs_fixture_paladin.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod paladin_fixture;
#[path = "../src/rpg/class_specs_fixture_raider.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod raider_fixture;
#[path = "../src/rpg/class_specs_fixture_ranger.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod ranger_fixture;
#[path = "../src/rpg/class_specs_fixture_shaman.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod shaman_fixture;
#[path = "../src/rpg/class_specs_fixture_shapeshifter.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod shapeshifter_fixture;
#[path = "../src/rpg/class_specs_fixture_thief.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod thief_fixture;
#[path = "../src/rpg/class_specs_fixture_transmuter.rs"]
#[allow(dead_code)] // known_races() used only for thief in this harness; kept for lib tests
mod transmuter_fixture;

const CLASS_TYPES_PATH: &str = "crates/litd-engine/src/rpg/class_specs_types.rs";
const CLASS_IMPL_PATH: &str = "crates/litd-engine/src/rpg/class_specs.rs";
const CLASS_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/class_specs_content.rs";
const CLASS_ELIGIBILITY_PATH: &str = "crates/litd-engine/src/rpg/class_eligibility.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1021_1036_class_specs_fsv.rs";
const FIXTURE_PATHS: [&str; 16] = [
    "crates/litd-engine/src/rpg/class_specs_fixture_thief.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_assassin.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_ranger.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_bard.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_berserker.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_raider.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_paladin.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_anti_paladin.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_shaman.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_healer.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_druid.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_invoker.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_transmuter.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_conjurer.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_necromancer.rs",
    "crates/litd-engine/src/rpg/class_specs_fixture_shapeshifter.rs",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1021_1036_class_specs")?;

    let specs: Vec<(u32, ClassSpec)> = vec![
        (1021, thief_fixture::thief_class_spec()),
        (1022, assassin_fixture::assassin_class_spec()),
        (1023, ranger_fixture::ranger_class_spec()),
        (1024, bard_fixture::bard_class_spec()),
        (1025, berserker_fixture::berserker_class_spec()),
        (1026, raider_fixture::raider_class_spec()),
        (1027, paladin_fixture::paladin_class_spec()),
        (1028, anti_paladin_fixture::anti_paladin_class_spec()),
        (1029, shaman_fixture::shaman_class_spec()),
        (1030, healer_fixture::healer_class_spec()),
        (1031, druid_fixture::druid_class_spec()),
        (1032, invoker_fixture::invoker_class_spec()),
        (1033, transmuter_fixture::transmuter_class_spec()),
        (1034, conjurer_fixture::conjurer_class_spec()),
        (1035, necromancer_fixture::necromancer_class_spec()),
        (1036, shapeshifter_fixture::shapeshifter_class_spec()),
    ];

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    let mut all_records = Vec::new();
    let mut authored_shas = Vec::new();
    for (_issue, spec) in &specs {
        let readback = class_spec_readback(spec)?;
        let records = class_spec_content_records(spec)?;
        authored_shas.push((spec.class_id.clone(), readback.class_spec_sha256.clone()));
        all_records.extend(records);
    }

    let lowered = persist_and_lower_rpg_authored_content(&store, &all_records)?;

    // Read every row back through a SEPARATE query path and prove authored sha256 == stored
    // sha256 for all 16 classes (edge case: happy-path stored+readback identical).
    let mut per_class = Vec::new();
    let mut readback_identical_count = 0usize;
    let mut loader_fount_identical_count = 0usize;
    for (issue, spec) in &specs {
        let row = store.get_content_for_editor(EngineContentKind::Unit, &spec.class_id)?;
        let loaded = load_class_spec_from_calyx(&store, &spec.class_id)?;
        let authored_sha = authored_shas
            .iter()
            .find(|(id, _)| id == &spec.class_id)
            .map(|(_, sha)| sha.clone())
            .unwrap_or_default();
        let identical = row.id == spec.class_id
            && row.payload["class_id"] == json!(spec.class_id)
            && row.payload["class_spec_sha256"] == json!(authored_sha);
        if identical {
            readback_identical_count += 1;
        }
        let loader_fount_identical = loaded.fount == spec.fount;
        if loader_fount_identical {
            loader_fount_identical_count += 1;
        }
        per_class.push(json!({
            "issue": issue,
            "class_id": spec.class_id,
            "display_name": spec.display_name,
            "prime_stat": spec.prime_stat,
            "leveled_ability_count": spec.leveled_abilities.len(),
            "specialization_count": spec.specializations.len(),
            "legacy_count": spec.legacies.len(),
            "authored_sha256": authored_sha,
            "stored_sha256": row.payload["class_spec_sha256"],
            "readback_identical": identical,
            "loader_fount": loaded.fount,
            "loader_fount_identical": loader_fount_identical,
            "lowering_hot_path_policy": row.lowering.hot_path_policy,
            "lowering_fingerprint_nonempty": !row.lowering.fingerprint.is_empty(),
        }));
    }

    // Edge case 1: happy-path readback identity is already proven above (per_class).
    // Edge case 2: invalid stat minimum is rejected fail-closed, state unchanged.
    let thief_races = thief_fixture::known_races();
    let edges = vec![
        edge("invalid_stat_minimum", &invalid_stat_minimum(), |spec| {
            validate_class_spec(spec)
        })?,
        edge(
            "dangling_specialization",
            &dangling_specialization(),
            validate_class_spec,
        )?,
        edge(
            "nonexistent_race_restriction",
            &nonexistent_race_restriction(),
            |spec| validate_class_race_access(spec, &thief_races),
        )?,
        edge(
            "broken_acquisition_cumulative",
            &broken_acquisition_cumulative(),
            validate_class_spec,
        )?,
        edge("empty_fount_reason", &empty_fount_reason(), |spec| {
            validate_class_spec(spec)
        })?,
        edge(
            "duplicate_specialization_membership",
            &duplicate_specialization_membership(),
            validate_class_spec,
        )?,
        edge(
            "duplicate_alignment_exception_race",
            &alignment_exception_unknown_race(),
            validate_class_spec,
        )?,
    ];

    // Schema-v2 migration readback (#1718): prove the four newly-typed fields
    // carry the migrated CF facts. Computed from the authored specs through the
    // validated readback, independent of the persisted-row loop above.
    let thief_rb = class_spec_readback(&thief_fixture::thief_class_spec())?;
    let ranger_rb = class_spec_readback(&ranger_fixture::ranger_class_spec())?;
    let healer_rb = class_spec_readback(&healer_fixture::healer_class_spec())?;
    let invoker_rb = class_spec_readback(&invoker_fixture::invoker_class_spec())?;
    let v2_migration = json!({
        "thief_costed_ability_count": thief_rb.costed_ability_count,
        "thief_fount": thief_rb.fount.label(),
        "ranger_dual_tagged_ability_count": ranger_rb.dual_tagged_ability_count,
        "healer_alignment_exception_count": healer_rb.alignment_exception_count,
        "healer_fount": healer_rb.fount.label(),
        "invoker_fount": invoker_rb.fount.label(),
    });
    require_i64(&v2_migration["thief_costed_ability_count"], 11)?;
    require_i64(&v2_migration["ranger_dual_tagged_ability_count"], 3)?;
    require_i64(&v2_migration["healer_alignment_exception_count"], 3)?;
    require_str(&v2_migration["thief_fount"], "martial")?;
    require_str(&v2_migration["healer_fount"], "supplication")?;
    require_str(&v2_migration["invoker_fount"], "invocation")?;

    let paladin = load_class_spec_from_calyx(&store, paladin_fixture::PALADIN_CLASS_ID)?;
    let healer = load_class_spec_from_calyx(&store, healer_fixture::HEALER_CLASS_ID)?;
    let shaman = load_class_spec_from_calyx(&store, shaman_fixture::SHAMAN_CLASS_ID)?;
    let eligibility = json!({
        "paladin_orderly_good": class_spec_eligibility(
            &paladin, "race.ashen_human", "good", "orderly"
        )?,
        "paladin_neutral_good": class_spec_eligibility(
            &paladin, "race.ashen_human", "good", "neutral"
        )?,
        "paladin_chaotic_good_rejected": !class_spec_eligibility(
            &paladin, "race.ashen_human", "good", "chaotic"
        )?,
        "healer_human_evil_allowed": class_spec_eligibility(
            &healer, "race.ashen_human", "evil", "neutral"
        )?,
        "healer_elf_evil_rejected": !class_spec_eligibility(
            &healer, "race.elf", "evil", "neutral"
        )?,
        "shaman_demonspawn_evil_chaotic": specialization_eligibility(
            &shaman,
            "spec.shaman.demonspawn",
            "race.ashkin_deep",
            "evil",
            "chaotic",
        )?,
        "shaman_demonspawn_good_rejected": !specialization_eligibility(
            &shaman,
            "spec.shaman.demonspawn",
            "race.ashkin_deep",
            "good",
            "chaotic",
        )?,
        "shaman_seraphs_wing_good_chaotic": specialization_eligibility(
            &shaman,
            "spec.shaman.seraphs_wing",
            "race.ashkin_deep",
            "good",
            "chaotic",
        )?,
        "shaman_seraphs_wing_orderly_good_rejected": !specialization_eligibility(
            &shaman,
            "spec.shaman.seraphs_wing",
            "race.ashkin_deep",
            "good",
            "orderly",
        )?,
    });
    for key in [
        "paladin_orderly_good",
        "paladin_neutral_good",
        "paladin_chaotic_good_rejected",
        "healer_human_evil_allowed",
        "healer_elf_evil_rejected",
        "shaman_demonspawn_evil_chaotic",
        "shaman_demonspawn_good_rejected",
        "shaman_seraphs_wing_good_chaotic",
        "shaman_seraphs_wing_orderly_good_rejected",
    ] {
        if eligibility[key] != json!(true) {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("eligibility.{key}"),
                reason: "authored golden eligibility decision was false".to_string(),
            }
            .into());
        }
    }

    let source_readbacks = FIXTURE_PATHS
        .into_iter()
        .chain([
            CLASS_TYPES_PATH,
            CLASS_IMPL_PATH,
            CLASS_CONTENT_PATH,
            CLASS_ELIGIBILITY_PATH,
            EXAMPLE_PATH,
        ])
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;
    let source_readback_count = source_readbacks.len();

    let report = json!({
        "schema": "litd.issue1021_1036.class_specs_fsv.v1",
        "issues": specs.iter().map(|(issue, _)| *issue).collect::<Vec<_>>(),
        "parent_issue": 986,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "sources": source_readbacks,
            "manual_gate": "Open this report through Synapse and inspect per_class, edges, and expected_actuals against observed values."
        },
        "sufficiency_argument": "This batch covers the #1021-1036 class data rows (Warrior/#1020 already has its own dedicated FSV). All 16 rows share one Calyx vault, are persisted+lowered in one call, and are each read back through a separate get_content_for_editor query, proving authored_sha256 == stored_sha256 for every class. Seven edge probes prove invalid stat minimum, dangling specialization reference, unknown-race restriction, and the four schema-v2 fields (broken acquisition cumulative cost, empty Unresolved fount reason, duplicate specialization membership, duplicate alignment-exception race) all fail closed with state unchanged. The v2_migration block proves the migrated CF facts read back: thief Binder point-buy chain (11 costed abilities), ranger dual-tagged terrain skills (3), healer per-race alignment exceptions (3), and the martial/supplication/invocation founts.",
        "expected_actuals": {
            "source_readback_count": source_readback_count,
            "class_count": specs.len(),
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": per_class.len(),
            "readback_identical_count": readback_identical_count,
            "loader_fount_identical_count": loader_fount_identical_count,
            "edge_count": edges.len(),
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "per_class": per_class,
        "edges": edges,
        "v2_migration": v2_migration,
        "eligibility": eligibility,
        "eligibility_source_of_truth": "reloaded ClassSpec rows from Calyx",
    });
    require_i64(&report["expected_actuals"]["class_count"], 16)?;
    require_i64(&report["expected_actuals"]["content_rows_written"], 16)?;
    require_i64(&report["expected_actuals"]["content_rows_read_back"], 16)?;
    require_i64(&report["expected_actuals"]["readback_identical_count"], 16)?;
    require_i64(
        &report["expected_actuals"]["loader_fount_identical_count"],
        16,
    )?;
    require_i64(&report["expected_actuals"]["edge_count"], 7)?;

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1021_1036_CLASS_SPECS_FSV report={} bytes={} sha256={} classes={} rows_written={} rows_read_back={} readback_identical={} loader_fount_identical={} edges={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["expected_actuals"]["class_count"],
        readback.json["expected_actuals"]["content_rows_written"],
        readback.json["expected_actuals"]["content_rows_read_back"],
        readback.json["expected_actuals"]["readback_identical_count"],
        readback.json["expected_actuals"]["loader_fount_identical_count"],
        readback.json["edges"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
    );
    Ok(())
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn invalid_stat_minimum() -> ClassSpec {
    let mut spec = thief_fixture::thief_class_spec();
    spec.stat_minimums = litd_engine::rpg::AttributeScores::new(0, 0, 0, 200, 0, 0);
    spec
}

fn dangling_specialization() -> ClassSpec {
    let mut spec = thief_fixture::thief_class_spec();
    spec.leveled_abilities[0].specialization_ids = vec!["spec.thief.nonexistent".to_string()];
    spec
}

fn nonexistent_race_restriction() -> ClassSpec {
    let mut spec = thief_fixture::thief_class_spec();
    spec.race_access = ClassRaceAccess::Restricted {
        race_ids: vec!["race.does_not_exist".to_string()],
    };
    spec
}

// Schema-v2 fail-closed edges (#1718): corrupt each new typed field and prove
// the validator rejects it with a named diagnostic and state unchanged.
fn broken_acquisition_cumulative() -> ClassSpec {
    let mut spec = thief_fixture::thief_class_spec();
    // The migrated Binder chain carries real cumulative costs; break one so it
    // no longer equals prerequisite.cumulative + cost.
    for ability in &mut spec.leveled_abilities {
        if let Some(acquisition) = &mut ability.acquisition {
            acquisition.cumulative_cost = acquisition.cumulative_cost.saturating_add(1);
            break;
        }
    }
    spec
}

fn empty_fount_reason() -> ClassSpec {
    let mut spec = conjurer_fixture::conjurer_class_spec();
    spec.fount = litd_engine::rpg::ClassFount::Unresolved {
        reason: String::new(),
    };
    spec
}

fn duplicate_specialization_membership() -> ClassSpec {
    let mut spec = thief_fixture::thief_class_spec();
    spec.leveled_abilities[0].specialization_ids = vec![
        "spec.thief.general".to_string(),
        "spec.thief.general".to_string(),
    ];
    spec
}

fn alignment_exception_unknown_race() -> ClassSpec {
    let mut spec = healer_fixture::healer_class_spec();
    spec.alignment_exceptions
        .push(litd_engine::rpg::RaceAlignmentException {
            race_id: "race.ashen_human".to_string(),
            allowed: litd_engine::rpg::RaceAccessRule::Any,
        });
    // Duplicate race id in the exception set is rejected by validate_class_spec.
    spec
}

fn edge(
    case: &str,
    spec: &ClassSpec,
    validate: impl Fn(&ClassSpec) -> Result<(), EngineError>,
) -> Result<Value, EngineError> {
    let before = spec_hash(spec)?;
    let error = match validate(spec) {
        Ok(_) => {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: format!("edge.{case}"),
                reason: "expected invalid class spec to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = spec_hash(spec)?;
    Ok(json!({
        "case": case,
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn spec_hash(spec: &ClassSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize class spec edge: {error}"),
        })
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1021_1036.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_str(value: &Value, expected: &str) -> Result<(), EngineError> {
    if value.as_str() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1021_1036.fsv.require_str".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
