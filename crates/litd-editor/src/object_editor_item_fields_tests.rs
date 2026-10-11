use litd_engine::math::Transform;

use super::*;
use crate::history::EditorObject;

fn item_object(props: &[(&str, &str)]) -> EditorObject {
    let mut object = EditorObject::new("item.test", "Item", None, Transform::IDENTITY);
    for (key, value) in props {
        object = object.with_property(*key, *value);
    }
    object
}

#[test]
fn defaults_apply_when_fields_absent() {
    let fields = build_item_fields(&item_object(&[])).unwrap();
    assert_eq!(fields.classification.class, "Purchasable");
    assert!(fields.classification.inventory_flags.is_empty());
    assert_eq!(fields.stock.stock_max, 1);
    assert_eq!(fields.charges.initial, 0);
    assert_eq!(fields.pickup.drop_on_death, "drop");
}

#[test]
fn full_field_set_extracts() {
    let fields = build_item_fields(&item_object(&[
        ("item.class", "Charged"),
        ("item.inventory_flags", "droppable,perishable"),
        ("economy.gold_cost", "150"),
        ("economy.lumber_cost", "20"),
        ("stock.stock_max", "3"),
        ("stock.replenish_interval_secs", "45"),
        ("charges.initial", "2"),
        ("charges.max", "5"),
        ("bonus.attack", "12"),
        ("bonus.armor", "3"),
        ("bonus.strength", "4"),
        ("ability.active", "ability.blink,ability.heal"),
        ("ability.passive", "ability.regen"),
        ("pickup.range", "200"),
        ("pickup.drop_on_death", "destroy"),
        ("art.icon", "icons/potion.blp"),
    ]))
    .unwrap();
    assert_eq!(fields.classification.class, "Charged");
    assert_eq!(
        fields.classification.inventory_flags,
        vec!["droppable", "perishable"]
    );
    assert_eq!(fields.economy.gold_cost, 150);
    assert_eq!(fields.charges.initial, 2);
    assert_eq!(fields.charges.max, 5);
    assert_eq!(
        fields.abilities.active,
        vec!["ability.blink", "ability.heal"]
    );
    assert_eq!(fields.pickup.drop_on_death, "destroy");
    validate_item_fields(&fields).unwrap();
}

#[test]
fn invalid_class_rejected() {
    let error = build_item_fields(&item_object(&[("item.class", "Legendary")]));
    assert!(error.is_err());
}

#[test]
fn invalid_inventory_flag_rejected() {
    let error = build_item_fields(&item_object(&[(
        "item.inventory_flags",
        "droppable,cursed",
    )]));
    assert!(error.is_err());
}

#[test]
fn charges_initial_exceeding_max_rejected() {
    let error = build_item_fields(&item_object(&[
        ("charges.initial", "6"),
        ("charges.max", "3"),
    ]));
    assert!(error.is_err());
}

#[test]
fn out_of_range_bonus_rejected() {
    let error = build_item_fields(&item_object(&[("bonus.armor", "99999")]));
    assert!(error.is_err());
}

#[test]
fn duplicate_ability_ref_rejected() {
    let error = build_item_fields(&item_object(&[("ability.active", "ability.x,ability.x")]));
    assert!(error.is_err());
}
