use std::collections::{BTreeMap, BTreeSet};

use litd_engine::math::Transform;

use super::*;
use crate::history::EditorObject;

fn object(id: &str, kind: &str, props: &[(&str, &str)]) -> EditorObject {
    let mut object = EditorObject::new(id, kind, None, Transform::IDENTITY);
    for (key, value) in props {
        object = object.with_property(*key, *value);
    }
    object
}

fn item_ids(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn recipe_builds_and_cross_validates() {
    let recipe = build_commerce_record(&object(
        "recipe.sword",
        "Recipe",
        &[
            ("recipe.components", "item.hilt,item.blade"),
            ("recipe.result", "item.sword"),
        ],
    ))
    .unwrap();
    let ids = item_ids(&["item.hilt", "item.blade", "item.sword"]);
    let resolved = validate_commerce_cross_record(&recipe, &ids, &BTreeMap::new()).unwrap();
    assert_eq!(resolved, 3);
}

#[test]
fn recipe_dangling_ref_fails_loud() {
    let recipe = build_commerce_record(&object(
        "recipe.sword",
        "Recipe",
        &[
            ("recipe.components", "item.hilt,item.missing"),
            ("recipe.result", "item.sword"),
        ],
    ))
    .unwrap();
    let ids = item_ids(&["item.hilt", "item.sword"]);
    let error = validate_commerce_cross_record(&recipe, &ids, &BTreeMap::new()).unwrap_err();
    assert!(error.to_string().contains("dangling_item_ref"));
}

#[test]
fn recipe_self_reference_fails_loud() {
    let recipe = build_commerce_record(&object(
        "recipe.loop",
        "Recipe",
        &[
            ("recipe.components", "item.sword"),
            ("recipe.result", "item.sword"),
        ],
    ))
    .unwrap();
    let ids = item_ids(&["item.sword"]);
    let error = validate_commerce_cross_record(&recipe, &ids, &BTreeMap::new()).unwrap_err();
    assert!(error.to_string().contains("self_reference"));
}

#[test]
fn recipe_cycle_fails_loud() {
    // item.a <- item.b and item.b <- item.a form a cycle.
    let mut graph = BTreeMap::new();
    graph.insert("item.a".to_string(), vec!["item.b".to_string()]);
    graph.insert("item.b".to_string(), vec!["item.a".to_string()]);
    let recipe = CommerceRecord::Recipe(RecipeRecord {
        components: vec!["item.b".to_string()],
        result: "item.a".to_string(),
    });
    let ids = item_ids(&["item.a", "item.b"]);
    let error = validate_commerce_cross_record(&recipe, &ids, &graph).unwrap_err();
    assert!(error.to_string().contains("cycle"));
}

#[test]
fn shop_builds_and_validates() {
    let shop = build_commerce_record(&object(
        "shop.arms",
        "Shop",
        &[
            ("shop.stock", "item.sword,item.shield"),
            ("shop.stock_max", "5"),
            ("shop.buy_enabled", "true"),
            ("shop.sell_enabled", "true"),
        ],
    ))
    .unwrap();
    let ids = item_ids(&["item.sword", "item.shield"]);
    let resolved = validate_commerce_cross_record(&shop, &ids, &BTreeMap::new()).unwrap();
    assert_eq!(resolved, 2);
    validate_commerce_shape(&shop).unwrap();
}

#[test]
fn drop_table_parses_weights_and_validates() {
    let table = build_commerce_record(&object(
        "drop.creep",
        "DropTable",
        &[("drop.entries", "item.gold:80,item.gem:20")],
    ))
    .unwrap();
    if let CommerceRecord::DropTable(ref t) = table {
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[0].weight, 80);
    } else {
        panic!("expected drop table");
    }
    let ids = item_ids(&["item.gold", "item.gem"]);
    validate_commerce_cross_record(&table, &ids, &BTreeMap::new()).unwrap();
}

#[test]
fn drop_table_zero_sum_rejected() {
    let error = build_commerce_record(&object(
        "drop.creep",
        "DropTable",
        &[("drop.entries", "item.gold:0")],
    ));
    assert!(error.is_err());
}

#[test]
fn drop_table_malformed_entry_rejected() {
    let error = build_commerce_record(&object(
        "drop.creep",
        "DropTable",
        &[("drop.entries", "item.gold")],
    ));
    assert!(error.is_err());
}
