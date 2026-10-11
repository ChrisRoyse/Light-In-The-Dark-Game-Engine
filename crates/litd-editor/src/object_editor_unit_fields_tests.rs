//! #818/#829 lowering-time validation coverage for the WC3 unit field set: happy-path
//! extraction plus every enum/flag/range rejection branch of `build_wc3_fields`,
//! `validate_wc3_fields`, `check_enum`, and `check_flags`. These are the SECOND layer of the
//! reject-before-lowering contract (the FIRST is persist-time in `mcp_object_tools_validation`).

use litd_engine::math::Transform;

use super::*;
use crate::history::EditorObject;

/// Build a unit `EditorObject` from a flat property list.
fn unit(props: &[(&str, &str)]) -> EditorObject {
    let mut object = EditorObject::new("unit.test", "Unit", None, Transform::IDENTITY);
    for (key, value) in props {
        object = object.with_property(*key, *value);
    }
    object
}

/// A fully-specified, all-valid unit used as the happy-path fixture.
fn full_valid_unit() -> EditorObject {
    unit(&[
        ("stats.mana", "200"),
        ("stats.armor", "5"),
        ("art.icon", "icons/champion.blp"),
        ("art.scale_pct", "120"),
        ("sound.move_set", "ChampionMove"),
        ("sound.ready", "ChampionReady"),
        ("combat.damage_dice", "2"),
        ("combat.damage_sides", "6"),
        ("combat.damage_base", "34"),
        ("combat.attack_cooldown_ms", "1200"),
        ("combat.attack_range", "600"),
        ("combat.attack_type", "Magic"),
        ("combat.armor_type", "Hero"),
        ("combat.targets_allowed", "ground,air,structure"),
        ("movement.type", "Fly"),
        ("movement.height", "80"),
        ("movement.turn_rate_pct", "40"),
        ("techtree.requires", "tech.barracks,tech.keep"),
        ("techtree.trains", "unit.footman,unit.rifleman"),
        ("techtree.upgrades_to", "unit.champion.awakened"),
        ("economy.gold_cost", "245"),
        ("economy.lumber_cost", "0"),
        ("economy.food_cost", "3"),
        ("economy.build_time_secs", "20"),
    ])
}

#[test]
fn build_wc3_fields_happy_path_extracts_full_set() {
    let fields = build_wc3_fields(&full_valid_unit()).expect("valid unit must build");
    assert_eq!(fields.stats.mana, 200);
    assert_eq!(fields.stats.armor, 5);
    assert_eq!(fields.art.icon, "icons/champion.blp");
    assert_eq!(fields.art.scale_pct, 120);
    assert_eq!(fields.sound.ready, "ChampionReady");
    assert_eq!(fields.combat.damage_base, 34);
    assert_eq!(fields.combat.attack_range, 600);
    assert_eq!(fields.combat.attack_type, "Magic");
    assert_eq!(fields.combat.armor_type, "Hero");
    assert_eq!(
        fields.combat.targets_allowed,
        vec![
            "ground".to_string(),
            "air".to_string(),
            "structure".to_string()
        ]
    );
    assert_eq!(fields.movement.move_type, "Fly");
    assert_eq!(fields.movement.height, 80);
    assert_eq!(
        fields.techtree.trains,
        vec!["unit.footman".to_string(), "unit.rifleman".to_string()]
    );
    assert_eq!(fields.techtree.upgrades_to, "unit.champion.awakened");
    assert_eq!(fields.economy.gold_cost, 245);
    assert_eq!(fields.economy.food_cost, 3);
    // A field set built from valid data must also survive readback validation.
    validate_wc3_fields(&fields).expect("happy-path fields must validate");
}

#[test]
fn build_wc3_fields_uses_wc3_defaults_when_absent() {
    let fields = build_wc3_fields(&unit(&[])).expect("empty unit must build from defaults");
    assert_eq!(fields.combat.attack_type, "Normal");
    assert_eq!(fields.combat.armor_type, "Medium");
    assert_eq!(fields.combat.targets_allowed, vec!["ground".to_string()]);
    assert_eq!(fields.movement.move_type, "Foot");
    assert_eq!(fields.art.scale_pct, 100);
    assert_eq!(fields.combat.attack_cooldown_ms, 1_500);
    assert_eq!(fields.combat.attack_range, 90);
    assert_eq!(fields.movement.turn_rate_pct, 60);
    validate_wc3_fields(&fields).expect("default fields must validate");
}

#[test]
fn build_wc3_fields_rejects_each_invalid_enum_and_flag_class() {
    let cases: &[(&str, &str)] = &[
        ("combat.attack_type", "Bogus"),
        ("combat.armor_type", "Spaceship"),
        ("movement.type", "Teleport"),
        ("combat.targets_allowed", "ground,spaceship"),
        ("combat.targets_allowed", "ground,ground"),
    ];
    for (key, bad) in cases {
        let object = unit(&[(key, bad)]);
        assert!(
            build_wc3_fields(&object).is_err(),
            "build_wc3_fields must reject {key}={bad}"
        );
    }
}

#[test]
fn build_wc3_fields_rejects_out_of_range_numeric() {
    let object = unit(&[("stats.armor", "99999")]);
    assert!(build_wc3_fields(&object).is_err());
}

#[test]
fn validate_wc3_fields_rejects_bad_attack_type() {
    let mut fields = build_wc3_fields(&full_valid_unit()).unwrap();
    fields.combat.attack_type = "Bogus".to_string();
    assert!(validate_wc3_fields(&fields).is_err());
}

#[test]
fn validate_wc3_fields_rejects_bad_armor_type() {
    let mut fields = build_wc3_fields(&full_valid_unit()).unwrap();
    fields.combat.armor_type = "Spaceship".to_string();
    assert!(validate_wc3_fields(&fields).is_err());
}

#[test]
fn validate_wc3_fields_rejects_bad_move_type() {
    let mut fields = build_wc3_fields(&full_valid_unit()).unwrap();
    fields.movement.move_type = "Teleport".to_string();
    assert!(validate_wc3_fields(&fields).is_err());
}

#[test]
fn validate_wc3_fields_rejects_bad_target_flag() {
    let mut fields = build_wc3_fields(&full_valid_unit()).unwrap();
    fields.combat.targets_allowed = vec!["ground".to_string(), "spaceship".to_string()];
    assert!(validate_wc3_fields(&fields).is_err());
}

#[test]
fn validate_wc3_fields_rejects_duplicate_target_flag() {
    let mut fields = build_wc3_fields(&full_valid_unit()).unwrap();
    fields.combat.targets_allowed = vec!["ground".to_string(), "ground".to_string()];
    assert!(validate_wc3_fields(&fields).is_err());
}

#[test]
fn validate_wc3_fields_rejects_out_of_range_numeric() {
    let mut fields = build_wc3_fields(&full_valid_unit()).unwrap();
    fields.stats.armor = 99_999;
    assert!(validate_wc3_fields(&fields).is_err());
}

#[test]
fn check_enum_accepts_canonical_and_rejects_unknown() {
    assert!(check_enum("combat.attack_type", "Magic", WC3_ATTACK_TYPES).is_ok());
    assert!(check_enum("combat.armor_type", "Hero", WC3_ARMOR_TYPES).is_ok());
    assert!(check_enum("movement.type", "Fly", WC3_MOVE_TYPES).is_ok());
    let err = check_enum("combat.attack_type", "Bogus", WC3_ATTACK_TYPES)
        .unwrap_err()
        .to_string();
    assert!(err.contains("is not one of"), "unexpected error: {err}");
}

#[test]
fn check_flags_accepts_known_and_rejects_unknown_or_duplicate() {
    assert!(
        check_flags(
            "combat.targets_allowed",
            &["ground".to_string(), "air".to_string()]
        )
        .is_ok()
    );
    let unknown = check_flags("combat.targets_allowed", &["spaceship".to_string()])
        .unwrap_err()
        .to_string();
    assert!(
        unknown.contains("is not one of"),
        "unexpected error: {unknown}"
    );
    let duplicate = check_flags(
        "combat.targets_allowed",
        &["ground".to_string(), "ground".to_string()],
    )
    .unwrap_err()
    .to_string();
    assert!(
        duplicate.contains("duplicate flag"),
        "unexpected error: {duplicate}"
    );
}
