//! FSV for #821 (item records) + #822 (recipes / shops / creep drop tables), both lowered
//! through the #829 object-data valve.
//!
//! Vertical slice, all through the REAL MCP surface:
//!  * #821: author a custom item FROM a base with full WC3 field overrides; persist through the
//!    object MCP tool into the Aster vault; INDEPENDENT fresh-handle vault readback matches every
//!    authored field; lower through the lower_item valve to a fingerprinted litd/data TOML on
//!    disk; re-read (read_lowered_item) and verify every overridden field + the fingerprint;
//!    prove item-ability refs resolve best-effort (resolved vs external) without failing.
//!  * #822: author Item records + a recipe, a shop, and a creep drop table; lower each through
//!    lower_shop (which runs WHOLE-DOCUMENT cross-record validation) and re-read/verify.
//!
//! Edge cases (before/after state printed):
//!  (1) invalid item value (item.class=Bogus) -> typed rejection, NOTHING persisted;
//!  (2) zero-override item copy preserves base WC3 item semantics, only identity differs;
//!  (3) deterministic re-lowering yields an identical fingerprint;
//!  (4) #822 cross-record FSV: a shop referencing a dangling item id and a recipe cycle both
//!      persist (shape-valid) but FAIL LOUD at lower_shop with typed errors.
//!
//! Negative control: corrupt a lowered artifact on disk and prove the readback rejects it
//! (SerializedFingerprintMismatch).

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, LOWER_ITEM_TOOL, LOWER_SHOP_TOOL, READ_LOWERED_ITEM_TOOL,
        READ_LOWERED_SHOP_TOOL, call_mcp_object_tool, read_mcp_object_document,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use serde_json::{Value, json};

#[path = "fixtures/editor_issue821_822_item_shop_verify.rs"]
mod verify_support;
use verify_support::verify;

struct Ctx {
    document_path: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
}

#[allow(clippy::too_many_arguments)]
fn copy_object(
    ctx: &Ctx,
    seed: Option<EditorDocument>,
    new_object_id: &str,
    rawcode: &str,
    kind: &str,
    base_object_id: &str,
    display_name: &str,
    base_properties: Value,
    overrides: Value,
) -> Result<Value, litd_engine::EngineError> {
    let initial = match seed {
        Some(doc) => serde_json::to_value(doc).unwrap(),
        None => Value::Null,
    };
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": ctx.document_path,
            "ledger_path": ctx.ledger_path,
            "ledger_id": "issue821-822-ledger",
            "request_id": format!("issue821-822-copy-{new_object_id}"),
            "requested_by": "litd-editor-issue821-822-fsv",
            "initial_document": initial,
            "new_object_id": new_object_id,
            "rawcode": rawcode,
            "base": json!({
                "base_object_id": base_object_id,
                "kind": kind,
                "display_name": display_name,
                "properties": base_properties,
            }),
            "overrides": overrides,
        }),
    )
    .map(|out| out.result)
}

fn author_item(ctx: &Ctx, id: &str, rawcode: &str) -> Result<Value, litd_engine::EngineError> {
    copy_object(
        ctx,
        None,
        id,
        rawcode,
        "Item",
        "base.item.blank",
        id,
        json!({ "item.class": "Purchasable", "economy.gold_cost": "50" }),
        json!({}),
    )
}

fn lower_item(ctx: &Ctx, object_id: &str, artifact_path: &std::path::Path) -> Value {
    call_mcp_object_tool(
        LOWER_ITEM_TOOL,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": artifact_path,
            "object_id": object_id,
        }),
    )
    .expect("lower_item MCP tool must succeed")
    .result
}

fn read_lowered_item(artifact_path: &std::path::Path, fingerprint: &str) -> Value {
    call_mcp_object_tool(
        READ_LOWERED_ITEM_TOOL,
        json!({ "artifact_path": artifact_path, "expected_fingerprint": fingerprint }),
    )
    .expect("read_lowered_item MCP tool must succeed")
    .result
}

fn lower_shop_call(
    ctx: &Ctx,
    object_id: &str,
    artifact_path: &std::path::Path,
) -> Result<Value, litd_engine::EngineError> {
    call_mcp_object_tool(
        LOWER_SHOP_TOOL,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": artifact_path,
            "object_id": object_id,
        }),
    )
    .map(|out| out.result)
}

fn item_base_properties() -> Value {
    json!({
        "item.class": "Purchasable",
        "economy.gold_cost": "150",
        "economy.lumber_cost": "0",
        "stock.stock_max": "2",
        "stock.replenish_interval_secs": "30",
        "charges.initial": "1",
        "charges.max": "3",
        "item.inventory_flags": "droppable,sellable,perishable",
        "bonus.hit_points": "40",
        "pickup.drop_on_death": "drop",
        "art.icon": "icons/potion.blp"
    })
}

fn champion_item_overrides() -> Value {
    json!({
        "item.class": "Charged",
        "economy.gold_cost": "250",
        "charges.max": "5",
        "item.inventory_flags": "droppable,pawnable",
        "bonus.strength": "5",
        "bonus.attack": "8",
        "ability.active": "ability.blink",
        "ability.passive": "ability.external.ward"
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue821_822_item_shop_lowering_valve")?;
    let vault = run.artifact_dir("vault")?;
    let ctx = Ctx {
        document_path: vault.join("object-document-aster"),
        ledger_path: vault.join("object-ledger-aster"),
    };

    // 1. Seed doc + author a resolvable ability, the champion item base, and its overrides.
    let seed = EditorDocument::new(
        "calyx://world/issue821-822-object-editor",
        "issue821-822-rev-1",
    )?;
    copy_object(
        &ctx,
        Some(seed),
        "ability.blink",
        "Abl1",
        "Ability",
        "base.ability.blink",
        "Blink",
        json!({ "ability.mana_cost": "25" }),
        json!({}),
    )?;
    copy_object(
        &ctx,
        None,
        "item.custom.champion",
        "iCh1",
        "Item",
        "base.item.potion",
        "Healing Draught",
        item_base_properties(),
        champion_item_overrides(),
    )?;

    // 2. INDEPENDENT vault readback (fresh handle) confirms the persisted, merged item.
    let persisted = read_mcp_object_document(&ctx.document_path)?;
    let champion = persisted
        .document
        .objects
        .get("item.custom.champion")
        .ok_or("champion item missing from independent vault readback")?;
    let vault_class = champion.properties.get("item.class").cloned();
    let vault_gold = champion.properties.get("economy.gold_cost").cloned();
    let vault_flags = champion.properties.get("item.inventory_flags").cloned();
    let vault_icon = champion.properties.get("art.icon").cloned(); // base-carried

    // 3. lower_item -> re-read TOML from disk (verify fields + fingerprint).
    let item_toml = run.artifact_path("lowered/champion.item.toml")?;
    let item_toml_b = run.artifact_path("lowered/champion.redo.item.toml")?;
    let lowered_item = lower_item(&ctx, "item.custom.champion", &item_toml);
    let item_fp = lowered_item["lowered_artifact"]["fingerprint"]
        .as_str()
        .ok_or("lower_item produced no fingerprint")?
        .to_string();
    let ability_res = lowered_item["lowered_artifact"]["ability_ref_resolution"].clone();
    let disk_item = read_lowered_item(&item_toml, &item_fp);
    let item = &disk_item["lowered_artifact"]["item"];
    let iwc3 = &item["wc3"];
    // Edge 3: deterministic re-lowering yields an identical fingerprint.
    let item_fp_again = lower_item(&ctx, "item.custom.champion", &item_toml_b)["lowered_artifact"]
        ["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Edge 1: invalid item enum override rejected, NOTHING persisted.
    let before_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("item.bad");
    let bad = copy_object(
        &ctx,
        None,
        "item.bad",
        "iBd1",
        "Item",
        "base.item.potion",
        "Bad Item",
        item_base_properties(),
        json!({ "item.class": "Bogus" }),
    );
    let after_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("item.bad");

    // Edge 2: zero-override clone preserves base item semantics; only identity differs.
    copy_object(
        &ctx,
        None,
        "item.clone",
        "iCl1",
        "Item",
        "base.item.potion",
        "Cloned Draught",
        item_base_properties(),
        json!({}),
    )?;
    let clone_toml = run.artifact_path("lowered/clone.item.toml")?;
    let clone_lowered = lower_item(&ctx, "item.clone", &clone_toml);
    let clone_fp = clone_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let clone_disk = read_lowered_item(&clone_toml, &clone_fp);
    let clone_wc3 = &clone_disk["lowered_artifact"]["item"]["wc3"];

    // 4. #822: author the item catalogue the commerce records reference.
    for (id, raw) in [
        ("item.hilt", "iHl1"),
        ("item.blade", "iBl1"),
        ("item.sword", "iSw1"),
        ("item.shield", "iSh1"),
        ("item.gold", "iGo1"),
        ("item.gem", "iGe1"),
    ] {
        author_item(&ctx, id, raw)?;
    }
    // Recipe (valid), shop (valid), drop table (valid).
    copy_object(
        &ctx,
        None,
        "recipe.sword",
        "rSw1",
        "Recipe",
        "base.commerce.recipe",
        "Sword Recipe",
        json!({ "recipe.components": "item.hilt,item.blade", "recipe.result": "item.sword" }),
        json!({}),
    )?;
    copy_object(
        &ctx,
        None,
        "shop.arms",
        "sAr1",
        "Shop",
        "base.commerce.shop",
        "Armsmarket",
        json!({
            "shop.stock": "item.sword,item.shield",
            "shop.replenish_interval_secs": "30",
            "shop.stock_max": "5",
            "shop.buy_enabled": "true",
            "shop.sell_enabled": "true"
        }),
        json!({}),
    )?;
    copy_object(
        &ctx,
        None,
        "drop.creep",
        "dCr1",
        "DropTable",
        "base.commerce.drop",
        "Creep Loot",
        json!({ "drop.entries": "item.gold:80,item.gem:20" }),
        json!({}),
    )?;

    let recipe_toml = run.artifact_path("lowered/recipe.sword.commerce.toml")?;
    let shop_toml = run.artifact_path("lowered/shop.arms.commerce.toml")?;
    let drop_toml = run.artifact_path("lowered/drop.creep.commerce.toml")?;
    let recipe_lowered = lower_shop_call(&ctx, "recipe.sword", &recipe_toml)?;
    let shop_lowered = lower_shop_call(&ctx, "shop.arms", &shop_toml)?;
    let drop_lowered = lower_shop_call(&ctx, "drop.creep", &drop_toml)?;
    let recipe_fp = recipe_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let recipe_disk = call_mcp_object_tool(
        READ_LOWERED_SHOP_TOOL,
        json!({ "artifact_path": recipe_toml, "expected_fingerprint": recipe_fp }),
    )?
    .result;

    // Edge 4a: shop referencing a dangling item id persists (shape-valid) but FAILS lowering.
    copy_object(
        &ctx,
        None,
        "shop.dangling",
        "sDa1",
        "Shop",
        "base.commerce.shop",
        "Broken Shop",
        json!({ "shop.stock": "item.sword,item.ghost" }),
        json!({}),
    )?;
    let dangling_persisted = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("shop.dangling");
    let dangling_toml = run.artifact_path("lowered/shop.dangling.commerce.toml")?;
    let dangling_result = lower_shop_call(&ctx, "shop.dangling", &dangling_toml);

    // Edge 4b: two recipes form a cycle (gold<-gem, gem<-gold); lowering FAILS loud.
    copy_object(
        &ctx,
        None,
        "recipe.cyc_a",
        "rCa1",
        "Recipe",
        "base.commerce.recipe",
        "Cycle A",
        json!({ "recipe.components": "item.gem", "recipe.result": "item.gold" }),
        json!({}),
    )?;
    copy_object(
        &ctx,
        None,
        "recipe.cyc_b",
        "rCb1",
        "Recipe",
        "base.commerce.recipe",
        "Cycle B",
        json!({ "recipe.components": "item.gold", "recipe.result": "item.gem" }),
        json!({}),
    )?;
    let cycle_toml = run.artifact_path("lowered/recipe.cyc_a.commerce.toml")?;
    let cycle_result = lower_shop_call(&ctx, "recipe.cyc_a", &cycle_toml);

    // Negative control: corrupt the lowered item artifact; readback MUST reject it.
    let mut corrupt = std::fs::read_to_string(&item_toml)?;
    corrupt = corrupt.replace("gold_cost = 250", "gold_cost = 251");
    let corrupt_path = run.artifact_path("lowered/champion.corrupt.item.toml")?;
    if let Some(parent) = corrupt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&corrupt_path, &corrupt)?;
    let corrupt_rejected = call_mcp_object_tool(
        READ_LOWERED_ITEM_TOOL,
        json!({ "artifact_path": corrupt_path, "expected_fingerprint": item_fp }),
    )
    .is_err();
    let corrupt_changed = corrupt != std::fs::read_to_string(&item_toml)?;

    let report = json!({
        "schema": "litd.issue821_822.item_shop_lowering_valve.v1",
        "issues": [821, 822, 829],
        "source_of_truth": "object MCP tools -> Aster vault -> #829 lowering valve -> litd/data item+commerce TOML re-read from disk",
        "item_happy_path": {
            "champion_fingerprint": item_fp,
            "vault_readback": {
                "class": vault_class,
                "gold_cost": vault_gold,
                "inventory_flags": vault_flags,
                "base_carried_icon": vault_icon,
            },
            "disk_toml": {
                "object_id": item["object_id"],
                "rawcode": item["rawcode"],
                "base_object_id": item["base_object_id"],
                "class": iwc3["classification"]["class"],
                "inventory_flags": iwc3["classification"]["inventory_flags"],
                "gold_cost": iwc3["economy"]["gold_cost"],
                "charges_max": iwc3["charges"]["max"],
                "bonus_strength": iwc3["bonuses"]["strength"],
                "bonus_attack": iwc3["bonuses"]["attack"],
                "ability_active": iwc3["abilities"]["active"],
                "ability_passive": iwc3["abilities"]["passive"],
                "base_carried_icon": iwc3["art"]["icon"],
            },
            "ability_ref_resolution": ability_res,
        },
        "commerce_happy_path": {
            "recipe_fingerprint": recipe_fp,
            "recipe_resolved_refs": recipe_disk["lowered_artifact"]["validation"]["resolved_item_ref_count"],
            "recipe_result": recipe_disk["lowered_artifact"]["commerce"]["recipe"]["result"],
            "recipe_components": recipe_disk["lowered_artifact"]["commerce"]["recipe"]["components"],
            "shop_resolved_refs": shop_lowered["lowered_artifact"]["validation"]["resolved_item_ref_count"],
            "shop_record_kind": shop_lowered["lowered_artifact"]["validation"]["record_kind"],
            "drop_resolved_refs": drop_lowered["lowered_artifact"]["validation"]["resolved_item_ref_count"],
            "drop_record_kind": drop_lowered["lowered_artifact"]["validation"]["record_kind"],
        },
        "edges": {
            "invalid_item_rejected_nothing_persisted": {
                "before_present": before_bad,
                "copy_errored": bad.is_err(),
                "after_present": after_bad,
            },
            "zero_override_identity_only": {
                "clone_object_id": clone_disk["lowered_artifact"]["item"]["object_id"],
                "clone_class": clone_wc3["classification"]["class"],
                "clone_gold_cost": clone_wc3["economy"]["gold_cost"],
                "clone_charges_max": clone_wc3["charges"]["max"],
                "clone_flags": clone_wc3["classification"]["inventory_flags"],
            },
            "deterministic_relowering": {
                "fingerprint_a": item_fp,
                "fingerprint_b": item_fp_again,
            },
            "cross_record_dangling_ref": {
                "shop_persisted": dangling_persisted,
                "lowering_errored": dangling_result.is_err(),
                "error_is_dangling": dangling_result
                    .as_ref()
                    .err()
                    .map(|e| e.to_string().contains("dangling_item_ref"))
                    .unwrap_or(false),
            },
            "cross_record_recipe_cycle": {
                "lowering_errored": cycle_result.is_err(),
                "error_is_cycle": cycle_result
                    .as_ref()
                    .err()
                    .map(|e| e.to_string().contains("recipe_cycle"))
                    .unwrap_or(false),
            },
        },
        "negative_control": {
            "corrupt_written": corrupt_changed,
            "corrupt_readback_rejected": corrupt_rejected,
        },
    });

    verify(&report)?;
    let readback = run.write_report(&report)?;
    let bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&bytes) != readback.sha256 {
        return Err("persisted #821/#822 report hash mismatch".into());
    }
    println!(
        "wrote #821/#822 item+commerce lowering valve FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}
