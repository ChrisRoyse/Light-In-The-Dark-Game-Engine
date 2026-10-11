use litd_engine::math::Transform;

use super::*;
use crate::history::EditorObject;

fn hero_object(props: &[(&str, &str)]) -> EditorObject {
    let mut object = EditorObject::new("hero.test", "Hero", None, Transform::IDENTITY);
    for (key, value) in props {
        object = object.with_property(*key, *value);
    }
    object
}

#[test]
fn build_defaults_when_absent() {
    let fields = build_hero_fields(&hero_object(&[])).expect("defaults must build");
    assert_eq!(fields.primary_attribute, "STR");
    assert!(fields.learnable.is_empty());
    assert!(fields.flags.is_empty());
    validate_hero_fields(&fields).expect("defaults must validate");
}

#[test]
fn build_full_override_set() {
    let object = hero_object(&[
        ("hero.primary_attribute", "INT"),
        ("hero.str_base", "18"),
        ("hero.str_gain_milli", "1800"),
        ("hero.int_base", "24"),
        ("hero.int_gain_milli", "3200"),
        ("hero.xp_curve_ref", "gameplay.xp.hero_default"),
        ("hero.revive_base_time_secs", "40"),
        ("hero.revive_base_cost_gold", "300"),
        ("hero.learnable", "ability.fireball@1,ability.blink@3"),
        ("hero.flags", "hero_glow,revivable"),
    ]);
    let fields = build_hero_fields(&object).expect("full set must build");
    assert_eq!(fields.primary_attribute, "INT");
    assert_eq!(fields.int_gain_milli, 3200);
    assert_eq!(fields.learnable.len(), 2);
    assert_eq!(fields.learnable[1].ability_id, "ability.blink");
    assert_eq!(fields.learnable[1].level_requirement, 3);
    validate_hero_fields(&fields).expect("full set must validate");
}

#[test]
fn malformed_learnable_slot_rejected() {
    let object = hero_object(&[("hero.learnable", "ability.fireball")]);
    let error = build_hero_fields(&object).expect_err("missing @level must fail");
    assert!(error.to_string().contains("ability_id"), "{error}");
}

#[test]
fn duplicate_learnable_rejected() {
    let object = hero_object(&[("hero.learnable", "ability.fireball@1,ability.fireball@4")]);
    let error = build_hero_fields(&object).expect_err("duplicate must fail");
    assert!(error.to_string().contains("duplicate"), "{error}");
}

#[test]
fn invalid_primary_attribute_rejected() {
    let object = hero_object(&[("hero.primary_attribute", "LUCK")]);
    let error = build_hero_fields(&object).expect_err("bad attr must fail");
    assert!(error.to_string().contains("LUCK"), "{error}");
}
