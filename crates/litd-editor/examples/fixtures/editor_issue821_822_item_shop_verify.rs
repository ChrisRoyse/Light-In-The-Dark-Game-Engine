//! Report verifier for the #821/#822 item+commerce lowering-valve FSV, split out of the
//! example to hold the <500-line ratchet (#1466/#800 sec.6). Asserts every overridden field
//! reached disk and every edge/negative-control invariant held.

use serde_json::{Value, json};

pub(crate) fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };

    let item = &report["item_happy_path"];
    need(
        item["vault_readback"]["class"] == json!("Charged"),
        "vault readback item class mismatch",
    )?;
    need(
        item["vault_readback"]["gold_cost"] == json!("250"),
        "vault readback item gold_cost mismatch",
    )?;
    need(
        item["vault_readback"]["base_carried_icon"] == json!("icons/potion.blp"),
        "base-carried icon did not survive the copy",
    )?;
    let disk = &item["disk_toml"];
    need(
        disk["class"] == json!("Charged"),
        "disk item class mismatch",
    )?;
    need(
        disk["gold_cost"] == json!(250),
        "disk item gold_cost mismatch",
    )?;
    need(
        disk["charges_max"] == json!(5),
        "disk item charges_max mismatch",
    )?;
    need(
        disk["bonus_strength"] == json!(5),
        "disk item bonus_strength mismatch",
    )?;
    need(
        disk["inventory_flags"] == json!(["droppable", "pawnable"]),
        "disk item inventory_flags mismatch",
    )?;
    need(
        disk["ability_active"] == json!(["ability.blink"]),
        "disk item ability_active mismatch",
    )?;
    need(
        disk["base_carried_icon"] == json!("icons/potion.blp"),
        "base-carried icon did not lower",
    )?;
    // Item-ability refs resolved best-effort: blink resolves, external ward does not (non-fatal).
    let res = &item["ability_ref_resolution"];
    need(
        res["active_resolved"] == json!(1) && res["active_total"] == json!(1),
        "active ability ref did not resolve",
    )?;
    need(
        res["passive_resolved"] == json!(0) && res["passive_total"] == json!(1),
        "external passive ability ref should be unresolved",
    )?;
    need(
        res["unresolved"] == json!(["ability.external.ward"]),
        "unresolved ability ref list mismatch",
    )?;

    let commerce = &report["commerce_happy_path"];
    need(
        commerce["recipe_resolved_refs"] == json!(3),
        "recipe resolved ref count mismatch",
    )?;
    need(
        commerce["recipe_result"] == json!("item.sword"),
        "recipe result mismatch",
    )?;
    need(
        commerce["recipe_components"] == json!(["item.hilt", "item.blade"]),
        "recipe components mismatch",
    )?;
    need(
        commerce["shop_resolved_refs"] == json!(2) && commerce["shop_record_kind"] == json!("shop"),
        "shop lowering mismatch",
    )?;
    need(
        commerce["drop_resolved_refs"] == json!(2)
            && commerce["drop_record_kind"] == json!("drop_table"),
        "drop table lowering mismatch",
    )?;

    let edges = &report["edges"];
    let bad = &edges["invalid_item_rejected_nothing_persisted"];
    need(
        bad["before_present"] == json!(false)
            && bad["copy_errored"] == json!(true)
            && bad["after_present"] == json!(false),
        "invalid item override was NOT rejected without persistence",
    )?;
    let clone = &edges["zero_override_identity_only"];
    need(
        clone["clone_object_id"] == json!("item.clone"),
        "clone identity object_id wrong",
    )?;
    need(
        clone["clone_class"] == json!("Purchasable")
            && clone["clone_gold_cost"] == json!(150)
            && clone["clone_charges_max"] == json!(3)
            && clone["clone_flags"] == json!(["droppable", "sellable", "perishable"]),
        "clone did not preserve base item semantics",
    )?;
    let det = &edges["deterministic_relowering"];
    need(
        det["fingerprint_a"] == det["fingerprint_b"]
            && !det["fingerprint_a"].as_str().unwrap_or_default().is_empty(),
        "re-lowering unchanged item was not deterministic",
    )?;
    let dangling = &edges["cross_record_dangling_ref"];
    need(
        dangling["shop_persisted"] == json!(true)
            && dangling["lowering_errored"] == json!(true)
            && dangling["error_is_dangling"] == json!(true),
        "dangling shop item ref was NOT rejected loud at lowering",
    )?;
    let cycle = &edges["cross_record_recipe_cycle"];
    need(
        cycle["lowering_errored"] == json!(true) && cycle["error_is_cycle"] == json!(true),
        "recipe cycle was NOT rejected loud at lowering",
    )?;

    let neg = &report["negative_control"];
    need(
        neg["corrupt_written"] == json!(true) && neg["corrupt_readback_rejected"] == json!(true),
        "corrupt lowered item artifact was NOT rejected on readback",
    )?;
    Ok(())
}
