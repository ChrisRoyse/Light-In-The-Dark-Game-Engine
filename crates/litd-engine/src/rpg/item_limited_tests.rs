//! #1686 limited-item tests: the CF rarity mechanics (helpfile 61) as a
//! validated data model + readback, Calyx persist + readback per pilot, and the
//! loud-fail edges (limited-count zero/negative, container-rule violation, an
//! offline-keep floor of zero, and disabling the crumble / illusion valves).

use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        CF_LIMITED_KEEP_LEVEL_FLOOR, limited_item_content_records, limited_item_readback,
        persist_and_lower_rpg_authored_content, validate_limited_item_spec,
        validate_rpg_authored_content_records,
    },
    rpg_item_limited_test_fixture::{
        RIMEBOUND_WARBLADE_LIMITED_ID, SUNDERED_CROWN_LIMITED_ID, rimebound_warblade_limited_spec,
        sundered_crown_limited_spec,
    },
};

#[test]
fn issue1686_pilots_match_helpfile_61_mechanics() {
    for spec in [
        sundered_crown_limited_spec(),
        rimebound_warblade_limited_spec(),
    ] {
        let readback = limited_item_readback(&spec).unwrap();
        // No-container rule: limited items are too powerful to be bagged.
        assert!(!readback.may_enter_container);
        // L11 offline-keep gate (CF canon value).
        assert_eq!(readback.keep_level_floor, CF_LIMITED_KEEP_LEVEL_FLOOR);
        assert_eq!(readback.keep_level_floor, 11);
        // Overspread crumble + illusionary display are intrinsic.
        assert!(readback.crumble_on_overspread);
        assert!(readback.illusion_over_limit);
        // World-capped and hoarding is an enforced violation.
        assert!(readback.world_cap >= 1);
        assert!(readback.hoarding_is_violation);
        // The runtime-scope enforcement is documented, not silently dropped.
        assert!(readback.runtime_note_count >= 1);
        assert_eq!(readback.limited_item_sha256.len(), 64);
    }
}

#[test]
fn issue1686_pilots_persist_and_read_back_from_calyx_store() {
    for (spec, id) in [
        (sundered_crown_limited_spec(), SUNDERED_CROWN_LIMITED_ID),
        (
            rimebound_warblade_limited_spec(),
            RIMEBOUND_WARBLADE_LIMITED_ID,
        ),
    ] {
        let records = limited_item_content_records(&spec).unwrap();
        let validation = validate_rpg_authored_content_records(&records).unwrap();
        let (store, _vault) = content_store(id);
        let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
        let row = store
            .get_content_for_editor(EngineContentKind::Item, id)
            .unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(validation.record_count, 1);
        assert!(validation.all_editor_authored_sources);
        assert_eq!(row.kind, EngineContentKind::Item);
        assert_eq!(row.id, id);
        assert_eq!(row.payload["surface_kind"], "limited_item_spec");
        assert_eq!(row.payload["may_enter_container"], false);
        assert_eq!(row.payload["keep_level_floor"], 11);
        assert_eq!(row.payload["crumble"]["crumble_on_overspread"], true);
        assert_eq!(row.payload["illusion"]["illusion_over_limit"], true);
        assert!(
            row.source_path
                .starts_with("editor://rpg/ashen_reach/limited_items/")
        );
        assert_eq!(lowered.row_count, 1);
        assert_eq!(lowered.entities[0].surface_kind, "limited_item_spec");
    }
}

#[test]
fn issue1686_limited_count_zero_or_negative_fails_loud() {
    // FSV edge: a limited count of zero.
    let mut zero = sundered_crown_limited_spec();
    zero.world_cap = 0;
    assert_code(
        validate_limited_item_spec(&zero),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    // FSV edge: a negative ledger-delta count.
    let mut negative = sundered_crown_limited_spec();
    negative.world_cap = -5;
    assert_code(
        validate_limited_item_spec(&negative),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1686_container_rule_violation_fails_loud() {
    // FSV edge: a limited item authored as container-allowed contradicts CF.
    let mut baggable = rimebound_warblade_limited_spec();
    baggable.may_enter_container = true;
    let error = validate_limited_item_spec(&baggable).unwrap_err();
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    assert!(error.to_string().contains("container"));
}

#[test]
fn issue1686_keep_floor_and_valves_must_hold() {
    // A zero offline-keep floor is meaningless.
    let mut no_floor = sundered_crown_limited_spec();
    no_floor.keep_level_floor = 0;
    assert_code(
        validate_limited_item_spec(&no_floor),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    // Disabling the anti-hoard crumble valve contradicts helpfile 61.
    let mut no_crumble = sundered_crown_limited_spec();
    no_crumble.crumble.crumble_on_overspread = false;
    assert_code(
        validate_limited_item_spec(&no_crumble),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    // Disabling the illusionary display contradicts helpfile 61.
    let mut no_illusion = sundered_crown_limited_spec();
    no_illusion.illusion.illusion_over_limit = false;
    assert_code(
        validate_limited_item_spec(&no_illusion),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let safe: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1686-{safe}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
