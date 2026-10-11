//! Corpus-wide reset+repop coverage suite (#1668). Assembles EVERY authored
//! AreaSpec (the same single-source corpus the #1704 registry and #1667 terrain
//! suites use), assigns each a reset+repop profile, and proves the profiled set is
//! EXACTLY the registry set -- so no authored area is left without a reset profile
//! and no profile is an orphan. Also proves Calyx persist+readback of the profiled
//! specs and that missing coverage / a bogus profile fails loud by name.
//!
//! CF facts (transcribed, cited): the reset CADENCE is helpfile 98 (TICK) --
//! "Area resetting, otherwise known as repopulation, happens roughly every 3
//! area_minutes if no one is in the area; less often (15 area_minutes) if some one
//! is ... objects lying on the ground will not be regenerated if anyone is in the
//! area when it resets." Scarcity is helpfile 61 + #973/#974 (capped items do not
//! respawn while held).

use std::collections::BTreeSet;

use crate::{
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AREA_RESET_REPOP_TABLE_ID, AreaResetRepopProfile, CF_EMPTY_RESET_AREA_MINUTES,
        CF_OCCUPIED_RESET_AREA_MINUTES, RPG_AREA_RESET_REPOP_SCHEMA, RepopPolicy, ResetDerivation,
        area_reset_repop_content_records, area_reset_repop_readback, build_area_reset_repop_table,
        derive_area_reset_repop, persist_and_lower_rpg_authored_content, repop_policy_for_kind,
        validate_area_reset_repop, validate_reset_repop_coverage,
    },
};

use crate::rpg_area_registry_corpus_tests::full_authored_corpus;

fn registry_ids() -> BTreeSet<String> {
    full_authored_corpus()
        .iter()
        .map(|spec| spec.area_id.clone())
        .collect()
}

#[test]
fn issue1668_dump_reset_profiles() {
    // Diagnostic (printed under --nocapture): every corpus area, its kind, and its
    // derived repop policy. Non-asserting authoring worklist.
    for spec in full_authored_corpus() {
        match derive_area_reset_repop(&spec) {
            Ok(entry) => eprintln!(
                "PROFILED {} [{:?}] -> {} (boss_timer={})",
                spec.area_id,
                spec.area_kind,
                entry.repop_policy.label(),
                entry.has_boss_timer
            ),
            Err(error) => eprintln!(
                "UNPROFILED {} [{:?}]: {error}",
                spec.area_id, spec.area_kind
            ),
        }
    }
}

#[test]
fn issue1668_every_authored_area_has_reset_repop() {
    let corpus = full_authored_corpus();
    let table = build_area_reset_repop_table(&corpus)
        .unwrap_or_else(|e| panic!("reset+repop profiling failed: {e}"));
    let coverage = validate_reset_repop_coverage(&registry_ids(), &table)
        .unwrap_or_else(|e| panic!("coverage lint failed: {e}"));

    assert_eq!(
        coverage.profiled_area_count, coverage.registry_area_count,
        "every authored area must carry a reset+repop profile"
    );
    assert!(
        coverage.registry_area_count >= 100,
        "expected the full Thera corpus (>=100 areas), got {}",
        coverage.registry_area_count
    );

    let readback = area_reset_repop_readback(&table).unwrap();
    eprintln!(
        "issue1668 coverage: areas={} policy_histogram={:?} boss_timers={} rearms_doors={} \
         rearms_traps={} cf_documented_policy={} derived_from_kind_policy={} \
         empty_reset_min={} occupied_reset_max={}",
        readback.area_count,
        readback.policy_histogram,
        readback.boss_timer_count,
        readback.rearms_doors_locks_count,
        readback.rearms_traps_count,
        readback.cf_documented_policy_count,
        readback.derived_from_kind_policy_count,
        readback.min_empty_reset_area_minutes,
        readback.max_occupied_reset_area_minutes,
    );

    // The CF-documented global cadence (helpfile 98) holds for every area: the
    // smallest empty-reset is 3 area_minutes and the largest occupied-reset is 15.
    assert_eq!(
        readback.min_empty_reset_area_minutes, CF_EMPTY_RESET_AREA_MINUTES,
        "every area's empty-reset cadence must be the CF 3 area_minute rule"
    );
    assert_eq!(
        readback.max_occupied_reset_area_minutes, CF_OCCUPIED_RESET_AREA_MINUTES,
        "every area's occupied-reset cadence must be the CF 15 area_minute rule"
    );
    // Cadence is CF-documented for all; policy is derived-by-kind for all (the
    // override table is empty pending per-area CF sources).
    assert_eq!(
        readback.cf_documented_policy_count + readback.derived_from_kind_policy_count,
        readback.area_count,
        "every profile is either CF-documented or derived-by-kind"
    );

    // The corpus spans cities (dense), roads (patrol), wilderness/regions (roaming),
    // dungeons (guarded interiors), and lairs (boss). All five policies present, no
    // blind spot.
    assert_eq!(
        coverage.distinct_policy_count, 5,
        "all five repop policies must be represented; histogram={:?}",
        coverage.policy_histogram
    );
    for policy in RepopPolicy::all() {
        assert!(
            readback
                .policy_histogram
                .get(policy.label())
                .copied()
                .unwrap_or(0)
                > 0,
            "repop policy {} has zero profiled areas (blind spot)",
            policy.label()
        );
    }
    // Boss lairs carry boss timers (#1662); guarded interiors + lairs re-arm traps
    // (trapper parity, #1668). Both counts are non-zero because the corpus has both.
    assert!(
        readback.boss_timer_count > 0,
        "boss lairs must contribute boss timers (#1662)"
    );
    assert!(
        readback.rearms_traps_count > 0,
        "dungeons/lairs must re-arm traps (#1668 trapper parity)"
    );
    assert_eq!(readback.table_sha256.len(), 64);
}

#[test]
fn issue1668_policy_matches_kind_for_representatives() {
    // Prove the by-kind derivation is faithful for one representative of each kind
    // family present in the corpus.
    use crate::rpg::AreaKind;
    assert_eq!(
        repop_policy_for_kind(AreaKind::City),
        RepopPolicy::CivilizedDense
    );
    assert_eq!(
        repop_policy_for_kind(AreaKind::Town),
        RepopPolicy::CivilizedDense
    );
    assert_eq!(
        repop_policy_for_kind(AreaKind::CabalHall),
        RepopPolicy::CivilizedDense
    );
    assert_eq!(
        repop_policy_for_kind(AreaKind::Road),
        RepopPolicy::RoadPatrol
    );
    assert_eq!(
        repop_policy_for_kind(AreaKind::Wilderness),
        RepopPolicy::WildRoaming
    );
    assert_eq!(
        repop_policy_for_kind(AreaKind::Region),
        RepopPolicy::WildRoaming
    );
    assert_eq!(
        repop_policy_for_kind(AreaKind::Dungeon),
        RepopPolicy::GuardedInterior
    );
    assert_eq!(repop_policy_for_kind(AreaKind::Lair), RepopPolicy::BossLair);
}

#[test]
fn issue1668_missing_coverage_fails_loud_by_name() {
    // Edge: drop one area's profile and prove the coverage lint fails loud naming
    // the unprofiled area (not a silent pass).
    let corpus = full_authored_corpus();
    let mut table = build_area_reset_repop_table(&corpus).unwrap();
    let dropped = table.entries.remove(0).area_id;
    let error = validate_reset_repop_coverage(&registry_ids(), &table)
        .expect_err("dropping a profile must fail coverage");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    let message = error.to_string();
    assert!(
        message.contains("have no reset+repop profile"),
        "must fail as an unprofiled-areas error: {message}"
    );
    assert!(
        message.contains(&dropped),
        "diagnostic must name the unprofiled area {dropped}: {message}"
    );
}

#[test]
fn issue1668_bogus_cadence_fails_loud_by_name() {
    // Edge: an occupied-reset FASTER than the empty-reset inverts the CF helpfile 98
    // rule and must fail loud, naming the area.
    let mut bogus = derive_area_reset_repop(&full_authored_corpus()[0]).unwrap();
    bogus.area_id = "area.thera.__bogus__".to_string();
    bogus.empty_reset_area_minutes = 10;
    bogus.occupied_reset_area_minutes = 2;
    let error = validate_area_reset_repop(&bogus).expect_err("inverted cadence must fail");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    let message = error.to_string();
    assert!(
        message.contains("area.thera.__bogus__") && message.contains("resets faster"),
        "diagnostic must name the area and the inverted cadence: {message}"
    );
}

#[test]
fn issue1668_bogus_scarcity_flag_fails_loud() {
    // Edge: a profile claiming capped items respawn while held violates the scarcity
    // ledger (#973/#974, helpfile 61) and must fail loud.
    let mut bogus = derive_area_reset_repop(&full_authored_corpus()[0]).unwrap();
    bogus.capped_items_respawn_while_held = true;
    let error = validate_area_reset_repop(&bogus).expect_err("respawning capped items must fail");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    assert!(
        error.to_string().contains("scarcity ledger"),
        "must fail as a scarcity-ledger violation: {error}"
    );
}

#[test]
fn issue1668_bogus_policy_flag_fails_loud() {
    // Edge: a road that claims a boss timer disagrees with its RoadPatrol
    // affordances and must fail loud.
    let mut bogus = AreaResetRepopProfile {
        area_id: "area.thera.__bogus_road__".to_string(),
        repop_policy: RepopPolicy::RoadPatrol,
        empty_reset_area_minutes: CF_EMPTY_RESET_AREA_MINUTES,
        occupied_reset_area_minutes: CF_OCCUPIED_RESET_AREA_MINUTES,
        regen_objects_only_when_empty: true,
        capped_items_respawn_while_held: false,
        rearms_doors_locks: false,
        rearms_traps: false,
        has_boss_timer: true,
        population_density_tier: 1,
        cadence_derivation: ResetDerivation::CfDocumented,
        policy_derivation: ResetDerivation::DerivedFromKind,
        note: "deliberately bogus".to_string(),
    };
    let error = validate_area_reset_repop(&bogus).expect_err("road with boss timer must fail");
    assert!(
        error.to_string().contains("flags disagree"),
        "must fail as a policy-flag mismatch: {error}"
    );
    // Fixing the flag makes it valid again (proves the check is precise).
    bogus.has_boss_timer = false;
    assert!(validate_area_reset_repop(&bogus).is_ok());
}

#[test]
fn issue1668_persists_and_reads_back_from_calyx() {
    let corpus = full_authored_corpus();
    let table = build_area_reset_repop_table(&corpus).unwrap();
    let records = area_reset_repop_content_records(&table).unwrap();
    // One summary row plus one row per profiled area.
    assert_eq!(records.len(), table.entries.len() + 1);
    assert!(records.iter().all(|r| {
        r.source
            .path
            .starts_with("editor://rpg/thera/area_reset_repop/")
    }));

    let (store, _vault) = content_store("issue1668");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, records.len());

    let summary = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            &format!("{AREA_RESET_REPOP_TABLE_ID}.summary"),
        )
        .unwrap();
    assert_eq!(summary.payload["surface_kind"], "area_reset_repop_table");
    assert_eq!(summary.payload["schema"], RPG_AREA_RESET_REPOP_SCHEMA);
    assert_eq!(summary.payload["area_count"], table.entries.len());
    assert_eq!(
        summary.payload["min_empty_reset_area_minutes"],
        CF_EMPTY_RESET_AREA_MINUTES
    );

    // Spot-check one boss lair round-trips with its boss timer + trap re-arm.
    let lair = table
        .entries
        .iter()
        .find(|e| e.repop_policy == RepopPolicy::BossLair)
        .expect("corpus has at least one boss lair");
    let row = store
        .get_content_for_editor(
            EngineContentKind::Trigger,
            &format!("{AREA_RESET_REPOP_TABLE_ID}.{}", lair.area_id),
        )
        .unwrap();
    assert_eq!(row.payload["surface_kind"], "area_reset_repop");
    assert_eq!(row.payload["repop_policy"], "boss_lair");
    assert_eq!(row.payload["has_boss_timer"], true);
    assert_eq!(row.payload["rearms_traps"], true);
    assert_eq!(row.payload["capped_items_respawn_while_held"], false);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("{label}-reset-repop"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
