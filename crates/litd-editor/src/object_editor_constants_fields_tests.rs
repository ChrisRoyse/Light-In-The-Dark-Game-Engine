use litd_engine::math::Transform;

use super::*;
use crate::history::EditorObject;

fn base_object() -> EditorObject {
    let mut object = EditorObject::new(
        "gameplay.constants",
        "GameplayConstant",
        None,
        Transform::IDENTITY,
    )
    .with_property("object.rawcode", "GC01")
    .with_property("object.display_name", "Gameplay Constants");
    for (key, value) in gameplay_constants_base_properties() {
        object = object.with_property(key, value);
    }
    object
}

#[test]
fn zero_override_copy_resolves_full_base_set() {
    // An object with NO gameplay fields at all still resolves to the shipped base defaults.
    let sparse = EditorObject::new("gc", "GameplayConstant", None, Transform::IDENTITY);
    let constants = build_gameplay_constants(&sparse).unwrap();
    assert_eq!(constants.day_length_secs, 480);
    assert_eq!(constants.xp_level_thresholds.first().copied(), Some(0));
    assert_eq!(constants.float_encoding, CONSTANTS_FLOAT_ENCODING);
    // 25.0 STR->HP round-trips losslessly through the IEEE-754 bit pattern.
    assert_eq!(f64::from_bits(constants.str_to_hp_bits), 25.0);
}

#[test]
fn override_only_changes_named_field() {
    let object = base_object().with_property("cycle.day_length_secs", "600");
    let constants = build_gameplay_constants(&object).unwrap();
    assert_eq!(constants.day_length_secs, 600);
    assert_eq!(constants.night_length_secs, 180);
}

#[test]
fn non_monotonic_thresholds_are_rejected() {
    let object = base_object().with_property("xp.level_thresholds", "0,200,150");
    assert!(build_gameplay_constants(&object).is_err());
}

#[test]
fn zero_cycle_length_is_rejected() {
    let object = base_object().with_property("cycle.night_length_secs", "0");
    assert!(build_gameplay_constants(&object).is_err());
}

#[test]
fn unordered_upkeep_caps_are_rejected() {
    let object = base_object().with_property("upkeep.low_upkeep_food_cap", "40");
    // no=50 > low=40 breaks the non-decreasing invariant.
    assert!(build_gameplay_constants(&object).is_err());
}

#[test]
fn out_of_range_conversion_rate_is_rejected() {
    let object = base_object().with_property("hero.str_to_hp", "999999.0");
    assert!(build_gameplay_constants(&object).is_err());
}

#[test]
fn base_set_validates_and_fingerprint_inputs_are_stable() {
    let constants = build_gameplay_constants(&base_object()).unwrap();
    validate_gameplay_constants(&constants).unwrap();
    assert_eq!(constants.max_hero_level, 10);
}
