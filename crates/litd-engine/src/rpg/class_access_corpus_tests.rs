// Full-corpus referential-integrity suite for race `class_access` (#1631, gap 2).
// Mirrors the #1704 area-registry corpus pattern: it aggregates EVERY authored
// ClassSpec into a registry (single source of truth for valid class ids), then
// proves EVERY class id named in EVERY authored RaceSpec's `class_access`
// resolves to an authored class. Any future race that references an unauthored
// class id (a typo, an un-migrated placeholder, a deleted class) fails loud here.

use crate::rpg::{
    ClassRegistry, ClassSpec, RaceClassAccess, RaceSpec,
    validate_race_class_access_against_registry,
};

/// The complete authored ClassSpec corpus: all 17 classes (#1020 Warrior plus
/// the #1021-#1036 batch), assembled from the same fixtures the class-spec
/// tests use.
fn all_class_specs() -> Vec<ClassSpec> {
    vec![
        crate::rpg_class_specs_test_fixture::warrior_class_spec(),
        crate::rpg_class_specs_fixture_anti_paladin::anti_paladin_class_spec(),
        crate::rpg_class_specs_fixture_assassin::assassin_class_spec(),
        crate::rpg_class_specs_fixture_bard::bard_class_spec(),
        crate::rpg_class_specs_fixture_berserker::berserker_class_spec(),
        crate::rpg_class_specs_fixture_conjurer::conjurer_class_spec(),
        crate::rpg_class_specs_fixture_druid::druid_class_spec(),
        crate::rpg_class_specs_fixture_healer::healer_class_spec(),
        crate::rpg_class_specs_fixture_invoker::invoker_class_spec(),
        crate::rpg_class_specs_fixture_necromancer::necromancer_class_spec(),
        crate::rpg_class_specs_fixture_paladin::paladin_class_spec(),
        crate::rpg_class_specs_fixture_raider::raider_class_spec(),
        crate::rpg_class_specs_fixture_ranger::ranger_class_spec(),
        crate::rpg_class_specs_fixture_shaman::shaman_class_spec(),
        crate::rpg_class_specs_fixture_shapeshifter::shapeshifter_class_spec(),
        crate::rpg_class_specs_fixture_thief::thief_class_spec(),
        crate::rpg_class_specs_fixture_transmuter::transmuter_class_spec(),
    ]
}

/// The complete authored RaceSpec corpus: Human (#999), Elf (#1000), and the
/// #1001-#1019 batch (Half-Elf through Centaur).
fn all_race_specs() -> Vec<RaceSpec> {
    let mut specs = vec![
        crate::rpg_race_specs_test_fixture::human_race_spec(),
        crate::rpg_race_specs_test_fixture::elf_race_spec(),
    ];
    specs.extend(crate::rpg_race_specs_batch_1001_1018::all_batch_race_specs());
    specs
}

#[test]
fn issue1631_full_corpus_class_access_referential_integrity_holds() {
    let classes = all_class_specs();
    let races = all_race_specs();
    let registry = ClassRegistry::from_specs(&classes).expect("class registry builds");

    // Diagnostic readout (under --nocapture): the authored class roster and the
    // full validation counts, so the real corpus is visible even before asserts.
    for id in registry.ids() {
        eprintln!("AUTHORED_CLASS {id}");
    }

    let readback = validate_race_class_access_against_registry(&races, &registry)
        .unwrap_or_else(|e| panic!("corpus failed class-access referential integrity: {e}"));

    eprintln!(
        "issue1631 corpus: races={} classes={} checked_access_entries={} races_with_lists={}",
        readback.race_count,
        readback.class_count,
        readback.checked_class_access_entry_count,
        readback.races_with_class_lists,
    );

    // Exactly the 17 authored ClassSpecs form the registry.
    assert_eq!(readback.class_count, 17, "expected 17 authored classes");
    // Human (#999) + Elf (#1000) + the 19 #1001-#1019 races.
    assert_eq!(readback.race_count, 21, "expected 21 authored races");
    // Every race except the All-access Human carries a class list.
    assert_eq!(
        readback.races_with_class_lists, 20,
        "expected 20 races with Restricted/AllExcept lists"
    );
    // The corpus really validated a large body of cross-references, not zero.
    assert!(
        readback.checked_class_access_entry_count >= 100,
        "expected >=100 class-access entries validated, got {}",
        readback.checked_class_access_entry_count
    );
}

#[test]
fn issue1631_injected_unauthored_class_id_fails_loud() {
    // Edge: inject one bogus class id into an otherwise-clean corpus and prove
    // the registry fails closed with the exact structured diagnostic (race+id).
    let registry = ClassRegistry::from_specs(&all_class_specs()).expect("registry builds");
    let mut races = all_race_specs();
    let victim = races
        .iter_mut()
        .find(|r| matches!(r.class_access, RaceClassAccess::Restricted { .. }))
        .expect("a race with a Restricted list exists");
    let victim_race = victim.race_id.clone();
    victim.class_access = RaceClassAccess::Restricted {
        class_ids: vec!["class.__typo_nonexistent__".to_string()],
    };

    let error = validate_race_class_access_against_registry(&races, &registry)
        .expect_err("must fail on an unauthored class id");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    let message = error.to_string();
    assert!(
        message.contains("class.__typo_nonexistent__"),
        "diagnostic must name the unauthored class id, got: {message}"
    );
    assert!(
        message.contains(&victim_race),
        "diagnostic must name the offending race {victim_race}, got: {message}"
    );
}

#[test]
fn issue1631_duplicate_class_id_fails_loud() {
    // Edge: two ClassSpecs claiming the same class_id must be rejected (an
    // ambiguous registry would mask a real collision).
    let mut classes = all_class_specs();
    let clone = classes[0].clone();
    classes.push(clone);
    let error =
        ClassRegistry::from_specs(&classes).expect_err("duplicate class id must fail the registry");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    assert!(
        error.to_string().contains("duplicate class id"),
        "got: {error}"
    );
}

#[test]
fn issue1631_empty_corpus_is_vacuously_clean() {
    // Edge: no races -> no references -> trivially clean, zero counts.
    let registry = ClassRegistry::from_specs(&all_class_specs()).expect("registry builds");
    let readback =
        validate_race_class_access_against_registry(&[], &registry).expect("empty is clean");
    assert_eq!(readback.race_count, 0);
    assert_eq!(readback.checked_class_access_entry_count, 0);
    assert_eq!(readback.races_with_class_lists, 0);
}
