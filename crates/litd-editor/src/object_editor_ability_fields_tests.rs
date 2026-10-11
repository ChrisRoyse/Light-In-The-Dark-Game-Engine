use litd_engine::math::Transform;

use super::*;
use crate::history::EditorObject;

fn ability_object(props: &[(&str, &str)]) -> EditorObject {
    let mut object = EditorObject::new("ability.test", "Ability", None, Transform::IDENTITY);
    for (key, value) in props {
        object = object.with_property(*key, *value);
    }
    object
}

#[test]
fn build_defaults_when_absent() {
    let fields = build_ability_fields(&ability_object(&[])).expect("defaults must build");
    assert_eq!(fields.kind, "passive");
    assert_eq!(fields.targeting, "none");
    assert_eq!(fields.levels, 1);
    assert!(fields.targets_allowed.is_empty());
    assert!(fields.scaling.base_damage.is_empty());
    validate_ability_fields(&fields).expect("defaults must validate");
}

#[test]
fn build_full_override_set() {
    let object = ability_object(&[
        ("ability.kind", "active"),
        ("ability.mana_cost", "75"),
        ("ability.cooldown_ms", "8000"),
        ("ability.cast_range", "600"),
        ("ability.targeting", "unit"),
        ("ability.targets_allowed", "ground,air"),
        ("ability.duration_ms", "5000"),
        ("ability.area_of_effect", "250"),
        ("ability.levels", "3"),
        ("ability.scaling.base_damage", "50,100,150"),
        ("ability.effects", "effect.fire,effect.burn"),
        ("ability.icon", "icons/fire.blp"),
        ("ability.requirements", "tech.arcane"),
    ]);
    let fields = build_ability_fields(&object).expect("full set must build");
    assert_eq!(fields.kind, "active");
    assert_eq!(fields.mana_cost, 75);
    assert_eq!(fields.targets_allowed, vec!["ground", "air"]);
    assert_eq!(fields.scaling.base_damage, vec![50, 100, 150]);
    validate_ability_fields(&fields).expect("full set must validate");
}

#[test]
fn scaling_length_must_match_levels() {
    let object = ability_object(&[
        ("ability.levels", "3"),
        ("ability.scaling.base_damage", "50,100"),
    ]);
    let error = build_ability_fields(&object).expect_err("mismatched scaling must fail");
    assert!(error.to_string().contains("scaling"), "{error}");
}

#[test]
fn invalid_kind_rejected() {
    let object = ability_object(&[("ability.kind", "bogus")]);
    let error = build_ability_fields(&object).expect_err("bad kind must fail");
    assert!(error.to_string().contains("bogus"), "{error}");
}

#[test]
fn invalid_target_flag_rejected() {
    let object = ability_object(&[("ability.targets_allowed", "ground,flying_carpet")]);
    let error = build_ability_fields(&object).expect_err("bad flag must fail");
    assert!(error.to_string().contains("flying_carpet"), "{error}");
}
