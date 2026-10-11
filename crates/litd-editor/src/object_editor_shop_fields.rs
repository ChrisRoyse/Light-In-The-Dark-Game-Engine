//! #822 commerce record field sets (recipes, shops, creep drop tables) for the object editor,
//! plus the CROSS-RECORD validation that runs at lowering time: every item ref must resolve to
//! an `Item`-kind record in the same document, recipes may not self-reference or form a cycle,
//! and drop-table weights must sum sensibly. Dangling refs fail loud with typed errors
//! (`object_editor_error` stable field codes) -- proven by the #822 FSV edge cases.
//!
//! Lowering-unit decision (issue #822): recipes + shops + drop-tables are lowered by the ONE
//! `lower_shop`/`read_lowered_shop` tool pair, one artifact per object_id (mirroring the
//! `lower_unit` idiom), because all three are same-track economy records whose validity is
//! defined by cross-referencing the shared Item record set and (for recipes) the shared recipe
//! graph. A single tool + a single cross-record validator over the whole document is the
//! natural unit; splitting into three tool pairs would triplicate the identical
//! document-scoped validation with no added safety.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::EditorObject;

use super::field_extract::{check_identifier, check_range, id_list_field, split_list};
use super::validation::{invalid_object_editor, object_editor_error};

pub const SHOP_STOCK_MAX: u32 = 1_000;
pub const SHOP_INTERVAL_MAX: u32 = 100_000;
pub const DROP_WEIGHT_MAX: u32 = 1_000_000;
pub const DROP_WEIGHT_SUM_MAX: u64 = 100_000_000;
pub const RECIPE_MAX_COMPONENTS: usize = 32;
pub const SHOP_MAX_STOCK_ITEMS: usize = 128;
pub const DROP_MAX_ENTRIES: usize = 128;

/// The authored kinds this commerce lowering unit covers, as object `kind` strings.
pub const COMMERCE_KINDS: &[&str] = &["Recipe", "Shop", "DropTable"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeRecord {
    pub components: Vec<String>,
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopRecord {
    pub stock: Vec<String>,
    pub replenish_interval_secs: u32,
    pub stock_max: u32,
    pub buy_enabled: bool,
    pub sell_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropTableRecord {
    pub entries: Vec<DropEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropEntry {
    pub item_id: String,
    pub weight: u32,
}

/// Internal typed view of a single commerce record used for build + validation dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommerceRecord {
    Recipe(RecipeRecord),
    Shop(ShopRecord),
    DropTable(DropTableRecord),
}

impl CommerceRecord {
    pub fn kind_str(&self) -> &'static str {
        match self {
            CommerceRecord::Recipe(_) => "recipe",
            CommerceRecord::Shop(_) => "shop",
            CommerceRecord::DropTable(_) => "drop_table",
        }
    }
}

pub const COMMERCE_PROPERTY_KEYS: &[&str] = &[
    "recipe.components",
    "recipe.result",
    "shop.stock",
    "shop.replenish_interval_secs",
    "shop.stock_max",
    "shop.buy_enabled",
    "shop.sell_enabled",
    "drop.entries",
];

/// Build a commerce record from an authored object, dispatching on its `kind`.
pub(crate) fn build_commerce_record(object: &EditorObject) -> Result<CommerceRecord, EngineError> {
    match object.kind.as_str() {
        "Recipe" => Ok(CommerceRecord::Recipe(build_recipe(object)?)),
        "Shop" => Ok(CommerceRecord::Shop(build_shop(object)?)),
        "DropTable" => Ok(CommerceRecord::DropTable(build_drop_table(object)?)),
        other => invalid_object_editor(
            "commerce.kind",
            format!("object {} is kind {other} not a commerce record", object.id),
        ),
    }
}

fn build_recipe(object: &EditorObject) -> Result<RecipeRecord, EngineError> {
    let components = id_list_field(object, "recipe.components")?;
    if components.is_empty() {
        return invalid_object_editor(
            "recipe.components",
            "a recipe requires at least one component item id",
        );
    }
    if components.len() > RECIPE_MAX_COMPONENTS {
        return invalid_object_editor(
            "recipe.components",
            format!("component count must be at most {RECIPE_MAX_COMPONENTS}"),
        );
    }
    let result = object
        .properties
        .get("recipe.result")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| object_editor_error("recipe.result", "result item id is required"))?;
    check_identifier("recipe.result", &result)?;
    Ok(RecipeRecord { components, result })
}

fn build_shop(object: &EditorObject) -> Result<ShopRecord, EngineError> {
    use super::field_extract::{bool_field, u32_field};
    let stock = id_list_field(object, "shop.stock")?;
    if stock.is_empty() {
        return invalid_object_editor("shop.stock", "a shop requires at least one stock item id");
    }
    if stock.len() > SHOP_MAX_STOCK_ITEMS {
        return invalid_object_editor(
            "shop.stock",
            format!("stock item count must be at most {SHOP_MAX_STOCK_ITEMS}"),
        );
    }
    Ok(ShopRecord {
        stock,
        replenish_interval_secs: u32_field(
            object,
            "shop.replenish_interval_secs",
            0,
            SHOP_INTERVAL_MAX,
            30,
        )?,
        stock_max: u32_field(object, "shop.stock_max", 0, SHOP_STOCK_MAX, 1)?,
        buy_enabled: bool_field(object, "shop.buy_enabled", true)?,
        sell_enabled: bool_field(object, "shop.sell_enabled", false)?,
    })
}

fn build_drop_table(object: &EditorObject) -> Result<DropTableRecord, EngineError> {
    let raw = object
        .properties
        .get("drop.entries")
        .map(String::as_str)
        .unwrap_or("");
    let entries = parse_drop_entries(raw)?;
    Ok(DropTableRecord { entries })
}

/// Parse `id:weight,id:weight` drop entries; validate id shape, per-weight range, sum, count.
fn parse_drop_entries(raw: &str) -> Result<Vec<DropEntry>, EngineError> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for token in split_list(raw) {
        let (id, weight) = token.split_once(':').ok_or_else(|| {
            object_editor_error(
                "drop.entries",
                format!("entry {token} must be item_id:weight"),
            )
        })?;
        let id = id.trim().to_string();
        check_identifier("drop.entries", &id)?;
        let weight = weight.trim().parse::<u32>().map_err(|_| {
            object_editor_error(
                "drop.entries",
                format!("weight for {id} must be an integer in 1..={DROP_WEIGHT_MAX}"),
            )
        })?;
        check_range("drop.entries", weight, 1, DROP_WEIGHT_MAX)?;
        if !seen.insert(id.clone()) {
            return invalid_object_editor("drop.entries", format!("duplicate drop item {id}"));
        }
        entries.push(DropEntry {
            item_id: id,
            weight,
        });
    }
    if entries.is_empty() {
        return invalid_object_editor(
            "drop.entries",
            "a drop table requires at least one weighted entry",
        );
    }
    if entries.len() > DROP_MAX_ENTRIES {
        return invalid_object_editor(
            "drop.entries",
            format!("drop entry count must be at most {DROP_MAX_ENTRIES}"),
        );
    }
    validate_drop_weight_sum(&entries)?;
    Ok(entries)
}

fn validate_drop_weight_sum(entries: &[DropEntry]) -> Result<(), EngineError> {
    let sum: u64 = entries.iter().map(|entry| u64::from(entry.weight)).sum();
    if sum == 0 || sum > DROP_WEIGHT_SUM_MAX {
        return invalid_object_editor(
            "drop.entries",
            format!("weights must sum to 1..={DROP_WEIGHT_SUM_MAX}; got {sum}"),
        );
    }
    Ok(())
}

/// Re-validate a commerce record's shape on TOML readback (independent of the document).
pub(crate) fn validate_commerce_shape(record: &CommerceRecord) -> Result<(), EngineError> {
    match record {
        CommerceRecord::Recipe(recipe) => {
            if recipe.components.is_empty() || recipe.components.len() > RECIPE_MAX_COMPONENTS {
                return invalid_object_editor("recipe.components", "invalid component count");
            }
            super::field_extract::check_id_list("recipe.components", &recipe.components)?;
            check_identifier("recipe.result", &recipe.result)?;
        }
        CommerceRecord::Shop(shop) => {
            if shop.stock.is_empty() || shop.stock.len() > SHOP_MAX_STOCK_ITEMS {
                return invalid_object_editor("shop.stock", "invalid stock item count");
            }
            super::field_extract::check_id_list("shop.stock", &shop.stock)?;
            check_range("shop.stock_max", shop.stock_max, 0, SHOP_STOCK_MAX)?;
            check_range(
                "shop.replenish_interval_secs",
                shop.replenish_interval_secs,
                0,
                SHOP_INTERVAL_MAX,
            )?;
        }
        CommerceRecord::DropTable(table) => {
            if table.entries.is_empty() || table.entries.len() > DROP_MAX_ENTRIES {
                return invalid_object_editor("drop.entries", "invalid drop entry count");
            }
            let mut seen = BTreeSet::new();
            for entry in &table.entries {
                check_identifier("drop.entries", &entry.item_id)?;
                check_range("drop.entries", entry.weight, 1, DROP_WEIGHT_MAX)?;
                if !seen.insert(entry.item_id.as_str()) {
                    return invalid_object_editor(
                        "drop.entries",
                        format!("duplicate drop item {}", entry.item_id),
                    );
                }
            }
            validate_drop_weight_sum(&table.entries)?;
        }
    }
    Ok(())
}

/// The item refs a commerce record depends on (for cross-record resolution).
pub(crate) fn commerce_item_refs(record: &CommerceRecord) -> Vec<String> {
    match record {
        CommerceRecord::Recipe(recipe) => {
            let mut refs = recipe.components.clone();
            refs.push(recipe.result.clone());
            refs
        }
        CommerceRecord::Shop(shop) => shop.stock.clone(),
        CommerceRecord::DropTable(table) => table
            .entries
            .iter()
            .map(|entry| entry.item_id.clone())
            .collect(),
    }
}

/// CROSS-RECORD validation at lowering (#822): every item ref must resolve to an Item record,
/// and recipes must not self-reference or form a cycle over the shared recipe graph. Returns
/// the count of resolved item refs on success; fails loud with a typed error otherwise.
pub(crate) fn validate_commerce_cross_record(
    record: &CommerceRecord,
    item_ids: &BTreeSet<String>,
    recipe_graph: &BTreeMap<String, Vec<String>>,
) -> Result<usize, EngineError> {
    let refs = commerce_item_refs(record);
    for item_ref in &refs {
        if !item_ids.contains(item_ref) {
            return Err(object_editor_error(
                "commerce.dangling_item_ref",
                format!("item ref {item_ref} does not resolve to an Item record"),
            ));
        }
    }
    if let CommerceRecord::Recipe(recipe) = record {
        if recipe.components.contains(&recipe.result) {
            return Err(object_editor_error(
                "commerce.recipe_self_reference",
                format!(
                    "recipe result {} appears in its own components",
                    recipe.result
                ),
            ));
        }
        if recipe_has_cycle(&recipe.result, recipe_graph) {
            return Err(object_editor_error(
                "commerce.recipe_cycle",
                format!(
                    "recipe result {} participates in a recipe cycle",
                    recipe.result
                ),
            ));
        }
    }
    Ok(refs.len())
}

fn recipe_has_cycle(start: &str, graph: &BTreeMap<String, Vec<String>>) -> bool {
    let mut visited = BTreeSet::new();
    let mut on_stack = BTreeSet::new();
    recipe_dfs(start, graph, &mut visited, &mut on_stack)
}

fn recipe_dfs<'a>(
    node: &'a str,
    graph: &'a BTreeMap<String, Vec<String>>,
    visited: &mut BTreeSet<&'a str>,
    on_stack: &mut BTreeSet<&'a str>,
) -> bool {
    if on_stack.contains(node) {
        return true;
    }
    if visited.contains(node) {
        return false;
    }
    visited.insert(node);
    on_stack.insert(node);
    if let Some(components) = graph.get(node) {
        for component in components {
            if recipe_dfs(component, graph, visited, on_stack) {
                return true;
            }
        }
    }
    on_stack.remove(node);
    false
}

/// Build the recipe graph (result item id -> component item ids) from all Recipe objects.
pub(crate) fn recipe_graph_from_document(
    document: &crate::history::EditorDocument,
) -> BTreeMap<String, Vec<String>> {
    let mut graph = BTreeMap::new();
    for object in document.objects.values() {
        if object.kind != "Recipe" {
            continue;
        }
        if let Ok(CommerceRecord::Recipe(recipe)) = build_commerce_record(object) {
            graph.insert(recipe.result, recipe.components);
        }
    }
    graph
}

/// The set of Item-kind object ids in a document (cross-record resolution target set).
pub(crate) fn item_id_set(document: &crate::history::EditorDocument) -> BTreeSet<String> {
    document
        .objects
        .values()
        .filter(|object| object.kind == "Item")
        .map(|object| object.id.clone())
        .collect()
}

#[cfg(test)]
#[path = "object_editor_shop_fields_tests.rs"]
mod tests;
