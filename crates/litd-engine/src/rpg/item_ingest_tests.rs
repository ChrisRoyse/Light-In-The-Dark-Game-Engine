//! #1688 itemsearch ingestion FSV: the pipeline is PROVEN on the real captured
//! itemsearch tranche (247 records), with count parity through Calyx readback,
//! itemized loud failures on real un-ingestable records, spot-audits of real
//! items field-for-field, and synthetic loud-fail edges.

use crate::{
    calyx_content::EngineCalyxContentStore,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{ingest_itemsearch_dump, parse_itemsearch_dump},
};

/// The real itemsearch dump captured 2026-07-12 (`carrionfields.net/itemsearch`),
/// 247 complete records across weapon/armor/clothing/treasure/instrument.
const SEED: &str = include_str!("itemsearch_seed_2026_07_12.txt");

#[test]
fn issue1688_real_tranche_accounts_for_every_record() {
    let parse = parse_itemsearch_dump(SEED);
    // The dump has 247 complete records (grep '^Name:' == 247).
    assert_eq!(parse.total_records, 247, "record count changed");
    // Anti-silent-drop invariant: every record is a spec OR an itemized failure.
    assert!(parse.accounts_for_every_record());
    assert_eq!(parse.specs.len() + parse.failures.len(), 247);
    // 6 real records cannot be ingested from itemsearch alone: 3 racial-slot
    // items (forepaws/wings need anatomy-race gating) + 3 items whose only flag
    // is not yet in the CF flag vocabulary (chaos/orderly_only/transparent).
    assert_eq!(
        parse.failures.len(),
        6,
        "unexpected failure set: {:?}",
        parse.failures
    );
    assert_eq!(parse.specs.len(), 241);

    // Failures are itemized with a real reason and the offending item name.
    for failure in &parse.failures {
        assert!(!failure.item_name.is_empty());
        assert!(!failure.reason.is_empty());
        assert!(failure.record_index >= 1 && failure.record_index <= 247);
    }
    let reasons: Vec<&str> = parse.failures.iter().map(|f| f.reason.as_str()).collect();
    assert!(
        reasons.iter().any(|r| r.contains("racial slot")),
        "expected racial-slot failure, got {reasons:?}"
    );
    assert!(
        reasons.iter().any(|r| r.contains("unknown item flag")),
        "expected unknown-flag failure, got {reasons:?}"
    );
    // Named specific real failing records (from the seed).
    let names: Vec<&str> = parse
        .failures
        .iter()
        .map(|f| f.item_name.as_str())
        .collect();
    assert!(
        names.iter().any(|n| n.contains("titanium wing braces")),
        "expected the wing-slot item to fail loud, got {names:?}"
    );
}

#[test]
fn issue1688_pipeline_round_trips_tranche_with_count_parity() {
    let (store, _vault) = content_store("ingest-tranche");
    let readback = ingest_itemsearch_dump(&store, SEED).unwrap();

    // Count parity is the FSV: parsed == persisted == readback, and every
    // record is accounted for (parsed + failed == total).
    assert_eq!(readback.total_records, 247);
    assert_eq!(readback.parsed, 241);
    assert_eq!(readback.failed, 6);
    assert_eq!(readback.parsed + readback.failed, readback.total_records);
    assert_eq!(readback.persisted_rows, 241);
    assert_eq!(readback.readback_rows, 241);
    assert_eq!(readback.persisted_rows, readback.parsed);
    assert_eq!(readback.readback_rows, readback.parsed);
    assert_eq!(readback.failures.len(), 6);
    assert_eq!(readback.package_fingerprint.len(), 64);
}

#[test]
fn issue1688_spot_audit_real_armor_item_field_for_field() {
    let parse = parse_itemsearch_dump(SEED);
    // "a pair of blackened leather gloves" (Pine Forest): armor/hands/leather,
    // L51, 1 lb 10 oz, giant_only, AC 13/7/13/3/14, +10 ac,+2 dmg,-5 breath.
    let spec = parse
        .specs
        .iter()
        .find(|s| s.display_name == "a pair of blackened leather gloves")
        .expect("gloves present in tranche");
    assert_eq!(spec.source_area_name, "Pine Forest");
    assert_eq!(spec.source_area_id, "area.cf.pine_forest");
    assert_eq!(spec.wear.slot.as_id(), "hands");
    assert_eq!(spec.material, "leather");
    assert_eq!(spec.level, 51);
    assert_eq!(spec.weight.pounds, 1);
    assert_eq!(spec.weight.ounces, 10);
    assert_eq!(spec.flags, vec!["giant_only".to_string()]);
    let ac = spec.armor.as_ref().expect("armor");
    assert_eq!(
        (ac.pierce, ac.bash, ac.slash, ac.magic, ac.element),
        (13, 7, 13, 3, 14)
    );
    assert_eq!(spec.affects.len(), 3);
    assert!(spec.weapon.is_none());
}

#[test]
fn issue1688_spot_audit_real_weapon_reconstructs_average() {
    let parse = parse_itemsearch_dump(SEED);
    // Any weapon: itemsearch publishes only the average + noun; 1d(2*avg-1)
    // reproduces the average exactly, and the noun resolves via helpfile 2304.
    let weapon_spec = parse
        .specs
        .iter()
        .find(|s| s.weapon.is_some())
        .expect("at least one weapon in tranche");
    let weapon = weapon_spec.weapon.as_ref().unwrap();
    assert_eq!(weapon.dice.count, 1);
    // sides = 2*avg - 1 is odd; average_tenths == 10*avg reproduces the average.
    assert_eq!(weapon.dice.sides % 2, 1);
    // Mean of a uniform 1d(sides) roll is (sides+1)/2; sides is odd (asserted
    // above) so this is exact. This is a mean, not a ceiling division, so
    // clippy's `div_ceil` rewrite would misstate the intent.
    #[allow(clippy::manual_div_ceil)]
    let avg = (weapon.dice.sides + 1) / 2;
    assert_eq!(weapon.dice.average_tenths(), (avg as u32) * 10);
    assert!(!weapon.damage_noun.is_empty());
    assert_eq!(weapon_spec.wear.slot.as_id(), "wield");
    assert!(weapon_spec.armor.is_none());

    // The tranche carries real two-handed weapons (itemsearch "<class>
    // (two-handed)"); the parser records the two-handed property.
    let two_handed = parse
        .specs
        .iter()
        .filter(|s| s.weapon.as_ref().is_some_and(|w| w.two_handed))
        .count();
    assert_eq!(
        two_handed, 16,
        "expected 16 two-handed weapons in the tranche"
    );
}

#[test]
fn issue1688_kind_coverage_spans_the_browsable_set() {
    let parse = parse_itemsearch_dump(SEED);
    let mut kinds = std::collections::BTreeSet::new();
    for spec in &parse.specs {
        kinds.insert(spec.kind_payload.kind().as_id());
    }
    // The itemsearch browsable set is weapon/armor/clothing/treasure/instrument.
    for expected in ["weapon", "armor", "clothing", "treasure", "instrument"] {
        assert!(kinds.contains(expected), "missing kind {expected}");
    }
}

#[test]
fn issue1688_unparseable_records_fail_loud_and_itemized() {
    // Each malformed record is itemized (no silent drop): total == parsed+failed.
    let dump = "\
Name: a mystery trinket Area: Nowhere Item Type: treasure Wear: neck Material: unobtanium Level: 10 Weight: 0 lb 2 oz

Name: a broken record Area: Nowhere Item Type: armor Wear: body Material: iron Level: 10

Name: a cursed charm Area: Nowhere Item Type: treasure Wear: neck Material: gold Level: 10 Weight: 0 lb 2 oz Flags: madeupflag
- Modifies mana by 5

Name: a fine ring Area: Nowhere Item Type: treasure Wear: finger Material: gold Level: 10 Weight: 0 lb 1 oz
- Modifies wisdom by 1
";
    let parse = parse_itemsearch_dump(dump);
    assert_eq!(parse.total_records, 4);
    assert!(parse.accounts_for_every_record());
    // 1 valid (the fine ring), 3 itemized failures.
    assert_eq!(parse.specs.len(), 1);
    assert_eq!(parse.failures.len(), 3);

    let by_name = |needle: &str| {
        parse
            .failures
            .iter()
            .find(|f| f.item_name.contains(needle))
            .unwrap_or_else(|| panic!("no failure for {needle}"))
    };
    assert!(
        by_name("mystery trinket")
            .reason
            .contains("unknown material")
    );
    assert!(by_name("broken record").reason.contains("Weight"));
    assert!(by_name("cursed charm").reason.contains("unknown item flag"));
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1688-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
