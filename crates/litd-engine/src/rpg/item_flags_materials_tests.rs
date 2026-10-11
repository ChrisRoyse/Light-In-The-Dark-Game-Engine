//! #1685 item flags & materials tests: the CF flag vocabulary with typed zap/
//! gate semantics, the material vocabulary, the material-physics interaction
//! table (iron burns elves) wired to the SAME #1631 race-affinity ids with
//! referential integrity both ways, Calyx persist + readback, and loud-fail
//! edges (unknown flag, unknown material, material without a race-side affinity,
//! contradictory gate combinations).

use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AuraKind, BioRequirement, CurseEffect, DurabilityRule, EmissionChannel, FlagSemantic,
        ItemFlag, ItemMaterial, MATERIAL_PHYSICS_AFFINITIES, RaceAffinity, ZapAlignment,
        item_spec_content_records, material_race_interaction,
        persist_and_lower_rpg_authored_content, resolve_material_affinity, stealth_signature,
        validate_item_flags, validate_item_material, validate_item_spec,
    },
    rpg_item_specs_test_fixture::steel_maul_weapon_spec,
    rpg_race_specs_batch_1001_1018::all_batch_race_specs,
};

fn race(race_id: &str) -> crate::rpg::RaceSpec {
    all_batch_race_specs()
        .into_iter()
        .find(|r| r.race_id == race_id)
        .unwrap_or_else(|| panic!("race {race_id} not in the authored corpus"))
}

// -- Flag vocabulary + typed semantics ----------------------------------------

#[test]
fn issue1685_flag_ids_round_trip_and_reject_bogus() {
    for flag in ItemFlag::ALL {
        assert_eq!(ItemFlag::from_id(flag.as_id()), Some(*flag));
    }
    // FSV edge: an unknown flag does not resolve.
    assert_eq!(ItemFlag::from_id("cursed_of_doom"), None);
    assert_eq!(ItemFlag::from_id(""), None);
}

#[test]
fn issue1685_flag_semantics_are_typed_not_prose() {
    // Anti-alignment ZAP rules.
    assert_eq!(
        ItemFlag::AntiGood.semantic(),
        FlagSemantic::AlignmentZap(ZapAlignment::Good)
    );
    assert_eq!(
        ItemFlag::AntiEvil.semantic(),
        FlagSemantic::AlignmentZap(ZapAlignment::Evil)
    );
    assert_eq!(
        ItemFlag::AntiNeutral.semantic(),
        FlagSemantic::AlignmentZap(ZapAlignment::Neutral)
    );
    // Stealth-signature emission (a humming sword gives you away).
    assert_eq!(
        ItemFlag::Humming.semantic(),
        FlagSemantic::SignatureEmission(EmissionChannel::Sound)
    );
    assert_eq!(
        ItemFlag::Glowing.semantic(),
        FlagSemantic::SignatureEmission(EmissionChannel::Light)
    );
    // Bio gates.
    assert_eq!(
        ItemFlag::GiantOnly.semantic(),
        FlagSemantic::BioGate(BioRequirement::GiantKind)
    );
    assert_eq!(
        ItemFlag::FemaleOnly.semantic(),
        FlagSemantic::BioGate(BioRequirement::Female)
    );
    // Curse + durability.
    assert_eq!(
        ItemFlag::NoRemove.semantic(),
        FlagSemantic::Curse(CurseEffect::BindOnEquip)
    );
    assert_eq!(
        ItemFlag::RotDeath.semantic(),
        FlagSemantic::Durability(DurabilityRule::CrumbleOnDeath)
    );
    assert_eq!(
        ItemFlag::Magic.semantic(),
        FlagSemantic::Aura(AuraKind::Magical)
    );
}

#[test]
fn issue1685_stealth_signature_reads_the_emission_flags() {
    // A glowing, humming item leaks light AND sound; the stealth meta reads data.
    let resolved =
        validate_item_flags("flags", &["glowing".to_string(), "humming".to_string()]).unwrap();
    let sig = stealth_signature(&resolved);
    assert!(sig.emits_light);
    assert!(sig.emits_sound);
    assert!(!sig.emits_darkness);
    assert!(sig.is_noisy());

    // A silent, non-glowing anti_good weapon leaks nothing (stealth-safe gear).
    let quiet = validate_item_flags("flags", &["anti_good".to_string()]).unwrap();
    assert!(!stealth_signature(&quiet).is_noisy());
}

#[test]
fn issue1685_unknown_flag_and_contradictions_fail_loud() {
    // FSV edge: unknown flag.
    assert_code(
        validate_item_flags("flags", &["ubercursed".to_string()]).map(|_| ()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    // Duplicate.
    assert_code(
        validate_item_flags("flags", &["magic".to_string(), "magic".to_string()]).map(|_| ()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    // Contradiction: male + female gate leaves nobody able to wear it.
    assert_code(
        validate_item_flags(
            "flags",
            &["male_only".to_string(), "female_only".to_string()],
        )
        .map(|_| ()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
    // Contradiction: all three alignment zaps -> unusable by anyone.
    assert_code(
        validate_item_flags(
            "flags",
            &[
                "anti_good".to_string(),
                "anti_neutral".to_string(),
                "anti_evil".to_string(),
            ],
        )
        .map(|_| ()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

// -- Material vocabulary + physics --------------------------------------------

#[test]
fn issue1685_material_vocabulary_is_the_full_itemsearch_set() {
    // The CF itemsearch Material filter lists 72 materials.
    assert_eq!(ItemMaterial::ALL.len(), 72);
    for material in ItemMaterial::ALL {
        assert_eq!(ItemMaterial::from_id(material.as_id()), Some(*material));
    }
    assert!(validate_item_material("material", "steel").is_ok());
    assert!(validate_item_material("material", "mithril").is_ok());
    // FSV edge: unknown material fails loud.
    assert_code(
        validate_item_material("material", "unobtanium"),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1685_material_affinity_is_the_same_1631_vocabulary_both_ways() {
    // Forward: every physical material resolves to a real RaceAffinity id.
    for material in ItemMaterial::ALL {
        if let Some(id) = material.physical_affinity_id() {
            let affinity = material.physical_affinity().unwrap().unwrap();
            assert_eq!(affinity.as_id(), id);
            assert!(MATERIAL_PHYSICS_AFFINITIES.contains(&affinity));
        }
    }
    // Reverse: every declared physics affinity is produced by some material
    // (bijection -> single shared vocabulary, referential integrity both ways).
    for affinity in MATERIAL_PHYSICS_AFFINITIES {
        assert!(
            ItemMaterial::ALL
                .iter()
                .any(|m| m.physical_affinity_id() == Some(affinity.as_id())),
            "no material produces affinity {}",
            affinity.as_id()
        );
    }
    // The physics affinities are exactly iron/silver/mithril/whitesteel/wood.
    assert_eq!(
        MATERIAL_PHYSICS_AFFINITIES,
        [
            RaceAffinity::Iron,
            RaceAffinity::Silver,
            RaceAffinity::Mithril,
            RaceAffinity::Whitesteel,
            RaceAffinity::Wood,
        ]
    );
}

#[test]
fn issue1685_iron_burns_the_iron_vulnerable_race() {
    // The half-elf is authored 'Vulnerable: Iron' (#1631). An iron item burns it.
    let half_elf = race("race.half_elf");
    let interaction = material_race_interaction(ItemMaterial::Iron, &half_elf)
        .unwrap()
        .expect("iron is a physical material");
    assert_eq!(interaction.material_id, "iron");
    assert_eq!(interaction.affinity_id, "iron");
    assert_eq!(interaction.race_id, "race.half_elf");
    assert!(interaction.burns);
    assert!(!interaction.warded);
    assert_eq!(interaction.severity, 25);

    // The drow-line (ashkin) is 'Vulnerable: Mithril'; mithril burns them.
    let ashkin = race("race.ashkin_mixed");
    let mithril = material_race_interaction(ItemMaterial::Mithril, &ashkin)
        .unwrap()
        .unwrap();
    assert!(mithril.burns);
    // ...but plain iron does NOT burn the drow-line (no iron vulnerability).
    let iron_on_ashkin = material_race_interaction(ItemMaterial::Iron, &ashkin)
        .unwrap()
        .unwrap();
    assert!(!iron_on_ashkin.burns);
    assert!(!iron_on_ashkin.warded);
    assert_eq!(iron_on_ashkin.severity, 0);
}

#[test]
fn issue1685_inert_material_has_no_physics_and_bad_affinity_fails_loud() {
    // FSV edge: an inert material (leather) has no race-side affinity -> None.
    let half_elf = race("race.half_elf");
    assert!(
        material_race_interaction(ItemMaterial::Leather, &half_elf)
            .unwrap()
            .is_none()
    );
    assert!(ItemMaterial::Leather.physical_affinity().unwrap().is_none());

    // FSV edge: an affinity id with NO race-side vocabulary entry fails loud.
    assert_code(
        resolve_material_affinity("material.physical_affinity", "adamantite").map(|_| ()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

// -- Persist + readback through Calyx -----------------------------------------

#[test]
fn issue1685_flagged_material_item_persists_and_reads_back() {
    // Author an iron, glowing, anti_good weapon and round-trip it through Calyx.
    let mut spec = steel_maul_weapon_spec();
    spec.material = "iron".to_string();
    spec.flags = vec!["glowing".to_string(), "anti_good".to_string()];
    // Sanity: it validates through the wired flag/material checks.
    validate_item_spec(&spec).unwrap();

    let records = item_spec_content_records(&spec).unwrap();
    let (store, _vault) = content_store("flags-materials");
    persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let row = store
        .get_content_for_editor(EngineContentKind::Item, &spec.item_id)
        .unwrap();
    assert_eq!(row.payload["material"], "iron");
    assert_eq!(row.payload["flags"][0], "glowing");
    assert_eq!(row.payload["flags"][1], "anti_good");
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1685-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
